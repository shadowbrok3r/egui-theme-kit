//! Sidebar navigation rows, full or collapsed to their icon.

use egui::{
    Color32, CursorIcon, Rect, Response, Role, Sense, Stroke, StrokeKind, Ui, Vec2, Widget,
    WidgetInfo, pos2, vec2,
};

use super::labels::{badge_layout, paint_badge};
use super::{
    Icon, State, StatusDot, elided, focus_ring, ink_on, kit, medium_font_sized, mix,
    semibold_font_sized, state_of, text_galley, whole,
};

/// Inset of a nav row's content from its left edge.
const PAD_X: f32 = 10.0;
/// Gap between a nav row's icon and label, and between trailing items.
const ICON_GAP: f32 = 10.0;
/// Nav label size.
const LABEL_PT: f32 = 13.0;
/// Width of the selected row's accent bar.
const ACCENT_W: f32 = 3.0;
/// Inset of the accent bar from the row's top and bottom when fully shown.
const ACCENT_INSET_Y: f32 = 7.0;
/// Opacity of a selected icon's duotone back layer.
const DUO_ALPHA: f32 = 0.3;
/// Seconds the selection wash and accent bar take to settle.
const FADE_SECS: f32 = 0.14;
/// Extra height of a nav row over a prominent button.
const EXTRA_H: f32 = 4.0;
/// Count-badge text size.
const COUNT_PT: f32 = 10.0;
/// Count-badge height.
const COUNT_H: f32 = 16.0;
/// Horizontal padding inside a count badge.
const COUNT_PAD_X: f32 = 5.0;
/// Count badge and dot size in a collapsed row's corner.
const CORNER_COUNT_H: f32 = 14.0;
const CORNER_DOT: f32 = 7.0;
/// Inset of a collapsed row's corner marks from the icon slot.
const CORNER_OVERHANG: f32 = 5.0;

/// A sidebar row: an icon and a label, with an accent bar and a wash while selected.
#[must_use = "call `show` to draw the row"]
pub struct NavItem<'a> {
    icon: Icon,
    label: &'a str,
    selected: bool,
    badge: Option<(&'a str, Color32)>,
    count: Option<(usize, Option<Color32>)>,
    dot: Option<(Color32, bool)>,
    collapsed: bool,
    width: Option<f32>,
    enabled: bool,
}

impl<'a> NavItem<'a> {
    pub fn new(icon: impl Into<Icon>, label: &'a str) -> Self {
        Self {
            icon: icon.into(),
            label,
            selected: false,
            badge: None,
            count: None,
            dot: None,
            collapsed: false,
            width: None,
            enabled: true,
        }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// A small text chip at the right edge, tinted `tone`.
    pub fn badge(mut self, text: &'a str, tone: Color32) -> Self {
        self.badge = Some((text, tone));
        self
    }

    /// A count pill at the right edge, in `tone` or the primary role; nothing for zero, "99+" above 99.
    pub fn count(mut self, count: usize, tone: impl Into<Option<Color32>>) -> Self {
        self.count = Some((count, tone.into()));
        self
    }

    /// A status dot at the right edge in `tone`, pulsing while `pulse` is set.
    pub fn dot(mut self, tone: Color32, pulse: bool) -> Self {
        self.dot = Some((tone, pulse));
        self
    }

    /// Icon only; the label becomes the tooltip, the count and dot sit on the icon's corner.
    pub fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }

    /// Outer width; the available width by default, or the row height when collapsed.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Draws the row disabled and ignores clicks when `false`.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Height of a nav row at the current density.
    pub fn height(ui: &Ui) -> f32 {
        kit(ui).1.button_h + EXTRA_H
    }

    /// The tooltip the row shows: its label when collapsed, none otherwise.
    pub fn hover_text(&self) -> Option<&'a str> {
        (self.collapsed && !self.label.is_empty()).then_some(self.label)
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        if !self.enabled {
            return ui
                .add_enabled_ui(false, |ui| self.enabled(true).show(ui))
                .inner;
        }
        let (t, m) = kit(ui);
        let height = Self::height(ui);
        let width = match (self.width, self.collapsed) {
            (Some(w), _) => w,
            (None, true) => height,
            (None, false) => ui.available_width(),
        };
        let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
        response.widget_info(|| {
            WidgetInfo::selected(Role::Button, ui.is_enabled(), self.selected, self.label)
        });
        let response = response.on_hover_cursor(CursorIcon::PointingHand);
        if ui.is_rect_visible(rect) {
            self.paint(ui, rect, &response, m.icon_lg, &t);
        }
        match self.hover_text() {
            Some(text) => response.on_hover_text(text),
            None => response,
        }
    }

    fn paint(&self, ui: &Ui, rect: Rect, response: &Response, icon: f32, t: &super::Tokens) {
        let (state, focused) = state_of(ui, response);
        let how = ui
            .ctx()
            .animate_bool_with_time(response.id, self.selected, FADE_SECS);
        let under = match state {
            State::Rest => Color32::TRANSPARENT,
            State::Hovered => t.hover,
            State::Pressed => t.pressed,
        };
        if under != Color32::TRANSPARENT {
            ui.painter().rect_filled(rect, t.radius_md, under);
        }
        if how > 0.0 {
            ui.painter()
                .rect_filled(rect, t.radius_md, t.primary_soft.gamma_multiply(how));
            let full = rect.height() - 2.0 * ACCENT_INSET_Y;
            let bar = Rect::from_center_size(
                pos2(rect.left() + ACCENT_W / 2.0, rect.center().y),
                vec2(ACCENT_W, (full * how).max(ACCENT_W)),
            );
            ui.painter()
                .rect_filled(bar, ACCENT_W / 2.0, t.primary.gamma_multiply(how));
        }

        let slot_x = if self.collapsed {
            rect.center().x
        } else {
            rect.left() + PAD_X + icon / 2.0
        };
        let slot = Rect::from_center_size(pos2(slot_x, rect.center().y), Vec2::splat(icon));
        let icon_ink = if ui.is_enabled() {
            mix(t.icon, t.primary, how)
        } else {
            t.text_faint
        };
        if self.selected {
            let back = t.primary.gamma_multiply(DUO_ALPHA * how);
            self.icon.paint_duo(ui, slot, icon, icon_ink, back);
        } else {
            self.icon.paint(ui, slot, icon, icon_ink);
        }

        if self.collapsed {
            self.paint_corner_marks(ui, slot, t);
        } else {
            self.paint_label_and_trail(ui, rect, slot, state, t);
        }
        if focused {
            focus_ring(ui, rect, t.radius_md, t);
        }
    }

    fn paint_label_and_trail(
        &self,
        ui: &Ui,
        rect: Rect,
        slot: Rect,
        state: State,
        t: &super::Tokens,
    ) {
        let mut right = rect.right() - PAD_X;
        let cy = rect.center().y;
        if let Some((tone, pulse)) = self.dot {
            let dot = StatusDot::new(tone).pulse(pulse);
            let side = dot.side();
            dot.paint(ui, pos2(right - side / 2.0, cy));
            right -= side + ICON_GAP / 2.0;
        }
        if let Some((count, tone)) = self.count.filter(|(n, _)| *n > 0) {
            let tone = tone.unwrap_or(t.primary);
            let text = count_text(count);
            let galley = text_galley(ui, &text, medium_font_sized(ui, COUNT_PT));
            let w = whole(galley.size().x + 2.0 * COUNT_PAD_X).max(COUNT_H);
            let pill = Rect::from_min_size(
                pos2(right - w, cy - COUNT_H / 2.0).round(),
                vec2(w, COUNT_H),
            );
            ui.painter().rect_filled(pill, (COUNT_H / 2.0) as u8, tone);
            let pos = (pill.center() - galley.size() / 2.0).round();
            ui.painter().galley(pos, galley, ink_on(tone));
            right = pill.left() - ICON_GAP / 2.0;
        }
        if let Some((text, tone)) = self.badge {
            let (galley, size) = badge_layout(ui, text);
            let chip = Rect::from_min_size(pos2(right - size.x, cy - size.y / 2.0), size);
            paint_badge(ui, chip, galley, tone);
            right = chip.left() - ICON_GAP / 2.0;
        }
        let x = slot.right() + ICON_GAP;
        let font = if self.selected {
            semibold_font_sized(ui, LABEL_PT)
        } else {
            medium_font_sized(ui, LABEL_PT)
        };
        let label = elided(ui, self.label, font, (right - x).max(1.0));
        let ink = if !ui.is_enabled() {
            t.text_faint
        } else if self.selected || state != State::Rest {
            t.text
        } else {
            t.text_dim
        };
        let y = (cy - label.size().y / 2.0).round();
        ui.painter().galley(pos2(x, y), label, ink);
    }

    fn paint_corner_marks(&self, ui: &Ui, slot: Rect, t: &super::Tokens) {
        if let Some((count, tone)) = self.count.filter(|(n, _)| *n > 0) {
            let tone = tone.unwrap_or(t.primary);
            let text = count_text(count);
            let galley = text_galley(ui, &text, medium_font_sized(ui, COUNT_PT - 1.0));
            let w = whole(galley.size().x + 2.0 * (COUNT_PAD_X - 1.0)).max(CORNER_COUNT_H);
            let pill = Rect::from_min_size(
                pos2(
                    slot.right() + CORNER_OVERHANG - w,
                    slot.top() - CORNER_OVERHANG,
                )
                .round(),
                vec2(w, CORNER_COUNT_H),
            );
            ui.painter().rect(
                pill,
                (CORNER_COUNT_H / 2.0) as u8,
                tone,
                Stroke::new(1.0, t.base),
                StrokeKind::Outside,
            );
            let pos = (pill.center() - galley.size() / 2.0).round();
            ui.painter().galley(pos, galley, ink_on(tone));
        } else if let Some((tone, pulse)) = self.dot {
            let center = pos2(slot.right(), slot.top()) + vec2(1.0, -1.0);
            ui.painter()
                .circle_filled(center, CORNER_DOT / 2.0 + 1.0, t.base);
            StatusDot::new(tone)
                .diameter(CORNER_DOT)
                .pulse(pulse)
                .paint(ui, center);
        }
    }
}

impl Widget for NavItem<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        self.show(ui)
    }
}

/// `count` as badge text, capped at "99+".
fn count_text(count: usize) -> String {
    if count > 99 {
        "99+".to_owned()
    } else {
        count.to_string()
    }
}

/// A sidebar row with `icon` and `label`.
pub fn nav_item(ui: &mut Ui, icon: impl Into<Icon>, label: &str, selected: bool) -> Response {
    NavItem::new(icon, label).selected(selected).show(ui)
}
