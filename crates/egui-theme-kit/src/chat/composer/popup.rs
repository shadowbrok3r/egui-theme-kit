//! Suggestion rows for `@` mentions and `/` commands, drawn inline above the composer.

use egui::{
    Align2, FontId, Galley, Rect, Sense, TextStyle, Ui, UiBuilder, pos2,
    text::{LayoutJob, TextFormat},
    vec2,
};
use std::sync::Arc;

use super::slash::SlashSource;
use super::trigger::Trigger;
use crate::chat::icons::ChatIcons;
use crate::chat::mention::{MentionRef, MentionSource};
use crate::chat::palette::ChatPalette;
use crate::chat::surface::{SurfaceKind, paint_surface};
use crate::chat::types::SessionStatus;
use crate::controls::{Icon, Metrics, fuzzy_match};

/// Most suggestions listed.
pub(super) const MAX_ROWS: usize = 8;
/// Share of the row width the label takes before the detail.
const LABEL_SHARE: f32 = 0.6;
/// Radius of the status dot as a share of the icon size.
const DOT_SHARE: f32 = 0.18;

/// A slash command copied out of the provider for later frames.
#[derive(Clone, Debug)]
pub(super) struct SlashPick {
    pub id: String,
    /// Name without its slash.
    pub name: String,
    pub args: String,
    pub about: String,
    pub icon: Option<Icon>,
    pub enabled: bool,
    pub run_on_pick: bool,
}

impl SlashPick {
    /// Whether picking it runs it rather than inserting `/name `.
    pub fn runs_on_pick(&self) -> bool {
        self.args.trim().is_empty() || self.run_on_pick
    }
}

/// One row of the popup.
#[derive(Clone, Debug)]
pub(super) enum Suggestion {
    Mention {
        target: MentionRef,
        detail: String,
        status: Option<SessionStatus>,
        positions: Vec<usize>,
    },
    Slash {
        pick: SlashPick,
        positions: Vec<usize>,
    },
}

impl Suggestion {
    pub fn enabled(&self) -> bool {
        match self {
            Self::Mention { .. } => true,
            Self::Slash { pick, .. } => pick.enabled,
        }
    }
}

/// The open popup: its trigger, rows and selected row.
#[derive(Clone, Debug)]
pub(super) struct Popup {
    pub trigger: Trigger,
    pub items: Vec<Suggestion>,
    pub selected: usize,
}

impl Popup {
    /// Moves the selection by `steps`, wrapping.
    pub fn step(&mut self, steps: isize) {
        let n = self.items.len() as isize;
        if n > 0 {
            self.selected = (self.selected as isize + steps).rem_euclid(n) as usize;
        }
    }

    pub fn current(&self) -> Option<&Suggestion> {
        self.items.get(self.selected).filter(|s| s.enabled())
    }
}

/// Stable order of `items` by fuzzy score of `query` in their key; non-matches keep provider order last.
fn rank<T>(query: &str, items: Vec<T>, key: impl Fn(&T) -> &str) -> Vec<(T, Vec<usize>)> {
    let mut scored: Vec<(Option<i32>, T, Vec<usize>)> = items
        .into_iter()
        .map(|item| match fuzzy_match(query, key(&item)) {
            Some(m) => (Some(m.score), item, m.positions),
            None => (None, item, Vec::new()),
        })
        .collect();
    scored.sort_by_key(|s| std::cmp::Reverse(s.0));
    scored
        .into_iter()
        .take(MAX_ROWS)
        .map(|(_, item, positions)| (item, positions))
        .collect()
}

/// Ranked mention rows for `query`.
pub(super) fn mention_items(source: &dyn MentionSource, query: &str) -> Vec<Suggestion> {
    let candidates = source.suggest(query, MAX_ROWS);
    rank(query, candidates, |c| c.target.label.as_str())
        .into_iter()
        .map(|(c, positions)| Suggestion::Mention {
            target: c.target,
            detail: c.detail,
            status: c.status,
            positions,
        })
        .collect()
}

/// Ranked command rows for `query`.
pub(super) fn slash_items(source: &dyn SlashSource, query: &str) -> Vec<Suggestion> {
    let picks: Vec<SlashPick> = source
        .commands(query)
        .into_iter()
        .map(|c| SlashPick::from(&c))
        .collect();
    rank(query, picks, |p| p.name.as_str())
        .into_iter()
        .map(|(pick, positions)| Suggestion::Slash { pick, positions })
        .collect()
}

/// `text` with chars at `positions` in `hit`, cut to one row of `max_w`.
fn highlighted(
    ui: &Ui,
    text: &str,
    positions: &[usize],
    font: FontId,
    (base, hit): (egui::Color32, egui::Color32),
    max_w: f32,
) -> Arc<Galley> {
    let mut job = LayoutJob::default();
    for (i, c) in text.chars().enumerate() {
        let color = if positions.contains(&i) { hit } else { base };
        let mut buf = [0u8; 4];
        job.append(
            c.encode_utf8(&mut buf),
            0.0,
            TextFormat::simple(font.clone(), color),
        );
    }
    job.wrap.max_width = max_w.max(1.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('\u{2026}');
    ui.painter().layout_job(job)
}

/// Draws the rows at `row_h` each; returns the index of a clicked enabled row.
pub(super) fn show(ui: &mut Ui, popup: &mut Popup, row_h: f32) -> Option<usize> {
    let p = ChatPalette::of(ui);
    let m = Metrics::get(ui.ctx());
    let icons = ChatIcons::get(ui.ctx());
    let width = ui.available_width();
    let height = row_h * popup.items.len() as f32;
    let (outer, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    paint_surface(ui, outer, SurfaceKind::Popup);
    let font = TextStyle::Body.resolve(ui.style());
    let small = TextStyle::Small.resolve(ui.style());
    let pad = m.pad.x;
    let mut clicked = None;
    for (i, item) in popup.items.iter().enumerate() {
        let rect = Rect::from_min_size(
            pos2(outer.left(), outer.top() + row_h * i as f32),
            vec2(width, row_h),
        );
        let id = ui.scope_id().with(("egui_theme_kit.chat.suggestion", i));
        let sense = if item.enabled() {
            Sense::click()
        } else {
            Sense::hover()
        };
        let response = ui.interact(rect, id, sense);
        if response.hovered() && item.enabled() {
            popup.selected = i;
        }
        if response.clicked() {
            clicked = Some(i);
        }
        if popup.selected == i && item.enabled() {
            ui.painter()
                .rect_filled(rect.shrink(2.0), p.radius_chip, p.mention_fill);
        }
        let icon_rect = Rect::from_center_size(
            pos2(rect.left() + pad + m.icon_md / 2.0, rect.center().y),
            vec2(m.icon_md, m.icon_md),
        );
        let text_x = icon_rect.right() + pad;
        let avail = (rect.right() - pad - text_x).max(1.0);
        let selected = popup.selected == i;
        let (base, hit) = match (item.enabled(), selected) {
            (false, _) => (p.text_faint, p.text_faint),
            (true, true) => (p.mention_text, p.mention_text),
            (true, false) => (p.text, p.link),
        };
        let (label, positions, detail, icon, dot) = match item {
            Suggestion::Mention {
                target,
                detail,
                status,
                positions,
            } => (
                target.label.clone(),
                positions.clone(),
                detail.clone(),
                icons.mention,
                status.map(|s| s.tone().color(&p)),
            ),
            Suggestion::Slash { pick, positions } => {
                let label = format!("/{}", pick.name);
                let positions = positions.iter().map(|i| i + 1).collect();
                let detail = match (pick.args.is_empty(), pick.about.is_empty()) {
                    (true, _) => pick.about.clone(),
                    (false, true) => pick.args.clone(),
                    (false, false) => format!("{}  {}", pick.args, pick.about),
                };
                (
                    label,
                    positions,
                    detail,
                    pick.icon.unwrap_or(icons.slash),
                    None,
                )
            }
        };
        icon.paint(ui, icon_rect, m.icon_md, base);
        if let Some(color) = dot {
            let r = m.icon_md * DOT_SHARE;
            ui.painter()
                .circle_filled(icon_rect.right_bottom() - vec2(r, r), r, color);
        }
        let label_w = if detail.is_empty() {
            avail
        } else {
            avail * LABEL_SHARE
        };
        let galley = highlighted(ui, &label, &positions, font.clone(), (base, hit), label_w);
        let label_pos = pos2(text_x, rect.center().y - galley.size().y / 2.0);
        let used = galley.size().x;
        ui.painter().galley(label_pos, galley, base);
        if !detail.is_empty() {
            let detail_w = (avail - used - pad).max(1.0);
            let galley = highlighted(
                ui,
                &detail,
                &[],
                small.clone(),
                (p.text_dim, p.text_dim),
                detail_w,
            );
            let at = Align2::LEFT_CENTER
                .align_size_within_rect(galley.size(), rect)
                .left_top();
            ui.painter()
                .galley(pos2(text_x + used + pad, at.y), galley, p.text_dim);
        }
    }
    clicked
}

/// Shows the rows in a scope with a fixed id so they never shift later ids.
pub(super) fn show_scoped(
    ui: &mut Ui,
    scope: egui::Id,
    popup: &mut Popup,
    row_h: f32,
) -> Option<usize> {
    ui.scope_builder(UiBuilder::new().scope_id(scope), |ui| {
        show(ui, popup, row_h)
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::composer::SlashCommand;
    use crate::chat::mention::MentionCandidate;

    fn candidate(id: &str, label: &str) -> MentionCandidate {
        MentionCandidate {
            target: MentionRef::new("session", id, label),
            detail: String::new(),
            status: None,
            section: None,
        }
    }

    #[test]
    fn mention_rows_rank_by_fuzzy_score_and_cap() {
        let source = |_: &str, _: usize| {
            let mut v = vec![candidate("1", "Other thing"), candidate("2", "Fix scroll")];
            v.extend((0..20).map(|i| candidate(&i.to_string(), "zzz")));
            v
        };
        let rows = mention_items(&source, "fix");
        assert_eq!(rows.len(), MAX_ROWS);
        let Suggestion::Mention {
            target, positions, ..
        } = &rows[0]
        else {
            panic!("mention row expected");
        };
        assert_eq!(target.id, "2");
        assert_eq!(positions, &vec![0, 1, 2]);
    }

    #[test]
    fn slash_rows_copy_the_commands() {
        let source = |_: &str| {
            vec![SlashCommand {
                id: "compact".into(),
                name: "/compact".into(),
                args: "".into(),
                about: "Shrink the context".into(),
                section: None,
                icon: None,
                enabled: true,
                run_on_pick: false,
            }]
        };
        let rows = slash_items(&source, "co");
        let Suggestion::Slash { pick, .. } = &rows[0] else {
            panic!("slash row expected");
        };
        assert_eq!(pick.name, "compact");
        assert!(pick.runs_on_pick());
    }

    #[test]
    fn selection_wraps_and_skips_disabled_accept() {
        let pick = SlashPick {
            id: "x".into(),
            name: "x".into(),
            args: String::new(),
            about: String::new(),
            icon: None,
            enabled: false,
            run_on_pick: false,
        };
        let mut popup = Popup {
            trigger: Trigger::Slash {
                query: String::new(),
            },
            items: vec![
                Suggestion::Slash {
                    pick: pick.clone(),
                    positions: Vec::new(),
                },
                Suggestion::Slash {
                    pick: SlashPick {
                        enabled: true,
                        ..pick
                    },
                    positions: Vec::new(),
                },
            ],
            selected: 0,
        };
        assert!(popup.current().is_none());
        popup.step(-1);
        assert_eq!(popup.selected, 1);
        assert!(popup.current().is_some());
        popup.step(1);
        assert_eq!(popup.selected, 0);
    }
}
