//! Bundled Phosphor Regular icons with duotone back layers.
//!
//! One constant per icon (`CPU`, `HARD_DRIVE`, ...), the sorted [`ALL`] slice, [`BORROWED`] and [`PACK`].
//! Icons are derived from Phosphor Icons (MIT); see `assets/phosphor/LICENSE`.
//!
//! ```ignore
//! use egui_icon_pack::phosphor_regular as svg;
//! svg::CPU.show(ui, 16.0);
//! ```

include!(concat!(env!("OUT_DIR"), "/phosphor_regular.rs"));
