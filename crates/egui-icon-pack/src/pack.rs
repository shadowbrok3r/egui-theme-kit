//! Icon collections: the [`IconPack`] trait, the constant-friendly [`Pack`] and the merging [`IconRegistry`].

use std::iter::Copied;
use std::slice::Iter;

use crate::Icon;

/// A collection of icons sorted by name.
#[allow(clippy::wrong_self_convention)]
pub trait IconPack {
    /// All icons, sorted by name without duplicates.
    fn icons(&self) -> &[Icon];

    /// Looks up an icon by name.
    fn find(&self, name: &str) -> Option<Icon> {
        let icons = self.icons();
        icons
            .binary_search_by(|icon| icon.name().cmp(name))
            .ok()
            .map(|i| icons[i])
    }

    /// The Phosphor icon whose font glyph is `glyph`; the last of several sharing a glyph wins.
    fn from_glyph(&self, glyph: &str) -> Option<Icon> {
        if glyph.is_empty() {
            return None;
        }
        self.icons()
            .iter()
            .rev()
            .find(|icon| !icon.is_custom() && icon.glyph() == glyph)
            .copied()
    }
}

impl IconPack for [Icon] {
    fn icons(&self) -> &[Icon] {
        self
    }
}

impl<const N: usize> IconPack for [Icon; N] {
    fn icons(&self) -> &[Icon] {
        self
    }
}

impl IconPack for Vec<Icon> {
    fn icons(&self) -> &[Icon] {
        self
    }
}

impl<T: IconPack + ?Sized> IconPack for &T {
    fn icons(&self) -> &[Icon] {
        (**self).icons()
    }
}

/// A static slice of icons; generated tables expose their `ALL` slice as one.
#[derive(Clone, Copy, Debug)]
pub struct Pack(&'static [Icon]);

impl Pack {
    /// Wraps icons sorted by name.
    ///
    /// # Panics
    ///
    /// Fails to compile in a `const` and panics at run time when the icons are not strictly sorted by name.
    ///
    /// ```compile_fail,E0080
    /// use egui_icon_pack::{Icon, Pack};
    ///
    /// const SVG: &[u8] = b"<svg viewBox=\"0 0 256 256\"/>";
    /// const ICONS: &[Icon] = &[
    ///     Icon::new("b", "bytes://t/b.svg", "bytes://t/duo/b.svg", SVG, None, "", true),
    ///     Icon::new("a", "bytes://t/a.svg", "bytes://t/duo/a.svg", SVG, None, "", true),
    /// ];
    /// const PACK: Pack = Pack::new(ICONS);
    /// ```
    pub const fn new(icons: &'static [Icon]) -> Self {
        assert!(
            is_sorted(icons),
            "icons must be sorted by name without duplicates"
        );
        Self(icons)
    }

    /// The wrapped slice.
    pub const fn as_slice(self) -> &'static [Icon] {
        self.0
    }
}

impl IconPack for Pack {
    fn icons(&self) -> &[Icon] {
        self.0
    }
}

impl Pack {
    /// Iterates the icons in name order.
    pub fn iter(self) -> Copied<Iter<'static, Icon>> {
        self.0.iter().copied()
    }

    /// Number of icons.
    pub const fn len(self) -> usize {
        self.0.len()
    }

    /// True when the pack holds no icons.
    pub const fn is_empty(self) -> bool {
        self.0.is_empty()
    }
}

impl IntoIterator for Pack {
    type Item = Icon;
    type IntoIter = Copied<Iter<'static, Icon>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Several packs merged into one searchable collection; earlier packs win name clashes.
#[derive(Clone, Debug, Default)]
pub struct IconRegistry {
    icons: Vec<Icon>,
}

impl IconRegistry {
    /// Empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registry holding `pack`.
    pub fn with(mut self, pack: &dyn IconPack) -> Self {
        self.extend(pack);
        self
    }

    /// Iterates the icons in name order.
    pub fn iter(&self) -> Copied<Iter<'_, Icon>> {
        self.icons.iter().copied()
    }

    /// Number of icons.
    pub fn len(&self) -> usize {
        self.icons.len()
    }

    /// True when no icons are registered.
    pub fn is_empty(&self) -> bool {
        self.icons.is_empty()
    }

    /// Adds the icons of `pack` whose names are not registered yet.
    pub fn extend(&mut self, pack: &dyn IconPack) {
        for icon in pack.icons() {
            if let Some(known) = self.find(icon.name()) {
                debug_assert!(
                    known.uri() != icon.uri() || known.svg() == icon.svg(),
                    "icon `{}` shares the image key `{}` with different bytes; give the packs distinct uri prefixes",
                    icon.name(),
                    icon.uri()
                );
                continue;
            }
            let at = self.icons.partition_point(|have| have.name() < icon.name());
            self.icons.insert(at, *icon);
        }
    }
}

impl IconPack for IconRegistry {
    fn icons(&self) -> &[Icon] {
        &self.icons
    }
}

/// True when names strictly increase.
pub(crate) const fn is_sorted(icons: &[Icon]) -> bool {
    let mut i = 1;
    while i < icons.len() {
        if !str_lt(icons[i - 1].name(), icons[i].name()) {
            return false;
        }
        i += 1;
    }
    true
}

const fn str_lt(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let shared = if a.len() < b.len() { a.len() } else { b.len() };
    let mut i = 0;
    while i < shared {
        if a[i] != b[i] {
            return a[i] < b[i];
        }
        i += 1;
    }
    a.len() < b.len()
}
