//! Phosphor font registration and glyph text helpers.

use egui::{Color32, FontDefinitions, FontFamily, RichText};

/// Key of the Phosphor font in `FontDefinitions::font_data` and in the font families.
pub const FONT_KEY: &str = "phosphor";

/// Registers Phosphor in the Proportional, Monospace and named "Regular"/"Bold" families; idempotent.
pub fn install_fonts(fonts: &mut FontDefinitions) {
    if !fonts.font_data.contains_key(FONT_KEY) {
        egui_phosphor::add_to_fonts(fonts, egui_phosphor::Variant::Regular);
    }
    for family in [
        FontFamily::Proportional,
        FontFamily::Monospace,
        FontFamily::Name("Regular".into()),
        FontFamily::Name("Bold".into()),
    ] {
        if let Some(keys) = fonts.families.get_mut(&family)
            && !keys.iter().any(|k| k == FONT_KEY)
        {
            keys.insert(1.min(keys.len()), FONT_KEY.into());
        }
    }
}

/// Glyph as rich text in the Proportional family.
pub fn icon(glyph: &str) -> RichText {
    RichText::new(glyph).family(FontFamily::Proportional)
}

/// Glyph at `size` points.
pub fn icon_sized(glyph: &str, size: f32) -> RichText {
    RichText::new(glyph)
        .size(size)
        .family(FontFamily::Proportional)
}

/// Glyph in a fixed color.
pub fn icon_colored(glyph: &str, color: Color32) -> RichText {
    icon(glyph).color(color)
}
