//! SVG icon packs for egui.
//!
//! - [`Icon`]: a white SVG, optional duotone back layer and a Phosphor font glyph, painted tinted and pixel-snapped.
//! - [`IconPack`], [`Pack`], [`IconRegistry`]: lookup by name and glyph over generated tables.
//! - [`widgets`]: buttons, labels, toggles and menu entries with leading icons that follow the active theme.
//! - [`gallery`]: searchable debug window for any pack, or for the one registered with [`gallery::set_pack`].
//! - [`install`] and [`install_fonts`]: image loaders and the Phosphor font (feature `font`).
//! - [`phosphor_regular`]: bundled Phosphor Regular set with duotone layers (feature `phosphor-regular`).
//!
//! Custom packs are generated from a folder of SVG files by `egui-icon-pack-build`. An app that
//! carried its own copy of this code lists what changes in `MIGRATION.md` next to this crate's `Cargo.toml`.
//!
//! # Setup
//!
//! ```ignore
//! let mut fonts = egui::FontDefinitions::default();
//! egui_icon_pack::install_fonts(&mut fonts);
//! ctx.set_fonts(fonts);
//! egui_icon_pack::install(&ctx);
//! ```
//!
//! # Hand-written pack
//!
//! ```
//! use egui_icon_pack::{Icon, IconPack, Pack};
//!
//! const SQUARE: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" fill="#ffffff"><path d="M32,32H224V224H32Z"/></svg>"##;
//!
//! const ICONS: &[Icon] = &[Icon::new("square", "bytes://demo/square.svg", "bytes://demo/duo/square.svg", SQUARE, None, "", true)];
//! const PACK: Pack = Pack::new(ICONS);
//!
//! assert_eq!(PACK.find("square"), Some(ICONS[0]));
//! ```

mod icon;
mod pack;

pub mod gallery;
pub mod widgets;

#[cfg(feature = "font")]
mod fonts;
#[cfg(feature = "phosphor-regular")]
pub mod phosphor_regular;

#[cfg(test)]
mod tests;

#[cfg(feature = "font")]
pub use egui_phosphor as phosphor;
#[cfg(feature = "font")]
pub use fonts::{FONT_KEY, icon, icon_colored, icon_sized, install_fonts};
pub use icon::{Icon, snapped_square};
pub use pack::{IconPack, IconRegistry, Pack};
pub use widgets::IconSize;

/// Installs egui_extras' image loaders (SVG included); idempotent.
pub fn install(ctx: &egui::Context) {
    egui_extras::install_image_loaders(ctx);
}
