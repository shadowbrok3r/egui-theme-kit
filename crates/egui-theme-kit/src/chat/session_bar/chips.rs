//! Option chips of the title card, inline or folded into a menu.

use std::sync::Arc;

use egui::{Galley, Id, Rect, Response, Role, Sense, TextStyle, Ui, Vec2, WidgetInfo, pos2, vec2};

use super::parts::target;
use super::{ChipKind, OptionChip, SessionBarAction};
use crate::chat::icons::ChatIcons;
use crate::chat::palette::{ChatPalette, MIN_TEXT_CONTRAST, legible};
use crate::chat::surface::{SurfaceKind, paint_surface};
use crate::chat::types::Tone;
use crate::controls::{
    MenuItem, State, focus_ring, kit, medium_font, menu_check_item, menu_item, popup_menu,
    state_of, text_galley, whole,
};

/// Inset of a chip's content from its edges.
const PAD_X: f32 = 8.0;
/// Gap between the parts of a chip.
const PART_GAP: f32 = 5.0;
/// Size of a chip's meter.
const METER_W: f32 = 28.0;
const METER_H: f32 = 4.0;
/// Opacity of the hover wash over a chip.
const HOVER_WASH: f32 = 0.6;

/// Laid-out parts of one chip.
struct Parts {
    label: Option<Arc<Galley>>,
    value: Arc<Galley>,
    icon: bool,
    meter: Option<f32>,
    caret: bool,
}

fn parts<K>(ui: &Ui, chip: &OptionChip<'_, K>) -> Parts {
    let label = (!chip.label.is_empty())
        .then(|| text_galley(ui, &chip.label, TextStyle::Small.resolve(ui.style())));
    let value = text_galley(ui, &chip.value, medium_font(ui, TextStyle::Button));
    let meter = match chip.kind {
        ChipKind::Meter { fraction, .. } => Some(fraction.clamp(0.0, 1.0)),
        _ => None,
    };
    Parts {
        label,
        value,
        icon: chip.icon.is_some(),
        meter,
        caret: opens_menu(&chip.kind),
    }
}

/// Whether clicking a chip of `kind` opens a menu.
fn opens_menu(kind: &ChipKind<'_>) -> bool {
    matches!(
        kind,
        ChipKind::Menu { .. }
            | ChipKind::Meter {
                action: Some(_),
                ..
            }
    )
}

/// Whether a chip of `kind` reacts to clicks.
fn clickable(kind: &ChipKind<'_>) -> bool {
    opens_menu(kind) || matches!(kind, ChipKind::Toggle { .. })
}

fn caret_size(ui: &Ui) -> f32 {
    kit(ui).1.icon_sm - 2.0
}

fn width_of(ui: &Ui, parts: &Parts) -> f32 {
    let (_, m) = kit(ui);
    let mut w = 2.0 * PAD_X + parts.value.size().x;
    if parts.icon {
        w += m.icon_sm + PART_GAP;
    }
    if let Some(label) = &parts.label {
        w += label.size().x + PART_GAP;
    }
    if parts.meter.is_some() {
        w += PART_GAP + METER_W;
    }
    if parts.caret {
        w += PART_GAP + caret_size(ui);
    }
    whole(w)
}

/// Width a chip takes in the chip row.
pub(super) fn chip_width<K>(ui: &Ui, chip: &OptionChip<'_, K>) -> f32 {
    width_of(ui, &parts(ui, chip))
}

/// `label: value`, or the value alone.
fn caption<K>(chip: &OptionChip<'_, K>) -> String {
    if chip.label.is_empty() {
        chip.value.to_string()
    } else {
        format!("{}: {}", chip.label, chip.value)
    }
}

/// Colour of a chip's value and meter; neutral chips use the chip text and the accent.
fn tone_ink(tone: Tone, p: &ChatPalette) -> (egui::Color32, egui::Color32) {
    match tone {
        Tone::Neutral => (
            legible(p.text, p.chip_fill, p.base, MIN_TEXT_CONTRAST),
            p.accent,
        ),
        tone => {
            let c = tone.color(p);
            (legible(c, p.chip_fill, p.base, MIN_TEXT_CONTRAST), c)
        }
    }
}

/// Draws `chip` under `id`; returns what a click or its menu produced.
pub(super) fn show_chip<K: Clone>(
    ui: &mut Ui,
    id: Id,
    chip: &OptionChip<'_, K>,
) -> Option<SessionBarAction<K>> {
    let parts = parts(ui, chip);
    let size = vec2(width_of(ui, &parts), target(ui));
    let (_, rect) = ui.allocate_space(size);
    let sense = if clickable(&chip.kind) {
        Sense::click()
    } else {
        Sense::hover()
    };
    let response = ui.interact(rect, id, sense);
    let text = caption(chip);
    response.widget_info(|| match chip.kind {
        ChipKind::Toggle { on } => WidgetInfo::selected(Role::CheckBox, true, on, &text),
        ChipKind::Menu { .. } => WidgetInfo::labeled(Role::ComboBox, true, &text),
        _ => WidgetInfo::labeled(Role::Button, true, &text),
    });
    let action = match &chip.kind {
        ChipKind::Menu { options, selected } => popup_menu(&response, false, |ui| {
            let mut picked = None;
            for (i, option) in options.iter().enumerate() {
                if menu_check_item(ui, None, option, *selected == Some(i)).clicked() {
                    picked = Some(i);
                }
            }
            picked
        })
        .flatten()
        .map(|choice| SessionBarAction::Chip {
            id: chip.id.clone(),
            choice,
        }),
        ChipKind::Meter {
            action: Some(label),
            ..
        } => popup_menu(&response, false, |ui| menu_item(ui, None, label).clicked())
            .filter(|clicked| *clicked)
            .map(|_| SessionBarAction::ChipAction(chip.id.clone())),
        ChipKind::Toggle { on } => response.clicked().then(|| SessionBarAction::ChipToggle {
            id: chip.id.clone(),
            on: !on,
        }),
        _ => None,
    };
    if ui.is_rect_visible(rect) {
        paint_chip(ui, rect, &response, chip, parts);
    }
    action
}

fn paint_chip<K>(ui: &Ui, rect: Rect, response: &Response, chip: &OptionChip<'_, K>, parts: Parts) {
    let (t, m) = kit(ui);
    let p = ChatPalette::of(ui);
    paint_surface(ui, rect, SurfaceKind::Chip);
    let on = matches!(chip.kind, ChipKind::Toggle { on: true });
    if on {
        ui.painter().rect_filled(rect, p.radius_chip, p.accent_soft);
    }
    let (state, focused) = state_of(ui, response);
    if clickable(&chip.kind) && state != State::Rest {
        let wash = if state == State::Pressed {
            t.pressed
        } else {
            t.hover
        };
        ui.painter()
            .rect_filled(rect, p.radius_chip, wash.gamma_multiply(HOVER_WASH));
    }
    let (value_ink, meter_ink) = if on {
        let ink = legible(p.accent, p.accent_soft, p.base, MIN_TEXT_CONTRAST);
        (ink, p.accent)
    } else {
        tone_ink(chip.tone, &p)
    };
    let cy = rect.center().y;
    let mut x = rect.left() + PAD_X;
    if let Some(icon) = chip.icon {
        let slot = Rect::from_center_size(pos2(x + m.icon_sm / 2.0, cy), Vec2::splat(m.icon_sm));
        icon.paint(ui, slot, m.icon_sm, meter_ink);
        x = slot.right() + PART_GAP;
    }
    if let Some(label) = parts.label {
        let w = label.size().x;
        let y = (cy - label.size().y / 2.0).round();
        let ink = if on { p.soft_dim } else { p.chip_text };
        ui.painter().galley(pos2(x, y), label, ink);
        x += w + PART_GAP;
    }
    let w = parts.value.size().x;
    let y = (cy - parts.value.size().y / 2.0).round();
    ui.painter().galley(pos2(x, y), parts.value, value_ink);
    x += w;
    if let Some(fraction) = parts.meter {
        let track = Rect::from_min_size(
            pos2(x + PART_GAP, (cy - METER_H / 2.0).round()),
            vec2(METER_W, METER_H),
        );
        ui.painter()
            .rect_filled(track, METER_H / 2.0, p.chip_stroke);
        if fraction > 0.0 {
            let fill = Rect::from_min_size(track.min, vec2(METER_W * fraction, METER_H));
            ui.painter().rect_filled(fill, METER_H / 2.0, meter_ink);
        }
        x = track.right();
    }
    if parts.caret {
        let side = caret_size(ui);
        let slot = Rect::from_center_size(pos2(x + PART_GAP + side / 2.0, cy), Vec2::splat(side));
        let icons = ChatIcons::get(ui.ctx());
        let ink = if on { p.soft_dim } else { p.chip_text };
        icons.chevron_down.paint(ui, slot, side, ink);
    }
    if focused {
        focus_ring(ui, rect, p.radius_chip, &t);
    }
}

/// `chip` as rows of an open menu; returns what a pick produced.
pub(super) fn chip_menu<K: Clone>(
    ui: &mut Ui,
    chip: &OptionChip<'_, K>,
) -> Option<SessionBarAction<K>> {
    let text = caption(chip);
    let row = MenuItem::new(&text).icon(chip.icon);
    match &chip.kind {
        ChipKind::Menu { options, selected } => row
            .submenu(ui, |ui| {
                let mut picked = None;
                for (i, option) in options.iter().enumerate() {
                    if menu_check_item(ui, None, option, *selected == Some(i)).clicked() {
                        picked = Some(i);
                    }
                }
                picked
            })
            .flatten()
            .map(|choice| SessionBarAction::Chip {
                id: chip.id.clone(),
                choice,
            }),
        ChipKind::Meter { action, .. } => {
            let _ = row.enabled(false).show(ui);
            let label = action.as_deref()?;
            menu_item(ui, None, label)
                .clicked()
                .then(|| SessionBarAction::ChipAction(chip.id.clone()))
        }
        ChipKind::Toggle { on } => {
            row.checked(*on)
                .show(ui)
                .clicked()
                .then(|| SessionBarAction::ChipToggle {
                    id: chip.id.clone(),
                    on: !on,
                })
        }
        ChipKind::Info => {
            let _ = row.enabled(false).show(ui);
            None
        }
    }
}
