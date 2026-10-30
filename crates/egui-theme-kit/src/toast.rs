//! Theme-native `egui-toast` notifications through its custom-contents hook, and toasts with an action button such as Undo.

use std::sync::{Arc, Weak};

use egui::{
    Button, Color32, Context, CornerRadius, CursorIcon, Frame, Id, Image, ImageSource, Margin,
    Rect, Response, RichText, Sense, Shape, Stroke, StrokeKind, Ui, Vec2, WidgetText,
    epaint::RectShape,
};
use egui_toast::{Toast, ToastKind, ToastOptions, Toasts};

use super::{metrics::Metrics, tokens::Tokens};

/// Toast kinds drawn by [`contents`].
const KINDS: [ToastKind; 4] = [
    ToastKind::Info,
    ToastKind::Warning,
    ToastKind::Error,
    ToastKind::Success,
];

/// Shortest time, in seconds, a toast with an action stays up.
pub const ACTION_MIN_SECS: f64 = 6.0;

/// Longest time one frame takes off an action toast's countdown.
const MAX_STEP_SECS: f64 = 0.25;

/// Registers the themed renderer for every toast kind.
pub fn style(toasts: Toasts) -> Toasts {
    KINDS.into_iter().fold(toasts, |toasts, kind| {
        toasts.custom_contents(kind, contents)
    })
}

/// The accent color of a toast of `kind`.
pub fn tone(kind: ToastKind, t: &Tokens) -> Color32 {
    match kind {
        ToastKind::Warning => t.warn,
        ToastKind::Error => t.error,
        ToastKind::Success => t.success,
        ToastKind::Info | ToastKind::Custom(_) => t.info,
    }
}

/// A button on a toast, such as Undo; [`take_actions`] returns its id once it is clicked.
#[derive(Clone, Debug)]
pub struct ToastAction {
    /// Button label.
    pub label: String,
    /// What [`take_actions`] reports when the button is clicked.
    pub id: Id,
    /// Image drawn before the label in the label colour, such as an `egui_icon_pack::Icon`.
    pub icon: Option<ImageSource<'static>>,
}

impl ToastAction {
    /// A button labelled `label` that reports `id`.
    pub fn new(label: impl Into<String>, id: Id) -> Self {
        Self {
            label: label.into(),
            id,
            icon: None,
        }
    }

    /// This action with `icon` before its label.
    pub fn icon(mut self, icon: impl Into<ImageSource<'static>>) -> Self {
        self.icon = Some(icon.into());
        self
    }
}

/// A toast saying `text` with `action`'s button, up `secs` (at least [`ACTION_MIN_SECS`]) of unhovered time; drawn by [`style`], it ignores body clicks and shows a custom `kind` as Info.
pub fn action_toast(
    ctx: &Context,
    kind: ToastKind,
    text: impl Into<String>,
    action: ToastAction,
    secs: f64,
) -> Toast {
    let text = Arc::new(RichText::new(text.into()));
    let secs = if secs.is_nan() {
        ACTION_MIN_SECS
    } else {
        secs.max(ACTION_MIN_SECS)
    };
    with_book(ctx, |book| {
        book.entries.push(Entry {
            text: Arc::downgrade(&text),
            action,
            total: secs,
            left: secs,
            seen: None,
            closed: false,
        });
    });
    let kind = match kind {
        ToastKind::Custom(_) => ToastKind::Info,
        kind => kind,
    };
    Toast {
        kind,
        text: WidgetText::RichText(text),
        options: ToastOptions::default().duration(None).show_progress(false),
        ..Default::default()
    }
}

/// Ids of the toast actions clicked since the last call, oldest first.
pub fn take_actions(ctx: &Context) -> Vec<Id> {
    with_book(ctx, |book| std::mem::take(&mut book.clicked))
}

/// Closes the toasts whose action is `id` on their next frame, for example once Ctrl+Z ran the same undo.
pub fn close_action(ctx: &Context, id: Id) {
    with_book(ctx, |book| {
        for entry in book.entries.iter_mut().filter(|e| e.action.id == id) {
            entry.closed = true;
        }
    });
    ctx.request_repaint();
}

/// Plugin holding the action toasts; it survives a memory reset.
#[derive(Default)]
struct Book {
    entries: Vec<Entry>,
    clicked: Vec<Id>,
}

impl egui::Plugin for Book {
    fn debug_name(&self) -> &'static str {
        "egui_theme_kit::toast::Book"
    }
}

/// One action toast: its button and remaining time.
struct Entry {
    /// The toast's text, whose allocation identifies the toast; dead once egui-toast drops it.
    text: Weak<RichText>,
    action: ToastAction,
    total: f64,
    left: f64,
    /// Input time of the last frame that drew the toast.
    seen: Option<f64>,
    closed: bool,
}

impl Entry {
    /// Takes the time since the last frame off the time left unless `paused`; returns whether any is left.
    fn tick(&mut self, now: f64, paused: bool) -> bool {
        let step = self
            .seen
            .map_or(0.0, |seen| (now - seen).clamp(0.0, MAX_STEP_SECS));
        self.seen = Some(now);
        if !paused {
            self.left -= step;
        }
        self.left > 0.0
    }

    /// Share of the time left, for the progress strip; none while unbounded.
    fn fraction(&self) -> f32 {
        if self.total.is_finite() {
            (self.left / self.total).clamp(0.0, 1.0) as f32
        } else {
            0.0
        }
    }
}

/// Runs `f` on the context's action toasts once the entries of dropped toasts are gone.
fn with_book<R>(ctx: &Context, f: impl FnOnce(&mut Book) -> R) -> R {
    let handle = ctx.plugin_or_default::<Book>();
    let mut book = handle.lock();
    book.entries.retain(|entry| entry.text.strong_count() > 0);
    f(&mut book)
}

/// Index of `toast`'s entry in `book`.
fn entry_of(book: &Book, toast: &Toast) -> Option<usize> {
    let WidgetText::RichText(text) = &toast.text else {
        return None;
    };
    book.entries
        .iter()
        .position(|entry| std::ptr::eq(entry.text.as_ptr(), Arc::as_ptr(text)))
}

/// Draws one toast: a window card with a status dot, the message, an [`action_toast`]'s button and a dismiss button.
pub fn contents(ui: &mut Ui, toast: &mut Toast) -> Response {
    let (t, m) = (Tokens::get(ui.ctx()), Metrics::get(ui.ctx()));
    let tone = tone(toast.kind, &t);
    let action = with_book(ui.ctx(), |book| {
        entry_of(book, toast).map(|index| book.entries[index].action.clone())
    });
    let frame = Frame::window(ui.style())
        .inner_margin(Margin::symmetric(m.window_margin, m.gap.x as i8))
        .stroke(Stroke::NONE);
    let (mut acted, mut dismissed) = (false, false);
    let response = frame
        .show(ui, |ui| {
            ui.set_max_width(m.tooltip_w * 1.6);
            ui.horizontal(|ui| {
                if toast.options.show_icon {
                    let (slot, _) = ui.allocate_exact_size(Vec2::splat(m.icon_md), Sense::hover());
                    ui.painter()
                        .circle_filled(slot.center(), m.icon_md * 0.3, tone);
                }
                ui.label(toast.text.clone());
                if let Some(action) = &action {
                    acted = action_button(ui, action, t.primary, m.icon_md).clicked();
                }
                dismissed = ui
                    .add(Button::new("x").frame(false))
                    .on_hover_text("Dismiss")
                    .clicked();
            })
        })
        .response;

    let fraction = match &action {
        Some(action) => count_down(
            ui,
            toast,
            response.rect,
            acted.then_some(action.id),
            dismissed,
        ),
        None => {
            if dismissed {
                toast.close();
            }
            toast
                .options
                .show_progress
                .then(|| toast.options.progress() as f32)
        }
    };
    if let Some(fraction) = fraction {
        progress_bar(ui, &response, fraction, tone);
    }
    ui.painter().add(Shape::Rect(RectShape::stroke(
        response.rect,
        frame.corner_radius,
        Stroke::new(1.0, t.card_border),
        StrokeKind::Inside,
    )));
    if action.is_some() {
        return response;
    }
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    if response.interact(Sense::click()).clicked() {
        toast.close();
    }
    response
}

/// The action's button in `ink`, its icon `icon_size` points square before the label.
fn action_button(ui: &mut Ui, action: &ToastAction, ink: Color32, icon_size: f32) -> Response {
    let label = RichText::new(action.label.as_str()).color(ink);
    let button = match &action.icon {
        Some(icon) => Button::image_and_text(
            Image::new(icon.clone())
                .fit_to_exact_size(Vec2::splat(icon_size))
                .tint(ink),
            label,
        )
        .image_tint_follows_text_color(false),
        None => Button::new(label),
    };
    ui.add(button)
}

/// Counts an action toast down while the pointer is off `rect` and closes it once done, answered or closed; returns the share left.
fn count_down(
    ui: &Ui,
    toast: &mut Toast,
    rect: Rect,
    clicked: Option<Id>,
    dismissed: bool,
) -> Option<f32> {
    let paused = ui.rect_contains_pointer(rect);
    let now = ui.input(|i| i.time);
    let left = with_book(ui.ctx(), |book| {
        book.clicked.extend(clicked);
        let index = entry_of(book, toast)?;
        let entry = &mut book.entries[index];
        let stays = entry.tick(now, paused) && !entry.closed && clicked.is_none() && !dismissed;
        let fraction = entry.fraction();
        if !stays {
            book.entries.remove(index);
        }
        stays.then_some(fraction)
    });
    match left {
        Some(fraction) => {
            if !paused {
                ui.ctx().request_repaint();
            }
            Some(fraction)
        }
        None => {
            toast.close();
            None
        }
    }
}

/// The two-point remaining-time strip along the bottom edge, `fraction` of the width long.
fn progress_bar(ui: &mut Ui, response: &Response, fraction: f32, tone: Color32) {
    let rounding = CornerRadius {
        nw: 0,
        ne: 0,
        ..ui.visuals().window_corner_radius
    };
    let mut clip_rect = response.rect;
    clip_rect.set_top(clip_rect.bottom() - 2.0);
    clip_rect.set_right(clip_rect.left() + clip_rect.width() * fraction);
    ui.painter()
        .with_clip_rect(clip_rect)
        .rect_filled(response.rect, rounding, tone);
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Align2, Area, Event, Modifiers, Order, PointerButton, Pos2, RawInput, pos2, vec2};

    fn run_frame(ctx: &Context, toasts: &mut Toasts) {
        frame_at(ctx, toasts, None, Vec::new(), "");
    }

    /// One frame at input time `time` with `events`; returns the centre of the painted text `find`.
    fn frame_at(
        ctx: &Context,
        toasts: &mut Toasts,
        time: Option<f64>,
        events: Vec<Event>,
        find: &str,
    ) -> Option<Pos2> {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
            time,
            events,
            ..Default::default()
        };
        let full = ctx.run_ui(input, |ui| {
            Area::new(egui::Id::unique("toast_test"))
                .order(Order::Foreground)
                .anchor(Align2::LEFT_TOP, vec2(0.0, 0.0))
                .show(ui.ctx(), |ui| toasts.show(ui));
        });
        let found = full.shapes.iter().find_map(|clipped| match &clipped.shape {
            Shape::Text(text) if !find.is_empty() && text.galley.job.text == find => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            _ => None,
        });
        full.drop_without_applying_deltas();
        found
    }

    /// The toasts egui-toast keeps after the last frame.
    fn shown(ctx: &Context) -> usize {
        ctx.data(|d| {
            d.get_temp::<Vec<Toast>>(egui::Id::unique("__toasts"))
                .unwrap_or_default()
                .len()
        })
    }

    fn press(pos: Pos2, pressed: bool) -> Event {
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        }
    }

    #[test]
    fn every_kind_has_a_distinct_tone_from_the_tokens() {
        let t = Tokens::derive(
            &egui::Style::default(),
            Color32::from_rgb(10, 200, 10),
            Color32::from_rgb(200, 10, 200),
        );
        assert_eq!(tone(ToastKind::Error, &t), t.error);
        assert_eq!(tone(ToastKind::Warning, &t), t.warn);
        assert_eq!(tone(ToastKind::Success, &t), t.success);
        assert_eq!(tone(ToastKind::Info, &t), t.info);
        assert_eq!(tone(ToastKind::Custom(3), &t), t.info);
    }

    #[test]
    fn themed_toasts_render_and_expire() {
        let ctx = Context::default();
        let mut toasts = style(Toasts::new());
        for kind in KINDS {
            toasts.add(Toast {
                kind,
                text: format!("{kind:?} message").into(),
                options: ToastOptions::default().duration_in_seconds(30.0),
                ..Default::default()
            });
        }
        run_frame(&ctx, &mut toasts);
        run_frame(&ctx, &mut toasts);
        assert_eq!(shown(&ctx), KINDS.len());
    }

    #[test]
    fn an_action_toast_stays_its_minimum_and_its_time_stops_while_hovered() {
        let ctx = Context::default();
        let mut toasts = style(Toasts::new());
        let undo = ToastAction::new("Undo", Id::unique("undo-1"));
        toasts.add(action_toast(
            &ctx,
            ToastKind::Info,
            "Deleted layer",
            undo,
            1.0,
        ));
        let at = |step: u32| Some(f64::from(step) * 0.1);
        for step in 0..=50 {
            frame_at(&ctx, &mut toasts, at(step), Vec::new(), "");
        }
        assert_eq!(shown(&ctx), 1, "gone before {ACTION_MIN_SECS} s");

        let message = frame_at(&ctx, &mut toasts, at(51), Vec::new(), "Deleted layer")
            .expect("the message is painted");
        for step in 52..=90 {
            frame_at(
                &ctx,
                &mut toasts,
                at(step),
                vec![Event::PointerMoved(message)],
                "",
            );
        }
        assert_eq!(shown(&ctx), 1, "time ran while hovered");

        let away = Event::PointerMoved(pos2(700.0, 500.0));
        for step in 91..=105 {
            frame_at(&ctx, &mut toasts, at(step), vec![away.clone()], "");
        }
        assert_eq!(shown(&ctx), 0);
        assert!(take_actions(&ctx).is_empty());
        assert_eq!(with_book(&ctx, |book| book.entries.len()), 0);
    }

    #[test]
    fn the_action_button_reports_its_id_once_and_closes_the_toast() {
        let ctx = Context::default();
        let mut toasts = style(Toasts::new());
        let id = Id::unique("undo-7");
        toasts.add(action_toast(
            &ctx,
            ToastKind::Success,
            "Deleted pose",
            ToastAction::new("Undo", id),
            8.0,
        ));
        frame_at(&ctx, &mut toasts, Some(0.0), Vec::new(), "");
        let button = frame_at(&ctx, &mut toasts, Some(0.1), Vec::new(), "Undo")
            .expect("the action button is painted");
        let click = vec![Event::PointerMoved(button), press(button, true)];
        frame_at(&ctx, &mut toasts, Some(0.2), click, "");
        frame_at(&ctx, &mut toasts, Some(0.3), vec![press(button, false)], "");
        frame_at(&ctx, &mut toasts, Some(0.4), Vec::new(), "");
        assert_eq!(take_actions(&ctx), vec![id]);
        assert!(take_actions(&ctx).is_empty());
        assert_eq!(shown(&ctx), 0);
    }

    #[test]
    fn close_action_closes_the_toast_without_reporting_it_and_a_custom_kind_shows_as_info() {
        let ctx = Context::default();
        let mut toasts = style(Toasts::new());
        let id = Id::unique("undo-9");
        let toast = action_toast(
            &ctx,
            ToastKind::Custom(9),
            "Moved layer",
            ToastAction::new("Undo", id),
            8.0,
        );
        assert_eq!(toast.kind, ToastKind::Info);
        toasts.add(toast);
        frame_at(&ctx, &mut toasts, Some(0.0), Vec::new(), "");
        close_action(&ctx, id);
        frame_at(&ctx, &mut toasts, Some(0.1), Vec::new(), "");
        frame_at(&ctx, &mut toasts, Some(0.2), Vec::new(), "");
        assert_eq!(shown(&ctx), 0);
        assert!(take_actions(&ctx).is_empty());
    }

    #[test]
    fn a_frame_takes_at_most_the_step_limit_and_none_while_paused() {
        let mut entry = Entry {
            text: Weak::new(),
            action: ToastAction::new("Undo", Id::unique("undo")),
            total: 6.0,
            left: 6.0,
            seen: None,
            closed: false,
        };
        assert!(entry.tick(10.0, false));
        assert!(entry.tick(40.0, false));
        assert!((entry.left - (6.0 - MAX_STEP_SECS)).abs() < 1e-9);
        assert!(entry.tick(41.0, true));
        assert!((entry.left - (6.0 - MAX_STEP_SECS)).abs() < 1e-9);
        assert!((entry.fraction() - ((6.0 - MAX_STEP_SECS) / 6.0) as f32).abs() < 1e-6);
    }
}
