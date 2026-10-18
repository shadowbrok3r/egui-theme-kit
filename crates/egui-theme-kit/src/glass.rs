//! Real GPU backdrop blur behind tinted-glass surfaces over `backdrop-blur-egui`'s grab-pass path, degrading to flat fills without GL or the `glass` feature.

use eframe::egui::{
    Align2, Area, AsIdSalt, Color32, Context, CornerRadius, FontId, Frame, Id, InnerResponse,
    LayerId, Order, Rect, Response, Sense, StrokeKind, Ui, UiBuilder, Vec2, pos2,
};
use serde::{Deserialize, Serialize};

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use super::{colors, tokens::Tokens};

const PARAMS_KEY: &str = "egui_theme_kit.theme.glass_params";

/// Environment variable that switches frosting off for the process.
pub const NO_FROST_ENV: &str = "EGUI_THEME_KIT_NO_FROST";

/// Consecutive failed frost frames that trip the kill switch.
const FAILED_FRAMES_BEFORE_DISABLE: u32 = 3;

static DISABLED: AtomicBool = AtomicBool::new(false);
static FAILURE_RUN: FailureRun = FailureRun::new();
static FIRST_COMPOSITE_LOGGED: AtomicBool = AtomicBool::new(false);
static LAST_REPORT: AtomicU8 = AtomicU8::new(REPORT_NONE);
static COMPOSITED_FRAMES: AtomicU64 = AtomicU64::new(0);
static DISABLE_REASON: Mutex<Option<String>> = Mutex::new(None);

const REPORT_NONE: u8 = 0;

fn params_id() -> Id {
    Id::unique(PARAMS_KEY)
}

/// The glass material a theme asks for: how far the backdrop is smeared, the film painted over it, and how present it is.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct GlassParams {
    /// Whether frosting runs at all; off means every [`frost`] call is a no-op.
    pub enabled: bool,
    /// Blur radius in logical points; `0` is a plain tinted pane.
    pub blur_radius: f32,
    /// The glass film painted over the blur; its alpha is the mix between film and blurred backdrop.
    pub tint: Color32,
    /// Corner rounding of the frosted rect, in logical points.
    pub corner_radius: f32,
    /// Surface-global fade in `[0, 1]`; egui's opacity multiplier does not reach paint callbacks.
    pub presence: f32,
}

impl GlassParams {
    /// Themes that paint their own opaque chrome.
    pub const OFF: Self = Self {
        enabled: false,
        blur_radius: 0.0,
        tint: Color32::TRANSPARENT,
        corner_radius: 0.0,
        presence: 1.0,
    };

    /// Whether this material would produce anything visible.
    pub fn is_visible(&self) -> bool {
        self.enabled && self.presence > 0.0 && (self.blur_radius > 0.0 || self.tint.a() > 0)
    }
}

impl Default for GlassParams {
    fn default() -> Self {
        Self::OFF
    }
}

/// Stores the active theme's glass material on `ctx`.
pub fn set_params(ctx: &Context, params: GlassParams) {
    ctx.data_mut(|d| d.insert_temp(params_id(), params));
}

/// The active theme's glass material, [`GlassParams::OFF`] when no theme set one.
pub fn params(ctx: &Context) -> GlassParams {
    ctx.data(|d| d.get_temp::<GlassParams>(params_id()))
        .unwrap_or(GlassParams::OFF)
}

/// Whether [`NO_FROST_ENV`] is set to anything other than an off value.
fn env_disabled() -> bool {
    static ENV: OnceLock<bool> = OnceLock::new();
    *ENV.get_or_init(|| {
        std::env::var(NO_FROST_ENV)
            .is_ok_and(|v| !matches!(v.trim(), "" | "0" | "false" | "off" | "no"))
    })
}

/// Whether frosting is switched off by [`NO_FROST_ENV`], by [`disable`], or by the guard in [`poll_outcome`].
pub fn is_disabled() -> bool {
    env_disabled() || DISABLED.load(Ordering::Relaxed)
}

/// Switches frosting off for the rest of the process; idempotent, logs the first caller's reason.
pub fn disable(reason: &str) {
    if !DISABLED.swap(true, Ordering::Relaxed) {
        if let Ok(mut slot) = DISABLE_REASON.lock() {
            *slot = Some(reason.to_owned());
        }
        log::warn!("backdrop blur disabled: {reason}");
    }
}

/// Clears the kill switch and failure run set by [`disable`] or the failure guard; the environment switch stays.
pub fn reenable() {
    if DISABLED.swap(false, Ordering::Relaxed) {
        log::info!("backdrop blur re-enabled");
    }
    FAILURE_RUN.reset();
    if let Ok(mut slot) = DISABLE_REASON.lock() {
        *slot = None;
    }
}

/// Whether the GPU backend is built, ready, and not switched off.
pub fn is_available() -> bool {
    !is_disabled() && backend::is_available()
}

/// Whether the GPU backend was built, whatever the kill switch says.
pub fn backend_installed() -> bool {
    backend::is_available()
}

/// Builds the grab-pass backend from the host's GL context; returns whether it came up, always false without the `glass` feature.
pub fn install(cc: &eframe::CreationContext<'_>) -> bool {
    backend::install(cc)
}

/// Frees the backend's GL objects; call from `eframe::App::on_exit` while the context is current. Idempotent.
#[cfg(feature = "glass")]
pub fn shutdown(gl: &backdrop_blur_egui::glow::Context) {
    backend::shutdown(gl);
}

/// Enqueues a frost of `rect` with the active material before the surface over it paints; returns whether one was enqueued.
pub fn frost(ui: &Ui, rect: Rect) -> bool {
    frost_with(ui, rect, params(ui.ctx()))
}

/// [`frost`] with an explicit material.
pub fn frost_with(ui: &Ui, rect: Rect, params: GlassParams) -> bool {
    frost_gated(ui, rect, params, backend::frost)
}

/// Calls `enqueue` when frosting is on, `params` is visible and `rect` paints.
fn frost_gated(
    ui: &Ui,
    rect: Rect,
    params: GlassParams,
    enqueue: impl FnOnce(&Ui, Rect, GlassParams) -> bool,
) -> bool {
    if is_disabled() || !params.is_visible() || !rect.is_positive() || !paints(ui, rect) {
        return false;
    }
    enqueue(ui, rect, params)
}

/// Whether a shape added to `ui` over `rect` reaches the painted output.
fn paints(ui: &Ui, rect: Rect) -> bool {
    ui.is_visible()
        && ui.opacity() > 0.0
        && ui.clip_rect().intersect(rect).is_positive()
        && ui.ctx().input(|i| i.viewport().visible().unwrap_or(true))
}

/// Frosts every floating surface open this frame from last frame's area rects; call once per frame from the root `Ui` and get the count.
///
/// Windows frost on a layer behind the app content. Popups, menus, combo boxes and tooltips (small
/// `Foreground` and `Tooltip` surfaces) frost on a layer glued directly beneath each one, so they blur
/// whatever sits under them, windows included. When the blur cannot run (no GL backend, `--no-frost`,
/// the kill switch) such a surface gets an opaque fill from [`Tokens::window`] instead, so text beneath
/// never shows through a translucent glass fill. The returned count is the frosted surfaces.
pub fn frost_open_windows(ctx: &Context) -> usize {
    frost_open_surfaces(ctx, is_available(), &backend::frost)
}

/// [`frost_open_windows`] over an explicit availability and enqueue function.
fn frost_open_surfaces(
    ctx: &Context,
    available: bool,
    enqueue: &dyn Fn(&Ui, Rect, GlassParams) -> bool,
) -> usize {
    let params = params(ctx);
    if !params.is_visible() {
        return 0;
    }
    let mut frosted = 0;
    for (layer, rect) in floating_surfaces(ctx) {
        let done = if is_popup_like(ctx, layer, rect) {
            frost_popup_with(ctx, layer, rect, params, enqueue)
        } else {
            available
                && frost_behind_floating_with(ctx, layer.id, rect, params, |ui, rect, params| {
                    enqueue(ui, rect, params)
                })
        };
        frosted += usize::from(done);
    }
    frosted
}

/// Layer and last-frame rect of every visible floating surface with a positive rect, back to front.
fn floating_surfaces(ctx: &Context) -> Vec<(LayerId, Rect)> {
    ctx.memory(|m| {
        m.layer_ids()
            .filter(|layer| {
                matches!(
                    layer.order,
                    Order::Middle | Order::Foreground | Order::Tooltip
                )
            })
            .filter(|layer| m.areas().is_visible(layer))
            .filter_map(|layer| m.area_rect(layer.id).map(|rect| (layer, rect)))
            .filter(|(_, rect)| rect.is_positive())
            .collect()
    })
}

/// Salt of the layer a popup-like surface gets directly beneath it.
const POPUP_BACKDROP: &str = "egui_theme_kit.glass.popup_backdrop";

/// Share of the content area above which a `Foreground` surface is an overlay veil, not a popup.
const OVERLAY_FRACTION: f32 = 0.85;

/// Whether the surface is a popup, menu, combo list or tooltip rather than a window or a full-screen veil.
fn is_popup_like(ctx: &Context, layer: LayerId, rect: Rect) -> bool {
    matches!(layer.order, Order::Foreground | Order::Tooltip)
        && rect.area() < OVERLAY_FRACTION * ctx.content_rect().area()
}

/// Frosts the surface on a layer of its own order glued directly beneath it, or fills that layer opaque when no frost is enqueued.
fn frost_popup_with(
    ctx: &Context,
    surface: LayerId,
    rect: Rect,
    params: GlassParams,
    enqueue: &dyn Fn(&Ui, Rect, GlassParams) -> bool,
) -> bool {
    let backdrop = LayerId::new(surface.order, surface.id.with(POPUP_BACKDROP));
    ctx.set_sublayer(backdrop, surface);
    let mut ui = Ui::new(
        ctx.clone(),
        backdrop.id.with("ui"),
        UiBuilder::new().layer_id(backdrop).max_rect(rect),
    );
    ui.set_clip_rect(rect);
    if frost_gated(&ui, rect, params, |ui, rect, params| enqueue(ui, rect, params)) {
        return true;
    }
    let radius = ctx.global_style().visuals.menu_corner_radius;
    ui.painter()
        .rect_filled(rect, radius, Tokens::get(ctx).window);
    false
}

/// Frosts `rect` on its own `Order::Background` layer so it grabs the app content behind the surface, through [`frost_gated`] with `enqueue`.
fn frost_behind_floating_with(
    ctx: &Context,
    surface_id: Id,
    rect: Rect,
    params: GlassParams,
    enqueue: impl FnOnce(&Ui, Rect, GlassParams) -> bool,
) -> bool {
    let mut frosted = false;
    Area::new(surface_id.with("egui_theme_kit.glass.backdrop"))
        .order(Order::Background)
        .interactable(false)
        .movable(false)
        .fixed_pos(rect.min)
        .constrain(false)
        .show(ctx, |ui| {
            ui.set_clip_rect(rect);
            frosted = frost_gated(ui, rect, params, enqueue);
        });
    frosted
}

/// Frosts the backdrop behind `frame`, then shows it on top of the blurred result.
pub fn glass_frame<R>(
    ui: &mut Ui,
    id_salt: impl AsIdSalt,
    frame: Frame,
    contents: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<R> {
    let id = ui.scope_id().with(id_salt);
    if let Some(rect) = ui.ctx().data(|d| d.get_temp::<Rect>(id)) {
        frost(ui, rect);
    }
    let inner = frame.show(ui, contents);
    let rect = inner.response.rect;
    ui.ctx().data_mut(|d| d.insert_temp(id, rect));
    inner
}

/// A [`Frame`] shaped for glass: the theme's window stroke and corner radius over a fill weak enough to let the blur read through.
pub fn glass_frame_style(ui: &Ui, params: GlassParams) -> Frame {
    let visuals = ui.visuals();
    let corner_radius = CornerRadius::same(params.corner_radius.round().clamp(0.0, 255.0) as u8);
    let fill = if is_available() && params.is_visible() {
        Color32::TRANSPARENT
    } else {
        visuals.window_fill
    };
    Frame::new()
        .fill(fill)
        .stroke(visuals.window_stroke)
        .corner_radius(corner_radius)
        .inner_margin(6)
}

/// A self-contained sample of the active material: theme-colored blobs with a glass card over them.
pub fn preview(ui: &mut Ui, params: GlassParams) -> Response {
    let height = 108.0;
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().min(560.0), height),
        Sense::hover(),
    );
    let painter = ui.painter().with_clip_rect(rect);
    let visuals = ui.visuals().clone();
    painter.rect_filled(rect, CornerRadius::same(6), visuals.extreme_bg_color);

    let blobs = [
        colors::info(ui),
        visuals.warn_fg_color,
        colors::success(ui),
        visuals.error_fg_color,
    ];
    for (i, color) in blobs.iter().enumerate() {
        let t = (i as f32 + 0.5) / blobs.len() as f32;
        painter.circle_filled(
            pos2(rect.left() + rect.width() * t, rect.center().y),
            height * 0.40,
            *color,
        );
    }

    let card = Rect::from_center_size(rect.center(), Vec2::new(rect.width() * 0.62, height * 0.58));
    let frosted = frost_with(ui, card, params);
    let corner = CornerRadius::same(params.corner_radius.round().clamp(0.0, 255.0) as u8);
    painter.rect(
        card,
        corner,
        visuals.window_fill,
        visuals.window_stroke,
        StrokeKind::Inside,
    );
    let label = if frosted {
        "GPU backdrop blur"
    } else if is_available() {
        "blur off - tint only"
    } else {
        "no GPU blur on this host"
    };
    painter.text(
        card.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(12.0),
        visuals.text_color(),
    );
    response
}

/// Reads and clears the last painted frame's strongest frost result; call at the start of every pass.
pub fn poll_outcome(ctx: &Context) -> Option<FrostReport> {
    if ctx.current_pass_index() > 0 {
        backend::discard_pass_enqueues();
        return None;
    }
    let report = backend::poll_outcome()?;
    LAST_REPORT.store(report.code(), Ordering::Relaxed);
    if FAILURE_RUN.record(report) {
        disable("grab-pass frost failed repeatedly; surfaces now paint unfrosted");
    }
    if report == FrostReport::Composited {
        COMPOSITED_FRAMES.fetch_add(1, Ordering::Relaxed);
        if !FIRST_COMPOSITE_LOGGED.swap(true, Ordering::Relaxed) {
            log::info!("backdrop blur composited its first surface");
        }
    }
    Some(report)
}

/// Consecutive frames whose frosts failed.
struct FailureRun(AtomicU32);

impl FailureRun {
    const fn new() -> Self {
        Self(AtomicU32::new(0))
    }

    /// Counts a failed frame, resets on a frame that painted or clipped, ignores one that did not fire; true when the run reaches the kill threshold.
    fn record(&self, report: FrostReport) -> bool {
        match report {
            FrostReport::Failed => {
                let failures = self.0.fetch_add(1, Ordering::Relaxed) + 1;
                log::warn!("backdrop blur frost failed on {failures} consecutive frame(s)");
                failures >= FAILED_FRAMES_BEFORE_DISABLE
            }
            FrostReport::Composited | FrostReport::ClippedEmpty => {
                self.reset();
                false
            }
            FrostReport::DidNotFire => false,
        }
    }

    fn reset(&self) {
        self.0.store(0, Ordering::Relaxed);
    }

    fn length(&self) -> u32 {
        self.0.load(Ordering::Relaxed)
    }
}

/// What the frosts of one frame achieved, mirroring `backdrop_blur_egui::FrostOutcome`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrostReport {
    /// No callback ran: nothing enqueued, or every frosted rect was fully clipped.
    DidNotFire,
    /// A callback ran but its region clipped to nothing; a valid no-op.
    ClippedEmpty,
    /// A frost errored or panicked; details are in the backend's throttled warning.
    Failed,
    /// At least one surface was blurred and composited.
    Composited,
}

impl FrostReport {
    fn code(self) -> u8 {
        match self {
            Self::DidNotFire => 1,
            Self::ClippedEmpty => 2,
            Self::Failed => 3,
            Self::Composited => 4,
        }
    }

    fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::DidNotFire),
            2 => Some(Self::ClippedEmpty),
            3 => Some(Self::Failed),
            4 => Some(Self::Composited),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::DidNotFire => "did not fire",
            Self::ClippedEmpty => "clipped empty",
            Self::Failed => "failed",
            Self::Composited => "composited",
        }
    }
}

/// Snapshot of the frost pipeline for the debug readout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrostStatus {
    /// The grab-pass backend was built against the host GL context.
    pub backend_installed: bool,
    /// Frosting is switched off by the environment, `--no-frost`, or the failure guard.
    pub disabled: bool,
    /// The environment switch is set.
    pub env_disabled: bool,
    /// Why the kill switch tripped, when it did.
    pub reason: Option<String>,
    /// Consecutive frames the last polls reported a failure.
    pub consecutive_failures: u32,
    /// Frames that composited at least one surface.
    pub composited_frames: u64,
    /// The strongest result of the last polled frame.
    pub last: Option<FrostReport>,
}

/// The current frost pipeline status.
pub fn status() -> FrostStatus {
    FrostStatus {
        backend_installed: backend::is_available(),
        disabled: is_disabled(),
        env_disabled: env_disabled(),
        reason: DISABLE_REASON.lock().ok().and_then(|r| r.clone()),
        consecutive_failures: FAILURE_RUN.length(),
        composited_frames: COMPOSITED_FRAMES.load(Ordering::Relaxed),
        last: FrostReport::from_code(LAST_REPORT.load(Ordering::Relaxed)),
    }
}

impl FrostStatus {
    /// One-line summary of the pipeline state.
    pub fn summary(&self) -> String {
        if self.env_disabled {
            format!("off ({NO_FROST_ENV} is set)")
        } else if self.disabled {
            format!(
                "off ({})",
                self.reason.as_deref().unwrap_or("kill switch tripped")
            )
        } else if !self.backend_installed {
            "unavailable (no GL backend on this host)".to_owned()
        } else {
            match self.last {
                Some(last) => format!("ready, last frame {}", last.label()),
                None => "ready".to_owned(),
            }
        }
    }
}

#[cfg(not(feature = "glass"))]
mod backend {
    use super::{FrostReport, GlassParams};
    use eframe::egui::{Rect, Ui};

    pub(super) fn is_available() -> bool {
        false
    }

    pub(super) fn install(_: &eframe::CreationContext<'_>) -> bool {
        log::info!("backdrop blur unavailable: built without the `glass` feature");
        false
    }

    pub(super) fn frost(_: &Ui, _: Rect, _: GlassParams) -> bool {
        false
    }

    pub(super) fn discard_pass_enqueues() {}

    pub(super) fn poll_outcome() -> Option<FrostReport> {
        None
    }
}

#[cfg(feature = "glass")]
mod backend {
    use super::{FrostReport, GlassParams};
    use backdrop_blur_egui::{
        BlurRadius, CornerRadius, FrostOutcome, GrabPassRenderer, Presence, RepaintPolicy, Surface,
        Tint,
    };
    use eframe::egui::{Rect, Ui};
    use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
    use std::sync::{Arc, RwLock};

    /// Polls with frosts enqueued, since the last fired callback, before the wiring warning.
    const SILENT_POLLS_BEFORE_WARN: u32 = 3;

    /// One GL context per process; `on_exit` hands back the context without an egui `Context`.
    static RENDERER: RwLock<Option<Arc<GrabPassRenderer>>> = RwLock::new(None);
    /// Frosts enqueued since the last poll.
    static ENQUEUED: AtomicUsize = AtomicUsize::new(0);
    /// Polls with frosts enqueued since the last fired callback.
    static SILENT_POLLS: AtomicU32 = AtomicU32::new(0);
    /// Latches the "enqueued but never fired" warning to once per process.
    static WIRING_WARNED: AtomicBool = AtomicBool::new(false);

    fn renderer() -> Option<Arc<GrabPassRenderer>> {
        RENDERER.read().ok()?.clone()
    }

    pub(super) fn is_available() -> bool {
        RENDERER.read().is_ok_and(|r| r.is_some())
    }

    pub(super) fn install(cc: &eframe::CreationContext<'_>) -> bool {
        if is_available() {
            return true;
        }
        let Some(gl) = cc.gl.as_ref() else {
            log::info!("backdrop blur unavailable: eframe is not running the glow backend");
            return false;
        };
        log_gl_identity(gl);
        if super::is_disabled() {
            log::info!("backdrop blur not installed: frosting is switched off");
            return false;
        }
        match GrabPassRenderer::new(gl) {
            Ok(renderer) => {
                match RENDERER.write() {
                    Ok(mut slot) => *slot = Some(Arc::new(renderer)),
                    Err(e) => {
                        log::error!("backdrop blur renderer slot poisoned: {e}");
                        return false;
                    }
                }
                log::debug!("backdrop blur ready (grab-pass over glow)");
                true
            }
            Err(e) => {
                log::warn!("backdrop blur unavailable: {e}");
                false
            }
        }
    }

    /// Records which driver and adapter eframe's GL context landed on.
    fn log_gl_identity(gl: &backdrop_blur_egui::glow::Context) {
        use backdrop_blur_egui::glow::{self, HasContext};
        let read = |name: u32| unsafe { gl.get_parameter_string(name) };
        log::info!(
            "GL context: vendor={:?} renderer={:?} version={:?} glsl={:?}",
            read(glow::VENDOR),
            read(glow::RENDERER),
            read(glow::VERSION),
            read(glow::SHADING_LANGUAGE_VERSION),
        );
    }

    pub(super) fn shutdown(gl: &backdrop_blur_egui::glow::Context) {
        let taken = RENDERER.write().ok().and_then(|mut slot| slot.take());
        if let Some(renderer) = taken {
            renderer.destroy(gl);
        }
    }

    pub(super) fn frost(ui: &Ui, rect: Rect, params: GlassParams) -> bool {
        let Some(renderer) = renderer() else {
            return false;
        };
        renderer.frost(
            ui,
            Surface {
                rect,
                blur_radius: BlurRadius::new(params.blur_radius),
                tint: Tint::from_srgb_unmultiplied(params.tint.to_srgba_unmultiplied()),
                corner_radius: CornerRadius::new(params.corner_radius),
                presence: Presence::new(params.presence),
                repaint: RepaintPolicy::Static,
            },
        );
        ENQUEUED.fetch_add(1, Ordering::Relaxed);
        true
    }

    pub(super) fn discard_pass_enqueues() {
        ENQUEUED.store(0, Ordering::Relaxed);
    }

    /// Advances the silent-poll run; true on the poll that reaches the warning threshold.
    fn note_silent_poll(streak: &AtomicU32, enqueued: usize, outcome: FrostOutcome) -> bool {
        match outcome {
            FrostOutcome::DidNotFire if enqueued > 0 => {
                streak.fetch_add(1, Ordering::Relaxed) + 1 == SILENT_POLLS_BEFORE_WARN
            }
            FrostOutcome::DidNotFire => false,
            FrostOutcome::ClippedEmpty | FrostOutcome::Failed | FrostOutcome::Composited => {
                streak.store(0, Ordering::Relaxed);
                false
            }
        }
    }

    pub(super) fn poll_outcome() -> Option<FrostReport> {
        let renderer = renderer()?;
        let enqueued = ENQUEUED.swap(0, Ordering::Relaxed);
        let outcome = renderer.take_frost_outcome();
        if note_silent_poll(&SILENT_POLLS, enqueued, outcome)
            && !WIRING_WARNED.swap(true, Ordering::Relaxed)
        {
            log::warn!(
                "backdrop blur enqueued frosts on {SILENT_POLLS_BEFORE_WARN} frames with no paint \
                 callback run in between ({enqueued} surface(s) on the last)"
            );
        }
        Some(match outcome {
            FrostOutcome::DidNotFire => FrostReport::DidNotFire,
            FrostOutcome::ClippedEmpty => FrostReport::ClippedEmpty,
            FrostOutcome::Failed => FrostReport::Failed,
            FrostOutcome::Composited => FrostReport::Composited,
        })
    }

    #[cfg(test)]
    mod tests {
        use super::super::tests::{add_callback, callbacks, glass, input};
        use super::super::{frost_behind_floating_with, poll_outcome};
        use super::*;
        use eframe::egui::{Context, Id, pos2, vec2};

        #[test]
        fn the_wiring_warning_waits_for_three_silent_polls() {
            let streak = AtomicU32::new(0);
            let silent = |s: &AtomicU32| note_silent_poll(s, 1, FrostOutcome::DidNotFire);
            assert!(!silent(&streak));
            assert!(!silent(&streak));
            assert!(silent(&streak));
            assert!(!silent(&streak));

            for fired in [
                FrostOutcome::ClippedEmpty,
                FrostOutcome::Failed,
                FrostOutcome::Composited,
            ] {
                streak.store(0, Ordering::Relaxed);
                assert!(!silent(&streak));
                assert!(!silent(&streak));
                assert!(!note_silent_poll(&streak, 1, fired));
                assert!(!silent(&streak));
                assert!(!silent(&streak));
                assert!(silent(&streak), "{fired:?} did not reset the run");
            }

            streak.store(0, Ordering::Relaxed);
            assert!(!silent(&streak));
            assert!(!silent(&streak));
            assert!(!note_silent_poll(&streak, 0, FrostOutcome::DidNotFire));
            assert!(silent(&streak));
        }

        #[test]
        fn a_later_pass_drops_the_enqueues_of_the_discarded_one() {
            let ctx = Context::default();
            let rect = Rect::from_min_size(pos2(10.0, 10.0), vec2(50.0, 50.0));
            let enqueue = |ui: &Ui, rect: Rect, _: GlassParams| {
                add_callback(ui, rect);
                ENQUEUED.fetch_add(1, Ordering::Relaxed);
                true
            };
            let frost = |ui: &Ui| {
                frost_behind_floating_with(ui.ctx(), Id::unique("surface"), rect, glass(), enqueue)
            };
            let mut warm = ctx.run_ui(input(), |ui| {
                frost(ui);
            });
            warm.textures_delta.clear();
            ENQUEUED.store(0, Ordering::Relaxed);

            let mut full = ctx.run_ui(input(), |ui| {
                poll_outcome(ui.ctx());
                frost(ui);
                if ui.ctx().current_pass_index() == 0 {
                    ui.ctx().request_discard("test");
                }
            });
            full.textures_delta.clear();
            assert_eq!(full.platform_output.num_completed_passes, 2);
            assert_eq!(callbacks(&full), 1);
            assert_eq!(ENQUEUED.swap(0, Ordering::Relaxed), 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{PaintCallback, RawInput, Shape, ViewportId, vec2};
    use std::sync::Arc;

    #[test]
    fn off_is_never_visible_and_enabled_needs_a_dial_turned_up() {
        assert!(!GlassParams::OFF.is_visible());

        let mut params = GlassParams {
            enabled: true,
            blur_radius: 0.0,
            tint: Color32::TRANSPARENT,
            corner_radius: 8.0,
            presence: 1.0,
        };
        assert!(!params.is_visible());
        params.blur_radius = 18.0;
        assert!(params.is_visible());
        params.presence = 0.0;
        assert!(!params.is_visible());
    }

    #[test]
    fn a_pure_film_with_no_blur_is_still_visible() {
        let params = GlassParams {
            enabled: true,
            blur_radius: 0.0,
            tint: Color32::from_rgba_unmultiplied(20, 18, 40, 90),
            ..GlassParams::OFF
        };
        assert!(params.is_visible());
    }

    #[test]
    fn params_round_trip_through_the_context() {
        let ctx = Context::default();
        assert_eq!(params(&ctx), GlassParams::OFF);

        let applied = GlassParams {
            enabled: true,
            blur_radius: 24.0,
            tint: Color32::from_rgba_unmultiplied(12, 10, 24, 110),
            corner_radius: 10.0,
            presence: 0.9,
        };
        set_params(&ctx, applied);
        assert_eq!(params(&ctx), applied);
    }

    #[test]
    fn three_failed_frames_in_a_row_trip_the_kill_switch() {
        let run = FailureRun::new();
        assert!(!run.record(FrostReport::Failed));
        assert!(!run.record(FrostReport::Failed));
        assert!(run.record(FrostReport::Failed));
        assert_eq!(run.length(), 3);
    }

    #[test]
    fn a_painted_or_clipped_frame_resets_the_failed_run_and_a_silent_one_keeps_it() {
        for reset in [FrostReport::Composited, FrostReport::ClippedEmpty] {
            let run = FailureRun::new();
            run.record(FrostReport::Failed);
            run.record(FrostReport::Failed);
            assert!(!run.record(reset));
            assert_eq!(run.length(), 0, "{reset:?}");
            assert!(!run.record(FrostReport::Failed));
        }
        let run = FailureRun::new();
        run.record(FrostReport::Failed);
        assert!(!run.record(FrostReport::DidNotFire));
        assert_eq!(run.length(), 1);
    }

    #[test]
    fn report_codes_round_trip() {
        for report in [
            FrostReport::DidNotFire,
            FrostReport::ClippedEmpty,
            FrostReport::Failed,
            FrostReport::Composited,
        ] {
            assert_eq!(FrostReport::from_code(report.code()), Some(report));
        }
        assert_eq!(FrostReport::from_code(REPORT_NONE), None);
    }

    #[test]
    fn the_status_summary_names_the_state() {
        let mut status = FrostStatus {
            backend_installed: false,
            disabled: false,
            env_disabled: false,
            reason: None,
            consecutive_failures: 0,
            composited_frames: 0,
            last: None,
        };
        assert!(status.summary().starts_with("unavailable"));
        status.backend_installed = true;
        assert_eq!(status.summary(), "ready");
        status.last = Some(FrostReport::Composited);
        assert_eq!(status.summary(), "ready, last frame composited");
        status.disabled = true;
        status.reason = Some("--no-frost".to_owned());
        assert_eq!(status.summary(), "off (--no-frost)");
        status.env_disabled = true;
        assert!(status.summary().contains(NO_FROST_ENV));
    }

    pub(super) fn input() -> RawInput {
        RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0))),
            ..Default::default()
        }
    }

    /// A material that frosts.
    pub(super) fn glass() -> GlassParams {
        GlassParams {
            enabled: true,
            blur_radius: 8.0,
            ..GlassParams::OFF
        }
    }

    /// Adds a stand-in frost callback over `rect`.
    pub(super) fn add_callback(ui: &Ui, rect: Rect) {
        ui.painter().add(PaintCallback {
            rect,
            callback: Arc::new(()),
        });
    }

    /// Paint callbacks in a frame's output.
    #[cfg(feature = "glass")]
    pub(super) fn callbacks(full: &eframe::egui::FullOutput) -> usize {
        full.shapes
            .iter()
            .filter(|s| matches!(s.shape, Shape::Callback(_)))
            .count()
    }

    /// [`frost_behind_floating_with`] a stand-in callback; adds each enqueue to `count`.
    fn frost_stand_in(ctx: &Context, surface: Id, rect: Rect, count: &mut usize) {
        frost_behind_floating_with(ctx, surface, rect, glass(), |ui, rect, _| {
            add_callback(ui, rect);
            *count += 1;
            true
        });
    }

    /// Whether [`frost_gated`] calls its enqueue for `rect`.
    fn gate_opens(ui: &Ui, rect: Rect) -> bool {
        frost_gated(ui, rect, glass(), |_, _, _| true)
    }

    /// One frame of the backdrop frost over `rect`; returns the enqueues and the callback clips.
    fn backdrop_frame(ctx: &Context, rect: Rect) -> (usize, Vec<Rect>) {
        let mut enqueued = 0;
        let mut full = ctx.run_ui(input(), |ui| {
            frost_stand_in(ui.ctx(), Id::unique("surface"), rect, &mut enqueued);
        });
        full.textures_delta.clear();
        let clips = full
            .shapes
            .iter()
            .filter(|s| matches!(s.shape, Shape::Callback(_)))
            .map(|s| s.clip_rect)
            .collect();
        (enqueued, clips)
    }

    #[test]
    fn a_backdrop_area_paints_clipped_to_its_surface_from_its_second_show() {
        let ctx = Context::default();
        let rect = Rect::from_min_size(pos2(100.0, 100.0), vec2(300.0, 80.0));
        assert_eq!(backdrop_frame(&ctx, rect), (0, vec![]));
        assert_eq!(backdrop_frame(&ctx, rect), (1, vec![rect]));
        assert_eq!(backdrop_frame(&ctx, rect), (1, vec![rect]));
    }

    #[test]
    fn a_frost_after_a_discard_request_does_not_paint() {
        let ctx = Context::default();
        let mut seen = Vec::new();
        let mut full = ctx.run_ui(input(), |ui| {
            let rect = ui.max_rect();
            let before = gate_opens(ui, rect);
            if ui.ctx().current_pass_index() == 0 {
                ui.ctx().request_discard("test");
            }
            seen.push((before, gate_opens(ui, rect)));
        });
        full.textures_delta.clear();
        assert_eq!(seen, [(true, false), (true, true)]);
    }

    #[test]
    fn a_frost_outside_the_clip_or_faded_out_does_not_paint() {
        let ctx = Context::default();
        let mut seen = None;
        let mut full = ctx.run_ui(input(), |ui| {
            ui.set_clip_rect(Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 100.0)));
            let inside = Rect::from_min_size(pos2(50.0, 50.0), vec2(100.0, 100.0));
            let touching = Rect::from_min_size(pos2(100.0, 0.0), vec2(100.0, 100.0));
            let clipped = (gate_opens(ui, inside), gate_opens(ui, touching));
            ui.set_opacity(0.0);
            seen = Some((clipped, gate_opens(ui, inside)));
        });
        full.textures_delta.clear();
        assert_eq!(seen, Some(((true, false), false)));
    }

    #[test]
    fn a_frost_in_a_minimized_viewport_does_not_paint() {
        let ctx = Context::default();
        let mut seen = Vec::new();
        for minimized in [false, true] {
            let mut raw = input();
            raw.viewports.entry(ViewportId::ROOT).or_default().minimized = Some(minimized);
            let mut full = ctx.run_ui(raw, |ui| seen.push(gate_opens(ui, ui.max_rect())));
            full.textures_delta.clear();
        }
        assert_eq!(seen, [true, false]);
    }

    // ---- popups, menus and tooltips -------------------------------------------------------

    use super::super::{
        fonts::{self, TextScale},
        presets::{self, apply_preset, builtin_presets},
    };
    use eframe::egui::{CentralPanel, FullOutput, Popup, PopupKind, Window, epaint::RectShape};

    /// What a frame painted, in draw order.
    #[derive(Default)]
    struct Painted {
        /// Text shapes with their index and bounds.
        texts: Vec<(usize, String, Rect)>,
        /// Filled rectangles with their index, bounds and fill.
        rects: Vec<(usize, Rect, Color32)>,
        /// Paint callbacks with their index and bounds.
        callbacks: Vec<(usize, Rect)>,
    }

    fn painted(full: &FullOutput) -> Painted {
        fn walk(shape: &Shape, index: usize, out: &mut Painted) {
            match shape {
                Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, index, out)),
                Shape::Text(text) => out.texts.push((
                    index,
                    text.galley.text().to_owned(),
                    text.galley.rect.translate(text.pos.to_vec2()),
                )),
                Shape::Rect(RectShape { rect, fill, .. }) if *fill != Color32::TRANSPARENT => {
                    out.rects.push((index, *rect, *fill));
                }
                Shape::Callback(callback) => out.callbacks.push((index, callback.rect)),
                _ => {}
            }
        }
        let mut out = Painted::default();
        for (index, clipped) in full.shapes.iter().enumerate() {
            walk(&clipped.shape, index, &mut out);
        }
        out
    }

    fn themed_context() -> Context {
        let ctx = Context::default();
        ctx.set_fonts(fonts::definitions(TextScale::Normal));
        presets::bootstrap_startup_theme(&ctx);
        ctx
    }

    /// One frame: app text, a window with text and, when `open`, a popup of `kind` anchored in the window.
    fn scene(
        ctx: &Context,
        open: Option<PopupKind>,
        frost: &dyn Fn(&Context) -> usize,
    ) -> FullOutput {
        ctx.run_ui(input(), |ui| {
            frost(ui.ctx());
            CentralPanel::default().show(ui, |ui| {
                for i in 0..30 {
                    ui.label(format!("BEHIND panel line {i}"));
                }
            });
            Window::new("host")
                .fixed_pos(pos2(40.0, 40.0))
                .show(ui.ctx(), |ui| {
                    let anchor = ui.button("anchor");
                    for i in 0..8 {
                        ui.label(format!("BEHIND window line {i} with extra words"));
                    }
                    if let Some(kind) = open {
                        Popup::from_response(&anchor)
                            .kind(kind)
                            .open(true)
                            .show(|ui| {
                                ui.label("popup item one");
                                ui.label("popup item two");
                            });
                    }
                });
        })
    }

    /// Runs `frames` frames and returns the last one's paint and the popup's rect.
    fn settle(
        ctx: &Context,
        open: Option<PopupKind>,
        frost: &dyn Fn(&Context) -> usize,
    ) -> (Painted, Option<Rect>) {
        let mut full = scene(ctx, open, frost);
        for _ in 0..4 {
            full.textures_delta.clear();
            full = scene(ctx, open, frost);
        }
        full.textures_delta.clear();
        let popup = ctx.memory(|m| {
            m.layer_ids()
                .filter(|l| matches!(l.order, Order::Foreground | Order::Tooltip))
                .filter(|l| m.areas().is_visible(l))
                .find_map(|l| m.area_rect(l.id))
        });
        (painted(&full), popup)
    }

    fn near(a: Rect, b: Rect) -> bool {
        (a.min - b.min).abs().max_elem() < 0.5 && (a.max - b.max).abs().max_elem() < 0.5
    }

    /// Indices of the filled rectangles that coincide with `popup`, back to front.
    fn fills_at(painted: &Painted, popup: Rect) -> Vec<(usize, Color32)> {
        painted
            .rects
            .iter()
            .filter(|(_, rect, _)| near(*rect, popup))
            .map(|(index, _, fill)| (*index, *fill))
            .collect()
    }

    /// Indices of the app text that lies under `popup`.
    fn text_under(painted: &Painted, popup: Rect) -> Vec<usize> {
        painted
            .texts
            .iter()
            .filter(|(_, text, rect)| text.starts_with("BEHIND") && rect.intersects(popup))
            .map(|(index, ..)| *index)
            .collect()
    }

    fn unfrosted(ctx: &Context) -> usize {
        frost_open_surfaces(ctx, false, &|_, _, _| false)
    }

    fn glass_presets() -> Vec<presets::PresetDef> {
        builtin_presets()
            .into_iter()
            .filter(|def| {
                let ctx = Context::default();
                apply_preset(&ctx, *def);
                params(&ctx).is_visible()
            })
            .collect()
    }

    #[test]
    fn every_glass_preset_paints_popups_over_an_opaque_fill_when_the_blur_cannot_run() {
        let presets = glass_presets();
        assert!(presets.len() >= 12, "the glass presets are exercised");
        for def in presets {
            for kind in [PopupKind::Popup, PopupKind::Menu, PopupKind::Tooltip] {
                let ctx = themed_context();
                apply_preset(&ctx, def);
                let (paint, popup) = settle(&ctx, Some(kind), &unfrosted);
                let popup = popup.unwrap_or_else(|| panic!("{} {kind:?}: no popup", def.id));
                let under = text_under(&paint, popup);
                assert!(!under.is_empty(), "{} {kind:?}: nothing under the popup", def.id);

                let base = Tokens::get(&ctx).window;
                assert_eq!(base.a(), 255);
                let fills = fills_at(&paint, popup);
                let base_at = fills
                    .iter()
                    .find(|(_, fill)| *fill == base)
                    .map(|(index, _)| *index)
                    .unwrap_or_else(|| panic!("{} {kind:?}: no opaque base in {fills:?}", def.id));
                let fill_at = fills.last().map(|(index, _)| *index).unwrap();
                assert!(base_at < fill_at, "{} {kind:?}: base under the popup fill", def.id);
                assert!(
                    under.iter().all(|index| *index < base_at),
                    "{} {kind:?}: text drawn after the base shows through",
                    def.id
                );
            }
        }
    }

    #[test]
    fn a_popup_is_frosted_on_a_layer_between_the_windows_and_the_popup() {
        let stand_in = |ui: &Ui, rect: Rect, _: GlassParams| {
            add_callback(ui, rect);
            true
        };
        let frosted = |ctx: &Context| frost_open_surfaces(ctx, true, &stand_in);
        let ctx = themed_context();
        apply_preset(&ctx, presets::preset("frost").expect("built in"));
        let (paint, popup) = settle(&ctx, Some(PopupKind::Popup), &frosted);
        let popup = popup.expect("a popup is open");
        let fill_at = fills_at(&paint, popup)
            .last()
            .map(|(index, _)| *index)
            .expect("the popup fill");
        let (frost_at, _) = paint
            .callbacks
            .iter()
            .find(|(_, rect)| near(*rect, popup))
            .map(|(index, rect)| (*index, *rect))
            .expect("a frost over the popup");
        assert!(frost_at < fill_at, "the frost is under the popup fill");
        let under = text_under(&paint, popup);
        assert!(!under.is_empty());
        assert!(
            under.iter().all(|index| *index < frost_at),
            "the frost grabs the app and window text beneath the popup"
        );
        let base = Tokens::get(&ctx).window;
        assert!(
            !fills_at(&paint, popup).iter().any(|(_, fill)| *fill == base),
            "a frosted popup needs no opaque base"
        );
        assert_eq!(paint.callbacks.len(), 2, "the window and the popup frost");
    }

    #[test]
    fn a_failed_enqueue_falls_back_to_the_opaque_base() {
        let ctx = themed_context();
        apply_preset(&ctx, presets::preset("frost").expect("built in"));
        let refuses = |ctx: &Context| frost_open_surfaces(ctx, true, &|_, _, _| false);
        let (paint, popup) = settle(&ctx, Some(PopupKind::Popup), &refuses);
        let popup = popup.expect("a popup is open");
        let base = Tokens::get(&ctx).window;
        assert!(fills_at(&paint, popup).iter().any(|(_, fill)| *fill == base));
        assert!(paint.callbacks.is_empty());
    }

    #[test]
    fn a_preset_without_glass_adds_no_popup_layers() {
        let flat = builtin_presets()
            .into_iter()
            .find(|def| {
                let ctx = Context::default();
                apply_preset(&ctx, *def);
                !params(&ctx).is_visible()
            })
            .expect("a preset without glass ships");
        let ctx = themed_context();
        apply_preset(&ctx, flat);
        let (paint, popup) = settle(&ctx, Some(PopupKind::Popup), &unfrosted);
        let popup = popup.expect("a popup is open");
        assert_eq!(fills_at(&paint, popup).len(), 1, "only the popup's own fill, no backdrop");
        assert_eq!(unfrosted(&ctx), 0);
    }

    #[test]
    fn a_full_screen_veil_is_not_taken_for_a_popup() {
        let ctx = themed_context();
        apply_preset(&ctx, presets::preset("frost").expect("built in"));
        let veil = |open: bool| {
            let mut full = ctx.run_ui(input(), |ui| {
                unfrosted(ui.ctx());
                if open {
                    Area::new(Id::unique("test.veil"))
                        .order(Order::Foreground)
                        .fixed_pos(pos2(0.0, 0.0))
                        .show(ui.ctx(), |ui| {
                            ui.allocate_space(vec2(1280.0, 800.0));
                        });
                }
            });
            full.textures_delta.clear();
            painted(&full)
        };
        let base = Tokens::get(&ctx).window;
        for _ in 0..4 {
            let paint = veil(true);
            assert!(
                !paint
                    .rects
                    .iter()
                    .any(|(_, rect, fill)| *fill == base && rect.area() > 1_000_000.0),
                "the veil stays see-through"
            );
        }
    }

    #[test]
    fn the_backdrop_of_a_closed_popup_lasts_one_frame() {
        let ctx = themed_context();
        apply_preset(&ctx, presets::preset("frost").expect("built in"));
        let (_, popup) = settle(&ctx, Some(PopupKind::Popup), &unfrosted);
        let popup = popup.expect("a popup is open");
        let base = Tokens::get(&ctx).window;
        let mut gone = false;
        for _ in 0..3 {
            let mut full = scene(&ctx, None, &unfrosted);
            full.textures_delta.clear();
            let paint = painted(&full);
            gone = !fills_at(&paint, popup).iter().any(|(_, fill)| *fill == base);
        }
        assert!(gone, "no opaque fill remains where the popup was");
    }
}
