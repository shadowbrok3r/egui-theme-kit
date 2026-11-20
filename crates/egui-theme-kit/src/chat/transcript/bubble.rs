//! User bubbles, unframed agent messages, notices, dividers and the load-earlier row.

use egui::{
    Align, Image, Layout, Margin, Rect, Response, Sense, Stroke, TextFormat, Ui, Vec2, pos2, vec2,
};

use super::LoadState;
use super::entry::{AgentMessage, AttachmentChip, Delivery, Notice, UserMessage};
use super::markdown::{self, wrap_width};
use super::rich::{Rich, one_row, plain};
use super::row::RowCx;
use crate::chat::mention::{WireSegment, segments};
use crate::chat::surface::{SurfaceKind, paint_surface, surface_frame};
use crate::chat::types::{ChatAction, Tone, item_id};
use crate::controls::{IconBox, IconButton, SpinnerSize, TextButton, icons, tinted_spinner};
use crate::kit::ButtonKind;

/// Share of the column a user bubble may take.
const BUBBLE_SHARE: f32 = 0.75;
/// Narrowest bubble cap, in body font sizes.
const BUBBLE_MIN_EM: f32 = 22.4;
/// Column width below which bubbles take the full width, in body font sizes.
const NARROW_EM: f32 = 33.6;
/// Side of an attachment thumbnail, in body font sizes.
const THUMB_EM: f32 = 4.0;

/// Widest a user bubble gets in a column `avail` wide with body text `em` points tall.
pub(crate) fn bubble_cap(avail: f32, em: f32) -> f32 {
    if avail < NARROW_EM * em {
        avail
    } else {
        (avail * BUBBLE_SHARE).max(BUBBLE_MIN_EM * em).min(avail)
    }
}

/// A right-aligned user bubble with attachments and a meta row: delivery, time and copy.
pub(crate) fn user(ui: &mut Ui, rc: &mut RowCx<'_>, m: &UserMessage<'_>) {
    let avail = wrap_width(ui);
    let pad = vec2(rc.m.pad.x + 2.0, rc.m.pad.y + 3.0);
    let cap = bubble_cap(avail, rc.f.body.size);
    let mut rich = Rich::new(cap - 2.0 * pad.x);
    let ink = rc.p.user_text;
    let ctx = ui.ctx().clone();
    for seg in segments(&m.text) {
        match seg {
            WireSegment::Text(text) => {
                rich.text(&ctx, text, TextFormat::simple(rc.f.body.clone(), ink));
            }
            WireSegment::Mention(target) => {
                let font = rc.f.body.clone();
                rich.mention(ui, rc, &target, &font, true);
            }
        }
    }
    let (galley, targets) = rich.layout(&ctx);
    let size = galley.size() + 2.0 * pad;
    let (row, _) = ui.allocate_exact_size(vec2(avail, size.y), Sense::hover());
    let bubble = Rect::from_min_size(pos2(row.right() - size.x, row.top()), size);
    paint_surface(ui, bubble, SurfaceKind::UserBubble);
    let bubble_resp = ui.interact(bubble, item_id(rc.id, &m.key, "bubble"), Sense::click());
    let text_rect = Rect::from_min_size(bubble.min + pad, galley.size());
    let label = ui.place(
        text_rect,
        egui::Label::new(galley.clone())
            .selectable(rc.selectable)
            .sense(Sense::click()),
    );
    targets.handle(ui, rc, &galley, &label);
    let whole = bubble_resp.union(label.clone());
    let failed = m.delivery == Delivery::Failed;
    message_menu(&whole, rc, &m.key, &m.text, failed);

    if !m.attachments.is_empty() {
        ui.add_space(rc.m.gap.y / 2.0);
        attachments(ui, rc, &m.attachments);
    }
    let meta_rect = Rect::from_min_size(
        ui.cursor().min,
        vec2(ui.available_width(), IconBox::Sm.side(&rc.m)),
    );
    let hovered = ui.rect_contains_pointer(bubble.union(meta_rect)) || label.hovered();
    meta(ui, rc, m, hovered);
}

/// Copy, and resend or edit for failed messages, on right-click or long-press.
fn message_menu(response: &Response, rc: &mut RowCx<'_>, key: &str, wire: &str, failed: bool) {
    response.context_menu(|ui| {
        if ui.button("Copy").clicked() {
            rc.copy(ui, wire.to_owned());
            ui.close();
        }
        if failed {
            if ui.button("Send again").clicked() {
                rc.actions.push(ChatAction::Resend(key.to_owned()));
                ui.close();
            }
            if ui.button("Edit and send").clicked() {
                rc.actions.push(ChatAction::EditResend(key.to_owned()));
                ui.close();
            }
        }
    });
}

/// Delivery state, time and copy under a user bubble; time and copy show on hover or touch.
fn meta(ui: &mut Ui, rc: &mut RowCx<'_>, m: &UserMessage<'_>, hovered: bool) {
    let side = IconBox::Sm.side(&rc.m);
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), side),
        Layout::right_to_left(Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = rc.m.gap.x / 2.0;
            ui.set_min_height(side);
            let reveal = hovered || rc.touch;
            if reveal {
                let copy = IconButton::new(rc.icons.copy, IconBox::Sm)
                    .tip("Copy message")
                    .show(ui);
                if rc.tapped(&copy) {
                    rc.copy(ui, m.text.to_string());
                }
                if let Some(t) = m.time {
                    let label = rc.time(t);
                    small(ui, rc, &label, rc.p.text_faint);
                }
                if let Some(author) = &m.author {
                    small(ui, rc, author, rc.p.text_faint);
                }
            }
            match m.delivery {
                Delivery::Sent => {}
                Delivery::Queued => {
                    small(ui, rc, "Queued", rc.p.text_dim);
                    icon(ui, rc, rc.icons.queue, rc.p.text_dim);
                }
                Delivery::Steered => {
                    small(ui, rc, "Steered", rc.p.accent);
                    icon(ui, rc, rc.icons.steer, rc.p.accent);
                }
                Delivery::Failed => {
                    let edit = ui.link("Edit");
                    if rc.tapped(&edit) {
                        rc.actions.push(ChatAction::EditResend(m.key.to_string()));
                    }
                    let retry = ui.link("Retry");
                    if rc.tapped(&retry) {
                        rc.actions.push(ChatAction::Resend(m.key.to_string()));
                    }
                    small(ui, rc, "Not sent", rc.p.error);
                    icon(ui, rc, rc.icons.error, rc.p.error);
                }
            }
        },
    );
}

fn small(ui: &mut Ui, rc: &RowCx<'_>, text: &str, color: egui::Color32) {
    let galley = one_row(ui, text, &rc.f.small, color, ui.available_width());
    ui.add(egui::Label::new(galley).selectable(false));
}

fn icon(ui: &mut Ui, rc: &RowCx<'_>, icon: crate::controls::Icon, color: egui::Color32) {
    icon.show_tinted(ui, rc.m.icon_sm, color);
}

/// Attachment chips, right-aligned: a thumbnail or file icon, the name, and a spinner while reading.
fn attachments(ui: &mut Ui, rc: &RowCx<'_>, chips: &[AttachmentChip<'_>]) {
    ui.with_layout(
        Layout::right_to_left(Align::Min).with_main_wrap(true),
        |ui| {
            ui.spacing_mut().item_spacing = vec2(rc.m.gap.x / 2.0, rc.m.gap.y / 2.0);
            for (n, chip) in chips.iter().enumerate() {
                let margin = Margin::symmetric(rc.m.pad.x as i8 / 2, (rc.m.pad.y / 2.0) as i8);
                surface_frame(ui, ("attachment", n), SurfaceKind::Chip, margin, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = rc.m.gap.x / 2.0;
                        match chip.image {
                            Some(texture) => {
                                let side = rc.f.body.size * THUMB_EM;
                                ui.add(
                                    Image::from_texture((texture, Vec2::splat(side)))
                                        .fit_to_exact_size(Vec2::splat(side))
                                        .corner_radius(rc.p.radius_chip),
                                );
                            }
                            None => {
                                icons::FILE.show_tinted(ui, rc.m.icon_sm, rc.p.chip_text);
                            }
                        }
                        let max = rc.f.body.size * 16.0;
                        let galley = one_row(ui, &chip.name, &rc.f.small, rc.p.chip_text, max);
                        ui.add(egui::Label::new(galley).selectable(false))
                            .on_hover_text(chip.name.as_ref());
                        if chip.reading {
                            tinted_spinner(ui, SpinnerSize::Sm, rc.p.running);
                        }
                    });
                });
            }
        },
    );
}

/// An agent reply: a faint name and time line when `header`, then unframed markdown.
pub(crate) fn agent(ui: &mut Ui, rc: &mut RowCx<'_>, m: &AgentMessage<'_>, header: bool) {
    if header && (m.author.is_some() || m.time.is_some()) {
        let mut line = m.author.as_deref().unwrap_or("").to_owned();
        if let Some(t) = m.time {
            if !line.is_empty() {
                line.push_str("  \u{b7}  ");
            }
            line.push_str(&rc.time(t));
        }
        let font = rc.f.small.clone();
        plain(ui, rc, &line, &font, rc.p.text_faint);
    }
    let top = ui.cursor().top();
    let id = item_id(rc.id, &m.key, "body");
    let ink = rc.p.text;
    if m.text.trim().is_empty() && m.streaming {
        tinted_spinner(ui, SpinnerSize::Sm, rc.p.running);
    } else {
        markdown::show(ui, rc, &m.text, ink, id, false);
        if m.streaming {
            tinted_spinner(ui, SpinnerSize::Sm, rc.p.running);
        }
    }
    if m.streaming {
        return;
    }
    if rc.touch {
        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
            let copy = IconButton::new(rc.icons.copy, IconBox::Sm)
                .tip("Copy message")
                .show(ui);
            if rc.tapped(&copy) {
                rc.copy(ui, m.text.to_string());
            }
        });
        return;
    }
    let body = Rect::from_x_y_ranges(ui.min_rect().x_range(), top..=ui.min_rect().bottom());
    if ui.rect_contains_pointer(body) {
        let side = IconBox::Sm.side(&rc.m);
        let spot = Rect::from_min_size(pos2(body.right() - side, body.top()), Vec2::splat(side));
        ui.painter().rect_filled(spot, rc.p.radius_chip, rc.p.base);
        let copy = ui.place(spot, |ui: &mut Ui| {
            IconButton::new(rc.icons.copy, IconBox::Sm)
                .tip("Copy message")
                .show(ui)
        });
        if rc.tapped(&copy) {
            rc.copy(ui, m.text.to_string());
        }
    }
}

/// A faint centred line with an optional time.
pub(crate) fn notice(ui: &mut Ui, rc: &RowCx<'_>, n: &Notice<'_>) {
    let color = match n.tone {
        Tone::Neutral => rc.p.text_faint,
        tone => tone.color(&rc.p),
    };
    let mut text = n.text.to_string();
    if let Some(t) = n.time {
        text.push_str("  \u{b7}  ");
        text.push_str(&rc.time(t));
    }
    let width = wrap_width(ui);
    ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Center), |ui| {
        let text = super::rich::renderable(ui.ctx(), &rc.f.small, &text);
        let mut job = egui::text::LayoutJob::simple(
            text.into_owned(),
            rc.f.small.clone(),
            color,
            width * 0.85,
        );
        job.halign = Align::Center;
        let galley = ui.ctx().fonts_mut(|f| f.layout_job(job));
        ui.add(egui::Label::new(galley).selectable(rc.selectable));
    });
}

/// A rule across the column with a centred caption.
pub(crate) fn divider(ui: &mut Ui, rc: &RowCx<'_>, caption: &str) {
    let width = wrap_width(ui);
    let h = RowCx::row_h(ui, &rc.f.small);
    let (rect, _) = ui.allocate_exact_size(vec2(width, h), Sense::hover());
    let stroke = Stroke::new(1.0, rc.p.separator);
    let y = rect.center().y;
    if caption.is_empty() {
        ui.painter().hline(rect.x_range(), y, stroke);
        return;
    }
    let galley = one_row(ui, caption, &rc.f.small, rc.p.text_faint, width * 0.7);
    let gap = rc.m.gap.x;
    let left = rect.center().x - galley.size().x / 2.0;
    ui.painter().hline(rect.left()..=left - gap, y, stroke);
    ui.painter()
        .hline(left + galley.size().x + gap..=rect.right(), y, stroke);
    let pos = pos2(left, y - galley.size().y / 2.0);
    ui.painter().galley(pos, galley, rc.p.text_faint);
}

/// The row above the first entry that asks for earlier history.
pub(crate) fn load_older(ui: &mut Ui, rc: &mut RowCx<'_>, state: LoadState) {
    let width = wrap_width(ui);
    ui.allocate_ui_with_layout(
        vec2(width, rc.m.control_h),
        Layout::top_down(Align::Center),
        |ui| match state {
            LoadState::Available => {
                let button = TextButton::new("Load earlier messages")
                    .kind(ButtonKind::Ghost)
                    .icon(rc.icons.chevron_up)
                    .show(ui);
                if rc.tapped(&button) {
                    rc.actions.push(ChatAction::LoadOlder);
                }
            }
            LoadState::Loading => {
                ui.horizontal(|ui| {
                    tinted_spinner(ui, SpinnerSize::Sm, rc.p.running);
                    small(ui, rc, "Loading earlier messages", rc.p.text_dim);
                });
            }
            LoadState::Failed => {
                ui.horizontal(|ui| {
                    small(ui, rc, "Earlier messages did not load", rc.p.error);
                    let retry = TextButton::new("Retry").kind(ButtonKind::Ghost).show(ui);
                    if rc.tapped(&retry) {
                        rc.actions.push(ChatAction::LoadOlder);
                    }
                });
            }
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bubbles_take_three_quarters_with_a_floor_and_the_full_width_when_narrow() {
        let em = 12.5;
        assert_eq!(bubble_cap(400.0, em), 400.0);
        assert_eq!(bubble_cap(1000.0, em), 750.0);
        assert_eq!(bubble_cap(430.0, em), 322.5);
        assert!(bubble_cap(500.0, em) >= 280.0);
        assert_eq!(bubble_cap(500.0, em * 1.3), 500.0);
    }
}
