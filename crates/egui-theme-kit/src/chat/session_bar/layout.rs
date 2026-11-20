//! Width classes of the bars and the pure geometry behind them.

use egui::Rangef;

use super::ViewItem;
use super::parts::severity;
use crate::chat::types::{MessageNav, Tone};

/// Width, in control heights, from which the title card shows every row in full.
const FULL_W: f32 = 31.5;
/// Width, in control heights, from which the title card keeps its chips and buttons inline.
const ELIDED_W: f32 = 23.0;
/// Width, in control heights, from which the title card keeps a two-line header.
const COMPACT_W: f32 = 17.5;
/// Width, in control heights, under which the app bar folds its switches into a menu.
pub(super) const APP_BAR_FOLD_W: f32 = 24.0;

/// How the session title card lays out at a width.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BarLayout {
    /// Every row; the subtitle wraps.
    Full,
    /// The subtitle is cut to one line.
    Elided,
    /// Chips and buttons move into an overflow menu.
    Folded,
    /// One title line; the subtitle shows on tap and the jump drops its label.
    Compact,
}

impl BarLayout {
    /// The layout for `width` points at a control height of `control_h`.
    pub fn for_width(width: f32, control_h: f32) -> Self {
        let units = width / control_h.max(1.0);
        if units >= FULL_W {
            Self::Full
        } else if units >= ELIDED_W {
            Self::Elided
        } else if units >= COMPACT_W {
            Self::Folded
        } else {
            Self::Compact
        }
    }

    /// Whether chips and buttons move into the overflow menu.
    pub fn folds(self) -> bool {
        matches!(self, Self::Folded | Self::Compact)
    }

    /// Whether the header is a single title line.
    pub fn compact(self) -> bool {
        self == Self::Compact
    }
}

/// Span of a title between groups `left_w` and `right_w` wide: centred on `row` while that leaves `min_w`, else the room between them.
pub(super) fn title_span(row: Rangef, left_w: f32, right_w: f32, gap: f32, min_w: f32) -> Rangef {
    let side = left_w.max(right_w) + gap;
    let centred = Rangef::new(row.min + side, row.max - side);
    if centred.span() >= min_w {
        return centred;
    }
    let min = row.min + left_w + gap;
    let max = row.max - right_w - gap;
    Rangef::new(min, max.max(min))
}

/// Count of leading `widths` that fit `avail` with `gap` between them, keeping `reserve` for a more button when some are left out.
pub(super) fn fit_count(widths: &[f32], gap: f32, avail: f32, reserve: f32) -> usize {
    let total = widths.iter().sum::<f32>() + gap * widths.len().saturating_sub(1) as f32;
    if total <= avail {
        return widths.len();
    }
    let room = avail - reserve - gap;
    let mut used = 0.0;
    let mut count = 0;
    for w in widths {
        let next = used + if count > 0 { gap } else { 0.0 } + w;
        if next > room {
            break;
        }
        used = next;
        count += 1;
    }
    count
}

/// Counter of the jump group: `n/N`, or `–/N` while none of the user's messages is in view.
pub(super) fn nav_text(nav: MessageNav) -> String {
    match nav.current {
        Some(i) => format!("{}/{}", (i + 1).min(nav.total), nav.total),
        None => format!("\u{2013}/{}", nav.total),
    }
}

/// Whether the previous and next steps lead anywhere.
pub(super) fn nav_steps(nav: MessageNav) -> (bool, bool) {
    if nav.total == 0 {
        return (false, false);
    }
    match nav.current {
        Some(i) => (i > 0, i + 1 < nav.total),
        None => (true, true),
    }
}

/// Sum of the badges of `items` and the most urgent tone among the non-zero ones.
pub(super) fn badge_total<V>(items: &[ViewItem<'_, V>]) -> Option<(u32, Tone)> {
    items
        .iter()
        .filter_map(|i| i.badge.filter(|n| *n > 0).map(|n| (n, i.tone)))
        .fold(None, |acc, (n, tone)| match acc {
            None => Some((n, tone)),
            Some((sum, top)) => {
                let top = if severity(tone) > severity(top) {
                    tone
                } else {
                    top
                };
                Some((sum.saturating_add(n), top))
            }
        })
}
