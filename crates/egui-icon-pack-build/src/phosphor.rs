//! Conversion of Phosphor Icons core assets into tintable white front and duo back layers.

use std::path::Path;
use std::{fs, io};

use crate::svg;

/// Front layer: `currentColor` replaced by white so egui can tint the icon.
pub fn white_front(svg: &str) -> String {
    svg.replace("currentColor", "#ffffff")
}

/// Duo layer: the `opacity="0.2"` shapes of a Phosphor duotone SVG, opacity removed, white ink.
///
/// Returns `None` when the file has no such shapes or nests elements.
pub fn duo_back_layer(duotone: &str) -> Option<String> {
    let root = svg::root_tag(duotone)?;
    let body_start = duotone.find(root)? + root.len();
    let body_end = duotone.rfind("</svg>")?;
    let body = duotone.get(body_start..body_end)?;

    let mut layer = String::new();
    let mut rest = body;
    while let Some(open) = rest.find('<') {
        let close = rest[open..].find('>')? + open;
        let element = &rest[open..=close];
        if element.starts_with("</") || !element.ends_with("/>") {
            return None;
        }
        if element.contains(" opacity=\"0.2\"") {
            layer.push_str(&element.replace(" opacity=\"0.2\"", ""));
        }
        rest = &rest[close + 1..];
    }
    if layer.is_empty() {
        return None;
    }
    Some(format!("{}{layer}</svg>", white_front(root)))
}

/// Files written by [`import`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Imported {
    /// True when a duo layer was written.
    pub duo: bool,
}

/// Imports icon `name` from an unpacked `@phosphor-icons/core` `assets` folder into `out_dir/{regular,duo}`.
///
/// Reads `regular/<name>.svg` and `duotone/<name>-duotone.svg`; the duo layer is skipped when absent.
pub fn import(core_assets: &Path, out_dir: &Path, name: &str) -> io::Result<Imported> {
    let regular = fs::read_to_string(core_assets.join("regular").join(format!("{name}.svg")))?;
    fs::create_dir_all(out_dir.join("regular"))?;
    fs::write(
        out_dir.join("regular").join(format!("{name}.svg")),
        white_front(regular.trim()),
    )?;

    let duotone = fs::read_to_string(
        core_assets
            .join("duotone")
            .join(format!("{name}-duotone.svg")),
    )
    .ok();
    let layer = duotone.as_deref().and_then(duo_back_layer);
    if let Some(layer) = &layer {
        fs::create_dir_all(out_dir.join("duo"))?;
        fs::write(out_dir.join("duo").join(format!("{name}.svg")), layer)?;
    }
    Ok(Imported {
        duo: layer.is_some(),
    })
}
