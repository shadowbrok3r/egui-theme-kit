//! The per-session title card.

use std::hash::Hash;

use egui::{
    Align, Align2, Id, Key, Label, Layout, Margin, Rect, RichText, ScrollArea, Sense, TextEdit,
    TextStyle, Ui, UiBuilder, WidgetInfo, pos2, text::CCursorRange, text::LayoutJob, vec2,
};

use super::chips::{chip_menu, chip_width, show_chip};
use super::layout::{BarLayout, fit_count, nav_steps, nav_text, title_span};
use super::parts::{bar_button, button_caption, icon_button, target};
use super::{SessionBar, SessionBarAction};
use crate::chat::icons::ChatIcons;
use crate::chat::palette::ChatPalette;
use crate::chat::surface::{SurfaceKind, surface_frame};
use crate::chat::types::{MessageNav, NavStep, SessionStatus};
use crate::controls::{
    MenuItem, StatusDot, callout, elided, kit, medium_font, menu_check_item, menu_item,
    menu_separator, popup_menu, semibold_font_sized, text_galley, vline, whole,
};

/// Narrowest centred title, in control heights, before it moves into the room between the groups.
const TITLE_MIN: f32 = 6.0;
/// Gap between the status dot and its label.
const DOT_GAP: f32 = 2.0;
/// Horizontal padding inside the jump counter.
const COUNTER_PAD: f32 = 4.0;
/// Tallest the message picker grows.
const PICK_MAX_H: f32 = 280.0;
/// Height of the rule between the button and jump groups, as a share of the row.
const RULE_SHARE: f32 = 0.6;

/// Rename draft, whether its edit has taken focus, and the session it belongs to.
type RenameDraft = (String, bool, u64);

impl<K: Clone + Hash> SessionBar<'_, K> {
    pub fn show(self, ui: &mut Ui) -> Option<SessionBarAction<K>> {
        let id = ui.make_persistent_id(self.id_salt);
        let (_, m) = kit(ui);
        let layout = BarLayout::for_width(ui.available_width(), m.control_h);
        let margin = Margin::symmetric(m.pad.x as i8, m.gap.y as i8);
        let mut action = surface_frame(ui, id, SurfaceKind::SessionCard, margin, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = (m.gap.y / 2.0).round();
            self.rows(ui, id, layout)
        })
        .inner;
        if let Some(error) = self.error.as_deref()
            && error_callout(ui, id, error)
        {
            action = action.or(Some(SessionBarAction::DismissError));
        }
        action
    }

    fn rows(&self, ui: &mut Ui, id: Id, layout: BarLayout) -> Option<SessionBarAction<K>> {
        let (_, m) = kit(ui);
        let (_, row) = ui.allocate_space(vec2(ui.available_width(), target(ui)));
        let left_w = self.status_group(ui, id, row, layout);
        let (right_w, mut action) = self.right_group(ui, id, row, layout);
        let span = title_span(
            row.x_range(),
            left_w,
            right_w,
            m.gap.x,
            TITLE_MIN * m.control_h,
        );
        let renamed = self.title(ui, id, Rect::from_x_y_ranges(span, row.y_range()), layout);
        action = action.or(renamed.map(SessionBarAction::Rename));
        let subtitle = self.subtitle.as_deref().filter(|s| !s.is_empty());
        if let Some(subtitle) = subtitle {
            let tapped = ui
                .data(|d| d.get_temp::<bool>(id.with("subtitle")))
                .unwrap_or(false);
            if !layout.compact() || tapped {
                subtitle_row(ui, subtitle, layout == BarLayout::Full);
            }
        }
        if !layout.folds() && !self.chips.is_empty() {
            let chips = self.chip_row(ui, id);
            action = action.or(chips);
        }
        action
    }

    /// Paints the status dot and label at the left of `row`; returns their width.
    fn status_group(&self, ui: &mut Ui, id: Id, row: Rect, layout: BarLayout) -> f32 {
        let p = ChatPalette::of(ui);
        let label = self.status_label.as_deref().unwrap_or(self.status.label());
        let dot = StatusDot::new(self.status.tone().color(&p))
            .pulse(self.status == SessionStatus::Working);
        let side = dot.side();
        let cy = row.center().y;
        dot.paint(ui, pos2(row.left() + side / 2.0, cy));
        let mut right = row.left() + side;
        if !layout.compact() {
            let galley = text_galley(ui, label, TextStyle::Small.resolve(ui.style()));
            let pos = pos2(right + DOT_GAP, (cy - galley.size().y / 2.0).round());
            right = pos.x + galley.size().x;
            ui.painter().galley(pos, galley, p.text_dim);
        }
        let hit = Rect::from_min_max(row.left_top(), pos2(right, row.bottom()));
        let response = ui.interact(hit, id.with("status"), Sense::hover());
        response.widget_info(|| WidgetInfo::labeled(egui::Role::Label, true, label));
        if layout.compact() {
            response.on_hover_text(label);
        }
        right - row.left()
    }

    /// Lays out the buttons, overflow menu and jump group from the right of `row`; returns their width.
    fn right_group(
        &self,
        ui: &mut Ui,
        id: Id,
        row: Rect,
        layout: BarLayout,
    ) -> (f32, Option<SessionBarAction<K>>) {
        let (_, m) = kit(ui);
        let side = target(ui);
        let icons = ChatIcons::get(ui.ctx());
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(row)
                .layout(Layout::right_to_left(Align::Center))
                .id_salt(id.with("right")),
        );
        child.spacing_mut().item_spacing.x = (m.gap.x / 4.0).round();
        let mut action = None;
        if let Some(nav) = self.nav {
            action =
                nav_group(&mut child, id, nav, !layout.compact(), side).map(SessionBarAction::Nav);
        }
        let folds = layout.folds();
        let has_menu =
            folds && (!self.chips.is_empty() || !self.buttons.is_empty() || self.renamable);
        let has_buttons = !folds && !self.buttons.is_empty();
        if self.nav.is_some() && (has_menu || has_buttons) {
            vline(&mut child, (side * RULE_SHARE).round());
        }
        if has_menu {
            let more = icon_button(
                &mut child,
                id.with("more"),
                icons.more,
                side,
                false,
                "More",
                true,
            );
            let picked = popup_menu(&more, false, |ui| self.overflow(ui, id)).flatten();
            action = action.or(picked);
        }
        if has_buttons {
            for (i, button) in self.buttons.iter().enumerate().rev() {
                if bar_button(&mut child, id.with(("button", i)), button, side).clicked() {
                    action = action.or(Some(SessionBarAction::Button(button.id.clone())));
                }
            }
        }
        (child.min_rect().width(), action)
    }

    /// Paints the title centred in `rect`, or the rename field while editing; returns a committed name.
    fn title(&self, ui: &mut Ui, id: Id, rect: Rect, layout: BarLayout) -> Option<String> {
        let owner = owner_of(self.session, &self.title);
        match ui.data(|d| d.get_temp::<RenameDraft>(rename_id(id))) {
            Some(draft) if draft.2 == owner => return self.rename_field(ui, id, rect, draft),
            Some(_) => ui.data_mut(|d| d.remove::<RenameDraft>(rename_id(id))),
            None => {}
        }
        let p = ChatPalette::of(ui);
        let size = TextStyle::Body.resolve(ui.style()).size;
        let galley = elided(ui, &self.title, semibold_font_sized(ui, size), rect.width());
        let cut = galley.elided;
        let pos = (rect.center() - galley.size() / 2.0).round();
        let hit = Rect::from_min_size(pos, galley.size());
        ui.painter().galley(pos, galley, p.card_text);
        let has_subtitle = self.subtitle.as_deref().is_some_and(|s| !s.is_empty());
        let taps = layout.compact() && has_subtitle;
        let sense = if self.renamable || taps {
            Sense::click()
        } else {
            Sense::hover()
        };
        let response = ui.interact(hit, id.with("title"), sense);
        response.widget_info(|| WidgetInfo::labeled(egui::Role::Label, true, &*self.title));
        if taps && response.clicked() {
            let key = id.with("subtitle");
            ui.data_mut(|d| {
                let shown = d.get_temp::<bool>(key).unwrap_or(false);
                d.insert_temp(key, !shown);
            });
        }
        if self.renamable && response.double_clicked() {
            start_rename(ui, id, &self.title, owner);
        }
        if cut {
            response.on_hover_text(&*self.title);
        }
        None
    }

    /// The rename field in `rect`; returns the new name when it is committed and differs.
    fn rename_field(&self, ui: &mut Ui, id: Id, rect: Rect, draft: RenameDraft) -> Option<String> {
        let (mut text, focused_once, owner) = draft;
        let edit_id = id.with("rename_edit");
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::left_to_right(Align::Center))
                .id_salt(id.with("rename_ui")),
        );
        let output = TextEdit::singleline(&mut text)
            .id(edit_id)
            .font(TextStyle::Body)
            .align(Align2::CENTER_CENTER)
            .desired_width(rect.width())
            .show(&mut child);
        let response = output.response;
        if !focused_once {
            response.request_focus();
            let mut state = output.state;
            state
                .cursor
                .set_char_range(Some(CCursorRange::select_all_str(&text)));
            state.store(ui.ctx(), edit_id);
            ui.data_mut(|d| d.insert_temp(rename_id(id), (text, true, owner)));
            return None;
        }
        let escaped = ui.input(|i| i.key_pressed(Key::Escape));
        if !escaped && !response.lost_focus() {
            ui.data_mut(|d| d.insert_temp(rename_id(id), (text, true, owner)));
            return None;
        }
        ui.data_mut(|d| d.remove::<RenameDraft>(rename_id(id)));
        let name = text.trim();
        (!escaped && !name.is_empty() && name != self.title.as_ref()).then(|| name.to_owned())
    }

    /// The overflow menu: rename, folded chips and folded buttons.
    fn overflow(&self, ui: &mut Ui, id: Id) -> Option<SessionBarAction<K>> {
        let mut action = None;
        if self.renamable && menu_item(ui, None, "Rename").clicked() {
            start_rename(ui, id, &self.title, owner_of(self.session, &self.title));
        }
        if self.renamable && !self.chips.is_empty() {
            menu_separator(ui);
        }
        for chip in self.chips {
            action = action.or(chip_menu(ui, chip));
        }
        if !self.buttons.is_empty() && (self.renamable || !self.chips.is_empty()) {
            menu_separator(ui);
        }
        for button in self.buttons {
            let caption = button_caption(button);
            let clicked = MenuItem::new(&caption)
                .icon(button.icon)
                .checked(button.selected)
                .show(ui)
                .clicked();
            if clicked {
                action = action.or(Some(SessionBarAction::Button(button.id.clone())));
            }
        }
        action
    }

    /// One centred row of chips; those that do not fit open from a more button.
    fn chip_row(&self, ui: &mut Ui, id: Id) -> Option<SessionBarAction<K>> {
        let (_, m) = kit(ui);
        let side = target(ui);
        let gap = (m.gap.x * 0.75).round();
        let widths: Vec<f32> = self.chips.iter().map(|c| chip_width(ui, c)).collect();
        let avail = ui.available_width();
        let shown = fit_count(&widths, gap, avail, side);
        let rest = shown < self.chips.len();
        let used = widths[..shown].iter().sum::<f32>()
            + gap * shown.saturating_sub(1) as f32
            + if rest { gap + side } else { 0.0 };
        let (_, row) = ui.allocate_space(vec2(avail, side));
        let left = (row.center().x - used / 2.0).max(row.left()).round();
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(Rect::from_min_max(pos2(left, row.top()), row.max))
                .layout(Layout::left_to_right(Align::Center))
                .id_salt(id.with("chips")),
        );
        child.spacing_mut().item_spacing.x = gap;
        let mut action = None;
        for (i, chip) in self.chips[..shown].iter().enumerate() {
            let picked = show_chip(&mut child, id.with(("chip", i)), chip);
            action = action.or(picked);
        }
        if rest {
            let icons = ChatIcons::get(ui.ctx());
            let more = icon_button(
                &mut child,
                id.with("more_chips"),
                icons.more,
                side,
                false,
                "More options",
                true,
            );
            let picked = popup_menu(&more, false, |ui| {
                let mut picked = None;
                for chip in &self.chips[shown..] {
                    picked = picked.or(chip_menu(ui, chip));
                }
                picked
            })
            .flatten();
            action = action.or(picked);
        }
        action
    }
}

fn rename_id(id: Id) -> Id {
    id.with("rename")
}

/// The session hash, else the title's hash.
pub(super) fn owner_of(session: Option<u64>, title: &str) -> u64 {
    session.unwrap_or_else(|| egui::util::hash(title))
}

pub(super) fn start_rename(ui: &Ui, id: Id, title: &str, owner: u64) {
    ui.data_mut(|d| d.insert_temp(rename_id(id), (title.to_owned(), false, owner)));
}

/// Subtitle centred under the title: wrapped, or cut to one line.
fn subtitle_row(ui: &mut Ui, subtitle: &str, wrap: bool) {
    let p = ChatPalette::of(ui);
    let font = TextStyle::Small.resolve(ui.style());
    let width = ui.available_width();
    if wrap {
        let mut job = LayoutJob::simple(subtitle.to_owned(), font, p.text_dim, width);
        job.halign = Align::Center;
        let galley = ui.painter().layout_job(job);
        let (_, rect) = ui.allocate_space(vec2(width, galley.size().y));
        ui.painter().galley(rect.center_top(), galley, p.text_dim);
        return;
    }
    let galley = elided(ui, subtitle, font, width);
    let cut = galley.elided;
    let (_, rect) = ui.allocate_space(vec2(width, galley.size().y));
    let pos = pos2(
        (rect.center().x - galley.size().x / 2.0).round(),
        rect.top(),
    );
    let hit = Rect::from_min_size(pos, galley.size());
    ui.painter().galley(pos, galley, p.text_dim);
    if cut {
        ui.interact(hit, ui.make_persistent_id("subtitle"), Sense::hover())
            .on_hover_text(subtitle);
    }
}

/// The jump group, laid out right to left; returns the step the user took.
fn nav_group(ui: &mut Ui, id: Id, nav: MessageNav, labelled: bool, side: f32) -> Option<NavStep> {
    let p = ChatPalette::of(ui);
    let icons = ChatIcons::get(ui.ctx());
    let (prev_ok, next_ok) = nav_steps(nav);
    let mut step = None;
    let next = icon_button(
        ui,
        id.with("nav_next"),
        icons.chevron_down,
        side,
        false,
        "Next of your messages",
        next_ok,
    );
    if next.clicked() {
        step = Some(NavStep::Next);
    }
    if let Some(i) = counter(ui, id, nav, side) {
        step = step.or(Some(NavStep::Pick(i)));
    }
    let prev = icon_button(
        ui,
        id.with("nav_prev"),
        icons.chevron_up,
        side,
        false,
        "Previous of your messages",
        prev_ok,
    );
    if prev.clicked() {
        step = step.or(Some(NavStep::Prev));
    }
    if labelled {
        let galley = text_galley(ui, "Your messages", TextStyle::Small.resolve(ui.style()));
        let (_, rect) = ui.allocate_space(vec2(whole(galley.size().x), side));
        let pos = pos2(
            rect.left(),
            (rect.center().y - galley.size().y / 2.0).round(),
        );
        ui.painter().galley(pos, galley, p.text_dim);
    }
    step
}

/// The `n/N` counter; clicking it lists the messages to pick from.
fn counter(ui: &mut Ui, id: Id, nav: MessageNav, side: f32) -> Option<usize> {
    let (t, m) = kit(ui);
    let p = ChatPalette::of(ui);
    let font = medium_font(ui, TextStyle::Small);
    let text = nav_text(nav);
    let widest = nav_text(MessageNav {
        current: Some(nav.total.saturating_sub(1)),
        total: nav.total,
    });
    let galley = text_galley(ui, &text, font.clone());
    let reserve = text_galley(ui, &widest, font).size().x;
    let w = whole(galley.size().x.max(reserve) + 2.0 * COUNTER_PAD);
    let (_, rect) = ui.allocate_space(vec2(w, side));
    let sense = if nav.total > 0 {
        Sense::click()
    } else {
        Sense::hover()
    };
    let response = ui.interact(rect, id.with("nav_count"), sense);
    response.widget_info(|| WidgetInfo::labeled(egui::Role::Button, nav.total > 0, &text));
    if ui.is_rect_visible(rect) {
        if response.hovered() && nav.total > 0 {
            ui.painter().rect_filled(rect, t.radius_sm, t.hover);
        }
        let pos = (rect.center() - galley.size() / 2.0).round();
        ui.painter().galley(pos, galley, p.text_dim);
    }
    let row_h = m.row_h;
    popup_menu(&response, false, |ui| {
        let mut picked = None;
        ScrollArea::vertical()
            .max_height(PICK_MAX_H)
            .id_salt(id.with("nav_pick"))
            .show_rows(ui, row_h, nav.total, |ui, rows| {
                for row in rows {
                    let i = nav.total - 1 - row;
                    let label = format!("Message {} of {}", i + 1, nav.total);
                    if menu_check_item(ui, None, &label, nav.current == Some(i)).clicked() {
                        picked = Some(i);
                    }
                }
            });
        picked
    })
    .flatten()
}

/// The error callout under the card; returns whether it was dismissed.
fn error_callout(ui: &mut Ui, id: Id, error: &str) -> bool {
    let (_, m) = kit(ui);
    let p = ChatPalette::of(ui);
    let icons = ChatIcons::get(ui.ctx());
    let side = target(ui);
    ui.add_space(m.gap.y / 2.0);
    callout(ui, p.error, icons.error, |ui| {
        ui.horizontal_top(|ui| {
            let w = (ui.available_width() - side - m.gap.x).max(0.0);
            ui.allocate_ui(vec2(w, 0.0), |ui| {
                ui.add(Label::new(RichText::new(error).color(p.text)).wrap());
            });
            icon_button(
                ui,
                id.with("dismiss"),
                icons.close,
                side,
                false,
                "Dismiss",
                true,
            )
            .clicked()
        })
        .inner
    })
}
