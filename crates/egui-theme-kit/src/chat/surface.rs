//! Surfaces chat widgets sit on: a flat fill and stroke, or kit glass that falls back to flat.

use std::sync::Arc;

use egui::{
    AsIdSalt, Color32, Context, CornerRadius, Frame, Id, InnerResponse, Margin, Rect, Shape,
    Stroke, StrokeKind, Ui, epaint::RectShape,
};

use super::palette::ChatPalette;
use crate::glass::{self, GlassParams};

/// Opacity of a surface fill over a frosted backdrop.
const GLASS_FILL_ALPHA: f32 = 0.55;

/// What a surface is for, which picks its fill, edge and radius.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SurfaceKind {
    SessionCard,
    UserBubble,
    Card,
    Composer,
    Popup,
    Chip,
    /// Plate behind a code block or payload.
    Plate,
    /// Wash behind inline code.
    Code,
    Button,
}

impl SurfaceKind {
    /// Whether kit glass frosts this kind.
    pub fn frosts(self) -> bool {
        matches!(
            self,
            Self::SessionCard | Self::UserBubble | Self::Card | Self::Composer | Self::Popup
        )
    }

    /// Fill, edge and corner radius of the flat look.
    pub fn flat(self, p: &ChatPalette) -> (Color32, Color32, u8) {
        match self {
            Self::SessionCard | Self::Card => (p.card_fill, p.card_stroke, p.radius_card),
            Self::UserBubble => (p.user_fill, p.user_stroke, p.radius_bubble),
            Self::Composer => (p.chip_fill, p.chip_stroke, p.radius_card),
            Self::Popup => (p.popup_fill, p.card_stroke, p.radius_card),
            Self::Chip | Self::Button => (p.chip_fill, p.chip_stroke, p.radius_chip),
            Self::Plate => (p.code_plate, Color32::TRANSPARENT, p.radius_chip),
            Self::Code => (p.code_chip, Color32::TRANSPARENT, p.radius_chip),
        }
    }
}

/// Paints chat surfaces; apps install their own with [`set_surface`].
pub trait ChatSurface: Send + Sync {
    /// The shape of a `kind` surface over `rect`.
    fn shape(&self, ui: &Ui, rect: Rect, kind: SurfaceKind, p: &ChatPalette) -> Shape;

    /// Enqueues backdrop work behind `rect` before the surface paints; returns whether any was enqueued.
    fn backdrop(&self, _ui: &Ui, _rect: Rect, _kind: SurfaceKind, _p: &ChatPalette) -> bool {
        false
    }
}

/// Palette fill plus a one-pixel inside stroke.
#[derive(Clone, Copy, Debug, Default)]
pub struct FlatSurface;

impl ChatSurface for FlatSurface {
    fn shape(&self, _ui: &Ui, rect: Rect, kind: SurfaceKind, p: &ChatPalette) -> Shape {
        let (fill, edge, radius) = kind.flat(p);
        rect_shape(rect, radius, fill, edge)
    }
}

/// Frosted kit glass under a translucent fill while the blur runs; [`FlatSurface`] otherwise.
#[derive(Clone, Copy, Debug, Default)]
pub struct KitGlassSurface;

impl KitGlassSurface {
    /// Whether `kind` is frosted on `ctx` this frame.
    pub fn active(ctx: &Context, kind: SurfaceKind) -> bool {
        kind.frosts() && glass::is_available() && glass::params(ctx).is_visible()
    }
}

impl ChatSurface for KitGlassSurface {
    fn shape(&self, ui: &Ui, rect: Rect, kind: SurfaceKind, p: &ChatPalette) -> Shape {
        if !Self::active(ui.ctx(), kind) {
            return FlatSurface.shape(ui, rect, kind, p);
        }
        let (fill, edge, radius) = kind.flat(p);
        rect_shape(rect, radius, fill.gamma_multiply(GLASS_FILL_ALPHA), edge)
    }

    fn backdrop(&self, ui: &Ui, rect: Rect, kind: SurfaceKind, p: &ChatPalette) -> bool {
        if !Self::active(ui.ctx(), kind) {
            return false;
        }
        let (_, _, radius) = kind.flat(p);
        let params = GlassParams {
            corner_radius: f32::from(radius),
            ..glass::params(ui.ctx())
        };
        glass::frost_with(ui, rect, params)
    }
}

fn rect_shape(rect: Rect, radius: u8, fill: Color32, edge: Color32) -> Shape {
    let stroke = if edge.a() == 0 {
        Stroke::NONE
    } else {
        Stroke::new(1.0, edge)
    };
    Shape::Rect(RectShape::new(
        rect,
        CornerRadius::same(radius),
        fill,
        stroke,
        StrokeKind::Inside,
    ))
}

fn surface_id() -> Id {
    Id::unique("egui_theme_kit.chat.surface")
}

/// Installs the surface chat widgets paint with.
pub fn set_surface(ctx: &Context, surface: Arc<dyn ChatSurface>) {
    ctx.data_mut(|d| d.insert_temp(surface_id(), surface));
}

/// The installed surface, [`KitGlassSurface`] when none was set.
pub fn surface(ctx: &Context) -> Arc<dyn ChatSurface> {
    ctx.data(|d| d.get_temp::<Arc<dyn ChatSurface>>(surface_id()))
        .unwrap_or_else(|| Arc::new(KitGlassSurface))
}

/// Paints a `kind` surface over the known `rect`, backdrop first.
pub fn paint_surface(ui: &Ui, rect: Rect, kind: SurfaceKind) {
    let p = ChatPalette::of(ui);
    let s = surface(ui.ctx());
    s.backdrop(ui, rect, kind, &p);
    ui.painter().add(s.shape(ui, rect, kind, &p));
}

/// Shows `contents` on a `kind` surface sized to them; the backdrop uses last frame's rect.
pub fn surface_frame<R>(
    ui: &mut Ui,
    id_salt: impl AsIdSalt,
    kind: SurfaceKind,
    margin: Margin,
    contents: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<R> {
    let id = ui
        .scope_id()
        .with(("egui_theme_kit.chat.surface_frame", id_salt));
    let p = ChatPalette::of(ui);
    let s = surface(ui.ctx());
    if let Some(rect) = ui.ctx().data(|d| d.get_temp::<Rect>(id)) {
        s.backdrop(ui, rect, kind, &p);
    }
    let slot = ui.painter().add(Shape::Noop);
    let inner = Frame::new().inner_margin(margin).show(ui, contents);
    let rect = inner.response.rect;
    ui.painter().set(slot, s.shape(ui, rect, kind, &p));
    ui.ctx().data_mut(|d| d.insert_temp(id, rect));
    InnerResponse::new(inner.inner, inner.response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glass_without_a_backend_paints_the_flat_shape() {
        let ctx = Context::default();
        let mut out = ctx.run_ui(Default::default(), |ui| {
            let p = ChatPalette::of(ui);
            let rect = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(40.0, 20.0));
            for kind in [SurfaceKind::UserBubble, SurfaceKind::Plate] {
                let glass = KitGlassSurface.shape(ui, rect, kind, &p);
                let flat = FlatSurface.shape(ui, rect, kind, &p);
                assert_eq!(format!("{glass:?}"), format!("{flat:?}"));
                assert!(!KitGlassSurface.backdrop(ui, rect, kind, &p));
            }
        });
        out.textures_delta.clear();
    }

    #[test]
    fn surface_frame_fills_behind_its_contents() {
        let ctx = Context::default();
        let mut out = ctx.run_ui(Default::default(), |ui| {
            surface_frame(
                ui,
                "bubble",
                SurfaceKind::UserBubble,
                Margin::same(6),
                |ui| {
                    ui.label("hi");
                },
            );
        });
        out.textures_delta.clear();
        let p = ChatPalette::get(&ctx);
        let first = out.shapes.iter().find_map(|s| match &s.shape {
            Shape::Rect(r) => Some(r.fill),
            _ => None,
        });
        assert_eq!(first, Some(p.user_fill));
    }
}
