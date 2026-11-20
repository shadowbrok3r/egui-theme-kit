//! Pieces the bars and rows share: touch-sized icon buttons, count pills and tone order.

use egui::{Color32, Id, Rect, Response, Role, Sense, Ui, Vec2, WidgetInfo, pos2, vec2};

use super::BarButton;
use crate::chat::palette::ChatPalette;
use crate::chat::types::Tone;
use crate::controls::{
    Icon, State, count_badge, focus_ring, ink_on, kit, medium_font_sized, paint_box,
    selected_colors, state_of, text_galley, whole,
};

/// Smallest side of a touch target.
pub(super) const TOUCH_MIN: f32 = 40.0;
/// Count pill text size.
const COUNT_PT: f32 = 10.0;
/// Horizontal padding inside a count pill.
const COUNT_PAD_X: f32 = 5.0;
/// Count pill height as a share of the control height.
const COUNT_H_SHARE: f32 = 2.0 / 3.0;
/// Opacity of a selected icon's duotone back layer.
const DUO_ALPHA: f32 = 0.3;

/// Whether the context reports a touch screen.
pub(super) fn touch(ui: &Ui) -> bool {
    ui.input(|i| i.has_touch_screen())
}

/// Side of a tappable control: the control height, at least [`TOUCH_MIN`] on touch.
pub(super) fn target(ui: &Ui) -> f32 {
    let (_, m) = kit(ui);
    if touch(ui) {
        m.control_h.max(TOUCH_MIN)
    } else {
        m.control_h
    }
}

/// A square icon button `side` wide under the fixed `id`; transparent until hovered.
pub(super) fn icon_button(
    ui: &mut Ui,
    id: Id,
    icon: Icon,
    side: f32,
    selected: bool,
    tip: &str,
    enabled: bool,
) -> Response {
    let (t, m) = kit(ui);
    let (_, rect) = ui.allocate_space(Vec2::splat(side));
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let response = ui.interact(rect, id, sense);
    response.widget_info(|| WidgetInfo::selected(Role::Button, enabled, selected, tip));
    if ui.is_rect_visible(rect) {
        let (state, focused) = state_of(ui, &response);
        let glyph = m.icon_md;
        if selected {
            let (fill, edge, _) = selected_colors(&t);
            paint_box(ui, rect, t.radius_sm, fill, edge);
            let back = t.primary.gamma_multiply(DUO_ALPHA);
            icon.paint_duo(ui, rect, glyph, t.primary, back);
        } else {
            let (fill, ink) = match (enabled, state) {
                (false, _) => (Color32::TRANSPARENT, t.text_faint),
                (true, State::Rest) => (Color32::TRANSPARENT, t.icon),
                (true, State::Hovered) => (t.hover, t.text),
                (true, State::Pressed) => (t.pressed, t.text),
            };
            ui.painter().rect_filled(rect, t.radius_sm, fill);
            icon.paint(ui, rect, glyph, ink);
        }
        if focused && enabled {
            focus_ring(ui, rect, t.radius_sm, &t);
        }
    }
    if tip.is_empty() {
        response
    } else {
        response.on_hover_text(tip)
    }
}

/// A [`BarButton`] `side` wide under `id`, with its count over the corner.
pub(super) fn bar_button<K>(ui: &mut Ui, id: Id, button: &BarButton<'_, K>, side: f32) -> Response {
    let response = icon_button(
        ui,
        id,
        button.icon,
        side,
        button.selected,
        &button.tip,
        true,
    );
    if let Some(count) = button.badge {
        let p = ChatPalette::of(ui);
        count_badge(ui, response.rect, count as usize, p.accent);
    }
    response
}

/// Menu label of a bar button: its tip, with the count when there is one.
pub(super) fn button_caption<K>(button: &BarButton<'_, K>) -> String {
    match button.badge.filter(|n| *n > 0) {
        Some(n) => format!("{} ({})", button.tip, count_text(n)),
        None => button.tip.to_string(),
    }
}

/// `count` as pill text, capped at `99+`.
pub(super) fn count_text(count: u32) -> String {
    if count > 99 {
        "99+".to_owned()
    } else {
        count.to_string()
    }
}

/// Width of a count pill for `count`.
pub(super) fn count_width(ui: &Ui, count: u32) -> f32 {
    let (_, m) = kit(ui);
    let galley = text_galley(ui, &count_text(count), medium_font_sized(ui, COUNT_PT));
    let h = (m.control_h * COUNT_H_SHARE).round();
    whole(galley.size().x + 2.0 * COUNT_PAD_X).max(h)
}

/// Paints a count pill filled `fill` ending at `right` and centred on `cy`; returns its left edge.
pub(super) fn paint_count(ui: &Ui, right: f32, cy: f32, count: u32, fill: Color32) -> f32 {
    let (_, m) = kit(ui);
    let galley = text_galley(ui, &count_text(count), medium_font_sized(ui, COUNT_PT));
    let h = (m.control_h * COUNT_H_SHARE).round();
    let w = whole(galley.size().x + 2.0 * COUNT_PAD_X).max(h);
    let pill = Rect::from_min_size(pos2(right - w, cy - h / 2.0).round(), vec2(w, h));
    ui.painter().rect_filled(pill, h / 2.0, fill);
    let pos = (pill.center() - galley.size() / 2.0).round();
    ui.painter().galley(pos, galley, ink_on(fill));
    pill.left()
}

/// Rank of a tone by urgency, highest most urgent.
pub(super) fn severity(tone: Tone) -> u8 {
    match tone {
        Tone::Neutral => 0,
        Tone::Success => 1,
        Tone::Info => 2,
        Tone::Accent => 3,
        Tone::Warn => 4,
        Tone::Error => 5,
    }
}

/// Fill of a count pill in `tone`; neutral counts use the accent.
pub(super) fn count_fill(tone: Tone, p: &ChatPalette) -> Color32 {
    match tone {
        Tone::Neutral => p.accent,
        tone => tone.color(p),
    }
}
