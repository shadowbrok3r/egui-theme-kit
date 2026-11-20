//! A two-line session row: status dot, title and updated time over the subtitle and badges.

use egui::{
    Color32, CursorIcon, Id, Rect, Response, Role, Sense, TextStyle, Ui, Vec2, WidgetInfo, pos2,
    vec2,
};

use super::parts::{TOUCH_MIN, count_fill, paint_count, touch};
use crate::chat::palette::ChatPalette;
use crate::chat::types::{SessionStatus, Tone};
use crate::controls::{
    State, StatusDot, count_badge, elided, focus_ring, kit, medium_font_sized, paint_box,
    semibold_font_sized, state_of, text_galley, whole,
};

/// Inset of a row's content from its edges.
const PAD_X: f32 = 10.0;
const PAD_Y: f32 = 6.0;
/// Gap between the two lines.
const LINE_GAP: f32 = 2.0;
/// Gap between the dot slot and the text, and between trailing parts.
const GAP: f32 = 8.0;
/// Width of the selected row's accent bar.
const ACCENT_W: f32 = 3.0;
/// Inset of the accent bar from the row's top and bottom.
const ACCENT_INSET_Y: f32 = 8.0;
/// Seconds the selection wash and accent bar take to settle.
const FADE_SECS: f32 = 0.14;
/// Shortest two-line row on a touch screen.
const TOUCH_ROW_H: f32 = 56.0;
/// Dot diameter of a collapsed row.
const COLLAPSED_DOT: f32 = 10.0;
/// Text size of a badge chip.
const BADGE_PT: f32 = 10.5;
/// Horizontal padding inside a badge chip.
const BADGE_PAD_X: f32 = 5.0;
/// Opacity of a badge chip's fill and edge.
const BADGE_FILL: f32 = 0.16;
const BADGE_EDGE: f32 = 0.55;

/// A two-line session row, or its status dot alone when collapsed.
#[must_use = "call `show` to draw the row"]
pub struct SessionItem<'a> {
    title: &'a str,
    subtitle: &'a str,
    status: SessionStatus,
    updated: Option<&'a str>,
    unread: u32,
    badge: Option<(&'a str, Tone)>,
    selected: bool,
    collapsed: bool,
    id: Option<Id>,
}

impl<'a> SessionItem<'a> {
    pub fn new(title: &'a str) -> Self {
        Self {
            title,
            subtitle: "",
            status: SessionStatus::Idle,
            updated: None,
            unread: 0,
            badge: None,
            selected: false,
            collapsed: false,
            id: None,
        }
    }

    pub fn subtitle(mut self, subtitle: &'a str) -> Self {
        self.subtitle = subtitle;
        self
    }

    pub fn status(mut self, status: SessionStatus) -> Self {
        self.status = status;
        self
    }

    /// The last-update label shown after the title.
    pub fn updated(mut self, label: &'a str) -> Self {
        self.updated = Some(label);
        self
    }

    /// Unread count pill; zero draws nothing.
    pub fn unread(mut self, count: u32) -> Self {
        self.unread = count;
        self
    }

    /// A small text chip tinted `tone` after the subtitle.
    pub fn badge(mut self, text: &'a str, tone: Tone) -> Self {
        self.badge = Some((text, tone));
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// The status dot alone, with the title as its tooltip and the unread count on its corner.
    pub fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }

    /// A fixed widget id in place of an automatic one.
    pub fn id(mut self, id: Id) -> Self {
        self.id = Some(id);
        self
    }

    /// Height of a two-line row at the current density and text size.
    pub fn height(ui: &Ui) -> f32 {
        let title = ui.text_style_height(&TextStyle::Body);
        let subtitle = ui.text_style_height(&TextStyle::Small);
        let h = (title + LINE_GAP + subtitle + 2.0 * PAD_Y).ceil();
        if touch(ui) { h.max(TOUCH_ROW_H) } else { h }
    }

    /// Side of a collapsed row.
    pub fn collapsed_side(ui: &Ui) -> f32 {
        let (_, m) = kit(ui);
        let side = m.button_h + 4.0;
        if touch(ui) { side.max(TOUCH_MIN) } else { side }
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let size = if self.collapsed {
            Vec2::splat(Self::collapsed_side(ui))
        } else {
            vec2(ui.available_width(), Self::height(ui))
        };
        let (auto, rect) = ui.allocate_space(size);
        let response = ui.interact(rect, self.id.unwrap_or(auto), Sense::click());
        response.widget_info(|| {
            WidgetInfo::selected(Role::Button, ui.is_enabled(), self.selected, self.title)
        });
        let response = response.on_hover_cursor(CursorIcon::PointingHand);
        if ui.is_rect_visible(rect) {
            self.paint(ui, rect, &response);
        }
        if self.collapsed && !self.title.is_empty() {
            response.on_hover_text(self.title)
        } else {
            response
        }
    }

    fn paint(&self, ui: &Ui, rect: Rect, response: &Response) {
        let (t, _) = kit(ui);
        let p = ChatPalette::of(ui);
        let (state, focused) = state_of(ui, response);
        let under = match state {
            State::Rest => Color32::TRANSPARENT,
            State::Hovered => t.hover,
            State::Pressed => t.pressed,
        };
        ui.painter().rect_filled(rect, p.radius_card, under);
        let how = ui
            .ctx()
            .animate_bool_with_time(response.id, self.selected, FADE_SECS);
        if how > 0.0 {
            ui.painter()
                .rect_filled(rect, p.radius_card, p.accent_soft.gamma_multiply(how));
            let full = rect.height() - 2.0 * ACCENT_INSET_Y;
            let bar = Rect::from_center_size(
                pos2(rect.left() + ACCENT_W / 2.0, rect.center().y),
                vec2(ACCENT_W, (full * how).max(ACCENT_W)),
            );
            ui.painter()
                .rect_filled(bar, ACCENT_W / 2.0, p.accent.gamma_multiply(how));
        }
        if self.collapsed {
            self.paint_collapsed(ui, rect, &p);
        } else {
            self.paint_lines(ui, rect, &p);
        }
        if focused {
            focus_ring(ui, rect, p.radius_card, &t);
        }
    }

    fn dot(&self, p: &ChatPalette) -> StatusDot<'static> {
        StatusDot::new(self.status.tone().color(p)).pulse(self.status == SessionStatus::Working)
    }

    fn paint_collapsed(&self, ui: &Ui, rect: Rect, p: &ChatPalette) {
        let dot = self.dot(p).diameter(COLLAPSED_DOT);
        dot.paint(ui, rect.center());
        if self.unread > 0 {
            let slot = Rect::from_center_size(rect.center(), Vec2::splat(dot.side()));
            count_badge(ui, slot, self.unread as usize, p.accent);
        }
    }

    fn paint_lines(&self, ui: &Ui, rect: Rect, p: &ChatPalette) {
        let (_, m) = kit(ui);
        let body = TextStyle::Body.resolve(ui.style());
        let small = TextStyle::Small.resolve(ui.style());
        let title_h = ui.text_style_height(&TextStyle::Body);
        let sub_h = ui.text_style_height(&TextStyle::Small);
        let block = title_h + LINE_GAP + sub_h;
        let top = (rect.center().y - block / 2.0).round();
        let title_cy = top + title_h / 2.0;
        let sub_cy = top + title_h + LINE_GAP + sub_h / 2.0;
        let right = rect.right() - PAD_X;

        let slot = Rect::from_center_size(
            pos2(rect.left() + PAD_X + m.icon_md / 2.0, title_cy),
            Vec2::splat(m.icon_md),
        );
        self.dot(p).paint(ui, slot.center());
        let x = slot.right() + GAP;

        let mut title_right = right;
        if let Some(label) = self.updated.filter(|l| !l.is_empty()) {
            let galley = text_galley(ui, label, small.clone());
            title_right -= galley.size().x;
            let pos = pos2(title_right, (title_cy - galley.size().y / 2.0).round());
            let ink = if self.selected {
                p.soft_faint
            } else {
                p.text_faint
            };
            ui.painter().galley(pos, galley, ink);
            title_right -= GAP;
        }
        let font = if self.selected || self.unread > 0 {
            semibold_font_sized(ui, body.size)
        } else {
            medium_font_sized(ui, body.size)
        };
        let title = elided(ui, self.title, font, (title_right - x).max(1.0));
        let pos = pos2(x, (title_cy - title.size().y / 2.0).round());
        ui.painter().galley(pos, title, p.text);

        let mut sub_right = right;
        if self.unread > 0 {
            sub_right = paint_count(
                ui,
                sub_right,
                sub_cy,
                self.unread,
                count_fill(Tone::Accent, p),
            );
            sub_right -= GAP / 2.0;
        }
        if let Some((text, tone)) = self.badge {
            sub_right =
                paint_badge(ui, sub_right, sub_cy, text, tone.color(p), p.radius_chip) - GAP / 2.0;
        }
        if !self.subtitle.is_empty() {
            let galley = elided(ui, self.subtitle, small, (sub_right - x).max(1.0));
            let pos = pos2(x, (sub_cy - galley.size().y / 2.0).round());
            let ink = if self.selected {
                p.soft_dim
            } else {
                p.text_dim
            };
            ui.painter().galley(pos, galley, ink);
        }
    }
}

/// Paints a text chip tinted `tone` ending at `right` and centred on `cy`; returns its left edge.
fn paint_badge(ui: &Ui, right: f32, cy: f32, text: &str, tone: Color32, radius: u8) -> f32 {
    let galley = text_galley(ui, text, medium_font_sized(ui, BADGE_PT));
    let size = vec2(
        whole(galley.size().x + 2.0 * BADGE_PAD_X),
        (galley.size().y + 2.0).ceil(),
    );
    let chip = Rect::from_min_size(pos2(right - size.x, cy - size.y / 2.0).round(), size);
    paint_box(
        ui,
        chip,
        radius,
        tone.gamma_multiply(BADGE_FILL),
        tone.gamma_multiply(BADGE_EDGE),
    );
    let pos = (chip.center() - galley.size() / 2.0).round();
    ui.painter().galley(pos, galley, tone);
    chip.left()
}
