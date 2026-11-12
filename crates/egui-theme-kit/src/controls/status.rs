//! Status dots with an optional pulsing halo, and keycap shortcut hints.

use std::sync::Arc;

use egui::{
    Color32, Context, Galley, KeyboardShortcut, ModifierNames, Rect, Response, Role, Sense, Stroke,
    StrokeKind, Ui, Vec2, Widget, WidgetInfo, pos2, vec2,
};

use super::{Icon, icons, kit, medium_font_sized, mix, text_galley, whole};

/// Default status dot diameter.
pub const DOT_D: f32 = 8.0;
/// Seconds per pulse of a dot's halo.
const PULSE_SECS: f32 = 1.6;
/// How far a halo grows past the dot, as a share of the dot's diameter.
const PULSE_REACH: f32 = 0.9;
/// Opacity of a halo as it leaves the dot.
const PULSE_ALPHA: f32 = 0.45;
/// Space a dot allocates around itself for its halo, per side.
const DOT_PAD: f32 = 3.0;
/// Keycap text size.
const KEY_PT: f32 = 10.5;
/// Keycap height.
const KEY_H: f32 = 18.0;
/// Horizontal padding inside a keycap.
const KEY_PAD_X: f32 = 5.0;
/// Gap between the caps of one shortcut.
const KEY_GAP: f32 = 3.0;
/// Depth of a keycap's lower edge.
const KEY_DEPTH: f32 = 1.5;
/// Share of the text colour in a keycap's lower edge.
const KEY_EDGE: f32 = 0.18;
/// Size of an arrow drawn in a keycap.
const KEY_ARROW: f32 = 10.0;

/// A small dot in a status tone, with a halo that pulses outward while `pulse` is set.
#[derive(Clone, Copy, Debug)]
#[must_use = "call `show` or `paint` to draw the dot"]
pub struct StatusDot<'a> {
    tone: Color32,
    diameter: f32,
    pulse: bool,
    label: &'a str,
}

impl<'a> StatusDot<'a> {
    pub fn new(tone: Color32) -> Self {
        Self {
            tone,
            diameter: DOT_D,
            pulse: false,
            label: "",
        }
    }

    pub fn diameter(mut self, diameter: f32) -> Self {
        self.diameter = diameter.max(2.0);
        self
    }

    /// A halo that grows and fades in a loop; repaints only while set.
    pub fn pulse(mut self, pulse: bool) -> Self {
        self.pulse = pulse;
        self
    }

    /// Tooltip and accessible name.
    pub fn label(mut self, label: &'a str) -> Self {
        self.label = label;
        self
    }

    /// Side of the square the dot allocates, halo room included.
    pub fn side(&self) -> f32 {
        whole(self.diameter + 2.0 * DOT_PAD)
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(self.side()), Sense::hover());
        response.widget_info(|| WidgetInfo::labeled(Role::Label, ui.is_enabled(), self.label));
        if ui.is_rect_visible(rect) {
            self.paint(ui, rect.center());
        }
        if self.label.is_empty() {
            response
        } else {
            response.on_hover_text(self.label)
        }
    }

    /// Paints the dot and its halo centred on `center`.
    pub fn paint(&self, ui: &Ui, center: egui::Pos2) {
        let r = self.diameter / 2.0;
        if self.pulse && ui.is_enabled() {
            let time = ui.input(|i| i.time) as f32;
            let phase = (time / PULSE_SECS).fract();
            let halo = r + self.diameter * PULSE_REACH * phase;
            let alpha = PULSE_ALPHA * (1.0 - phase);
            ui.painter()
                .circle_filled(center, halo, self.tone.gamma_multiply(alpha));
            ui.ctx().request_repaint();
        }
        let tone = if ui.is_enabled() {
            self.tone
        } else {
            self.tone.gamma_multiply(0.5)
        };
        ui.painter().circle_filled(center, r, tone);
    }
}

impl Widget for StatusDot<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        self.show(ui)
    }
}

/// A status dot in `tone`, pulsing while `pulse` is set.
pub fn status_dot(ui: &mut Ui, tone: Color32, pulse: bool) -> Response {
    StatusDot::new(tone).pulse(pulse).show(ui)
}

/// The keys of a shortcut written as `Ctrl+Shift+K`; a `+` right after a separator is the plus key.
pub fn split_keys(shortcut: &str) -> Vec<&str> {
    let mut keys = Vec::new();
    let mut start = 0;
    let bytes = shortcut.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'+' && i > start {
            keys.push(shortcut[start..i].trim());
            start = i + 1;
        }
        i += 1;
    }
    if start < shortcut.len() {
        keys.push(shortcut[start..].trim());
    }
    keys.retain(|k| !k.is_empty());
    keys
}

/// A painted arrow for an arrow key's name.
fn arrow_icon(key: &str) -> Option<Icon> {
    match key.to_ascii_lowercase().as_str() {
        "up" | "arrowup" => Some(icons::CARET_UP),
        "down" | "arrowdown" => Some(icons::CARET_DOWN),
        "left" | "arrowleft" => Some(icons::CARET_LEFT),
        "right" | "arrowright" => Some(icons::CARET_RIGHT),
        _ => None,
    }
}

/// `shortcut` spelt with plain modifier names for the context's platform, e.g. `Ctrl+K`.
pub fn shortcut_text(ctx: &Context, shortcut: &KeyboardShortcut) -> String {
    let is_mac = matches!(
        ctx.os(),
        egui::os::OperatingSystem::Mac | egui::os::OperatingSystem::IOS
    );
    shortcut.format(&ModifierNames::NAMES, is_mac)
}

/// One cap's content.
enum Cap {
    Text(Arc<Galley>),
    Arrow(Icon),
}

impl Cap {
    fn width(&self) -> f32 {
        match self {
            Self::Text(g) => whole(g.size().x + 2.0 * KEY_PAD_X).max(KEY_H),
            Self::Arrow(_) => KEY_H,
        }
    }
}

/// A keyboard shortcut hint drawn as one keycap per key.
#[derive(Clone, Debug)]
#[must_use = "call `show` to draw the keys"]
pub struct Kbd<'a> {
    keys: Vec<&'a str>,
    text: &'a str,
}

impl<'a> Kbd<'a> {
    /// Keys written as `Ctrl+K`; arrow names (`Up`, `ArrowDown`) paint arrows.
    pub fn new(shortcut: &'a str) -> Self {
        Self {
            keys: split_keys(shortcut),
            text: shortcut,
        }
    }

    /// Number of caps drawn.
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    fn caps(&self, ui: &Ui) -> Vec<Cap> {
        let font = medium_font_sized(ui, KEY_PT);
        self.keys
            .iter()
            .map(|key| match arrow_icon(key) {
                Some(icon) => Cap::Arrow(icon),
                None => Cap::Text(text_galley(ui, key, font.clone())),
            })
            .collect()
    }

    /// Size the keys take.
    pub fn size(&self, ui: &Ui) -> Vec2 {
        let caps = self.caps(ui);
        vec2(caps_width(&caps), KEY_H)
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let size = self.size(ui);
        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
        response.widget_info(|| WidgetInfo::labeled(Role::Label, ui.is_enabled(), self.text));
        if ui.is_rect_visible(rect) {
            self.paint(ui, rect);
        }
        response
    }

    /// Paints the caps from the left edge of `rect`, centred on its height, without allocating.
    pub fn paint(&self, ui: &Ui, rect: Rect) {
        let (t, _) = kit(ui);
        let radius = t.radius_sm.min(4);
        let edge = mix(t.field_border, t.text, KEY_EDGE);
        let top = (rect.center().y - KEY_H / 2.0).round();
        let mut x = rect.left();
        for cap in self.caps(ui) {
            let w = cap.width();
            let key = Rect::from_min_size(pos2(x, top), vec2(w, KEY_H));
            let face = Rect::from_min_max(key.min, pos2(key.right(), key.bottom() - KEY_DEPTH));
            ui.painter().rect_filled(key, radius, edge);
            ui.painter().rect(
                face,
                radius,
                t.field,
                Stroke::new(1.0, t.field_border),
                StrokeKind::Inside,
            );
            match cap {
                Cap::Text(galley) => {
                    let pos = (face.center() - galley.size() / 2.0).round();
                    ui.painter().galley(pos, galley, t.text_dim);
                }
                Cap::Arrow(icon) => icon.paint(ui, face, KEY_ARROW, t.text_dim),
            }
            x += w + KEY_GAP;
        }
    }
}

fn caps_width(caps: &[Cap]) -> f32 {
    let gaps = KEY_GAP * caps.len().saturating_sub(1) as f32;
    caps.iter().map(Cap::width).sum::<f32>() + gaps
}

impl Widget for Kbd<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        self.show(ui)
    }
}

/// Keycaps for a shortcut written as `Ctrl+K`.
pub fn kbd(ui: &mut Ui, shortcut: &str) -> Response {
    Kbd::new(shortcut).show(ui)
}

/// Keycaps for `shortcut` with the platform's modifier names.
pub fn kbd_shortcut(ui: &mut Ui, shortcut: &KeyboardShortcut) -> Response {
    let text = shortcut_text(ui.ctx(), shortcut);
    Kbd::new(&text).show(ui)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_split_on_plus_and_keep_a_plus_key() {
        assert_eq!(split_keys("Ctrl+K"), ["Ctrl", "K"]);
        assert_eq!(split_keys("Ctrl + Shift + P"), ["Ctrl", "Shift", "P"]);
        assert_eq!(split_keys("Ctrl++"), ["Ctrl", "+"]);
        assert_eq!(split_keys("+"), ["+"]);
        assert_eq!(split_keys("Esc"), ["Esc"]);
        assert!(split_keys("").is_empty());
    }

    #[test]
    fn arrow_names_paint_arrows() {
        assert!(arrow_icon("Up").is_some());
        assert!(arrow_icon("ArrowDown").is_some());
        assert!(arrow_icon("K").is_none());
    }
}
