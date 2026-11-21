//! The global app bar and its view dropdown.

use std::hash::Hash;

use egui::{
    Align, Color32, Layout, Rect, Response, Role, Sense, TextStyle, Ui, UiBuilder, Vec2,
    WidgetInfo, pos2, vec2,
};

use super::layout::{APP_BAR_FOLD_W, badge_total};
use super::parts::{bar_button, count_fill, count_width, icon_button, paint_count, target};
use super::{AppBarAction, ChatAppBar, ViewItem, ViewMenu};
use crate::chat::icons::ChatIcons;
use crate::chat::palette::ChatPalette;
use crate::chat::transcript::DetailLevel;
use crate::chat::types::Tone;
use crate::controls::{
    Segment, State, Toggle, fill_width, focus_ring, kit, medium_font, menu_check_item,
    menu_separator, popup_menu, segmented, semibold_font_sized, state_of, text_galley, title_rule,
    whole,
};

/// Inset of the view trigger's content from its edges.
const TRIGGER_PAD: f32 = 6.0;
/// Gap between the parts of the view trigger and of a view row.
const PART_GAP: f32 = 6.0;

/// The two detail levels with their labels.
const LEVELS: [(DetailLevel, &str); 2] = [
    (DetailLevel::Simple, "Simple"),
    (DetailLevel::Detailed, "Detailed"),
];

impl<V: Clone + PartialEq> ViewMenu<'_, V> {
    /// The picked value when it differs from `selected`.
    pub fn show(self, ui: &mut Ui) -> Option<V> {
        let (t, m) = kit(ui);
        let p = ChatPalette::of(ui);
        let icons = ChatIcons::get(ui.ctx());
        let current = self.items.iter().find(|i| i.value == self.selected);
        let label = current.map_or("", |i| i.label.as_ref());
        let icon = current.and_then(|i| i.icon);
        let total = badge_total(self.items);
        let galley = text_galley(ui, label, medium_font(ui, TextStyle::Button));
        let caret = m.icon_sm - 2.0;
        let mut w = 2.0 * TRIGGER_PAD + galley.size().x + PART_GAP + caret;
        if icon.is_some() {
            w += m.icon_md + PART_GAP;
        }
        if let Some((n, _)) = total {
            w += PART_GAP + count_width(ui, n);
        }
        let (rect, response) = ui.allocate_exact_size(vec2(whole(w), target(ui)), Sense::click());
        response.widget_info(|| WidgetInfo::labeled(Role::ComboBox, ui.is_enabled(), label));
        let selected = &self.selected;
        let picked = popup_menu(&response, false, |ui| {
            let mut picked = None;
            for item in self.items {
                if view_row(ui, item, item.value == *selected).clicked() {
                    picked = Some(item.value.clone());
                }
            }
            picked
        })
        .flatten();
        if ui.is_rect_visible(rect) {
            let (state, focused) = state_of(ui, &response);
            let fill = match state {
                State::Rest => Color32::TRANSPARENT,
                State::Hovered => t.hover,
                State::Pressed => t.pressed,
            };
            ui.painter().rect_filled(rect, t.radius_sm, fill);
            let cy = rect.center().y;
            let mut x = rect.left() + TRIGGER_PAD;
            if let Some(icon) = icon {
                let slot =
                    Rect::from_center_size(pos2(x + m.icon_md / 2.0, cy), Vec2::splat(m.icon_md));
                icon.paint(ui, slot, m.icon_md, p.text);
                x = slot.right() + PART_GAP;
            }
            let text_w = galley.size().x;
            let y = (cy - galley.size().y / 2.0).round();
            ui.painter().galley(pos2(x, y), galley, p.text);
            x += text_w;
            if let Some((n, tone)) = total {
                let right = x + PART_GAP + count_width(ui, n);
                x = right;
                paint_count(ui, right, cy, n, count_fill(tone, &p));
            }
            let slot =
                Rect::from_center_size(pos2(x + PART_GAP + caret / 2.0, cy), Vec2::splat(caret));
            icons.chevron_down.paint(ui, slot, caret, p.text_dim);
            if focused {
                focus_ring(ui, rect, t.radius_sm, &t);
            }
        }
        picked.filter(|v| *v != self.selected)
    }
}

/// One row of the view menu: icon, label, count pill and a check on the selected view.
fn view_row<V>(ui: &mut Ui, item: &ViewItem<'_, V>, selected: bool) -> Response {
    let (t, m) = kit(ui);
    let p = ChatPalette::of(ui);
    let icons = ChatIcons::get(ui.ctx());
    let font = if selected {
        semibold_font_sized(ui, TextStyle::Button.resolve(ui.style()).size)
    } else {
        medium_font(ui, TextStyle::Button)
    };
    let galley = text_galley(ui, &item.label, font);
    let count = item.badge.filter(|n| *n > 0);
    let lead = m.pad.x + m.icon_md + m.gap.x;
    let check_w = PART_GAP + m.icon_sm;
    let badge_w = count.map_or(0.0, |n| PART_GAP + count_width(ui, n));
    let min_w = lead + galley.size().x + badge_w + check_w + m.pad.x;
    let h = m.row_h.max(target(ui));
    let (rect, response) =
        ui.allocate_exact_size(vec2(fill_width(ui).max(min_w), h), Sense::click());
    response.widget_info(|| {
        WidgetInfo::selected(Role::RadioButton, ui.is_enabled(), selected, &*item.label)
    });
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let (state, focused) = state_of(ui, &response);
    if state != State::Rest {
        ui.painter().rect_filled(rect, t.radius_sm, t.hover);
    }
    let cy = rect.center().y;
    if let Some(icon) = item.icon {
        let slot = Rect::from_center_size(
            pos2(rect.left() + m.pad.x + m.icon_md / 2.0, cy),
            Vec2::splat(m.icon_md),
        );
        let ink = match item.tone {
            Tone::Neutral => t.icon,
            tone => tone.color(&p),
        };
        icon.paint(ui, slot, m.icon_md, ink);
    }
    let y = (cy - galley.size().y / 2.0).round();
    ui.painter()
        .galley(pos2(rect.left() + lead, y), galley, p.text);
    let mut right = rect.right() - m.pad.x;
    if selected {
        let slot =
            Rect::from_center_size(pos2(right - m.icon_sm / 2.0, cy), Vec2::splat(m.icon_sm));
        icons.check.paint(ui, slot, m.icon_sm, p.accent);
    }
    right -= check_w;
    if let Some(n) = count {
        paint_count(ui, right, cy, n, count_fill(item.tone, &p));
    }
    if focused {
        focus_ring(ui, rect, t.radius_sm, &t);
    }
    response
}

impl<V: Clone + PartialEq, K: Clone + Hash> ChatAppBar<'_, V, K> {
    /// Draws the bar with `leading` content first and a rule under it.
    pub fn show(self, ui: &mut Ui, leading: impl FnOnce(&mut Ui)) -> Option<AppBarAction<V, K>> {
        let Self {
            id_salt,
            view,
            mut detail,
            mut thinking,
            trailing,
        } = self;
        let id = ui.make_persistent_id(id_salt);
        let (_, m) = kit(ui);
        let side = target(ui);
        let width = ui.available_width();
        let folds = width < APP_BAR_FOLD_W * m.control_h;
        let (_, rect) = ui.allocate_space(vec2(width, m.bar_h.max(side)));
        let mut action = None;

        let mut left = ui.new_child(
            UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::left_to_right(Align::Center))
                .id_salt(id.with("left")),
        );
        leading(&mut left);
        if let Some(picked) = view.and_then(|v| v.show(&mut left)) {
            action = Some(AppBarAction::View(picked));
        }

        let mut right = ui.new_child(
            UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::right_to_left(Align::Center))
                .id_salt(id.with("right")),
        );
        right.spacing_mut().item_spacing.x = (m.gap.x / 2.0).round();
        for (i, button) in trailing.iter().enumerate().rev() {
            if bar_button(&mut right, id.with(("button", i)), button, side).clicked() {
                action = action.or(Some(AppBarAction::Button(button.id.clone())));
            }
        }
        if !trailing.is_empty() {
            right.add_space(m.gap.x / 2.0);
        }
        if folds {
            if detail.is_some() || thinking.is_some() {
                let icons = ChatIcons::get(ui.ctx());
                let more = icon_button(
                    &mut right,
                    id.with("view_options"),
                    icons.more,
                    side,
                    false,
                    "View options",
                    true,
                );
                let picked = popup_menu(&more, false, |ui| {
                    folded_options(ui, detail.as_deref_mut(), thinking.as_deref_mut())
                })
                .flatten();
                action = action.or(picked);
            }
        } else {
            if let Some(on) = thinking {
                if Toggle::new(on, "Thinking")
                    .label_left(true)
                    .show(&mut right)
                    .changed()
                {
                    action = action.or(Some(AppBarAction::Thinking(*on)));
                }
                right.add_space(m.gap.x);
            }
            if let Some(level) = detail {
                let segments = LEVELS.map(|(v, label)| Segment::new(v, label));
                if segmented(&mut right, level, &segments).changed() {
                    action = action.or(Some(AppBarAction::Detail(*level)));
                }
            }
        }
        title_rule(ui);
        action
    }
}

/// The detail levels and the Thinking switch as menu rows; returns what changed.
fn folded_options<V, K>(
    ui: &mut Ui,
    detail: Option<&mut DetailLevel>,
    thinking: Option<&mut bool>,
) -> Option<AppBarAction<V, K>> {
    let icons = ChatIcons::get(ui.ctx());
    let mut action = None;
    let has_detail = detail.is_some();
    if let Some(level) = detail {
        for (value, label) in LEVELS {
            if menu_check_item(ui, None, label, *level == value).clicked() && *level != value {
                *level = value;
                action = Some(AppBarAction::Detail(value));
            }
        }
    }
    if let Some(on) = thinking {
        if has_detail {
            menu_separator(ui);
        }
        if menu_check_item(ui, icons.reasoning, "Thinking", *on).clicked() {
            *on = !*on;
            action = Some(AppBarAction::Thinking(*on));
        }
    }
    action
}
