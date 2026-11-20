//! Drawing context shared by transcript rows, cards and boards for one frame.

use std::collections::HashMap;

use egui::{FontFamily, FontId, Id, Response, TextStyle, Ui};

use super::TranscriptOptions;
use crate::chat::icons::ChatIcons;
use crate::chat::mention::{MentionRef, MentionSource};
use crate::chat::palette::ChatPalette;
use crate::chat::types::{ChatAction, ClockFormat, TimeFormat, UnixMs};
use crate::controls::{Metrics, Tokens};

/// Clock labels used when the app sets no formatter.
static DEFAULT_CLOCK: ClockFormat = ClockFormat {
    utc_offset_min: 0,
    hour24: true,
};

/// Fonts rows draw with, resolved from the `Ui` style.
#[derive(Clone, Debug)]
pub(crate) struct Fonts {
    pub body: FontId,
    pub small: FontId,
    pub mono: FontId,
    /// Body size at the medium weight.
    pub strong: FontId,
}

impl Fonts {
    pub fn of(ui: &Ui) -> Self {
        let style = ui.style();
        Self {
            body: TextStyle::Body.resolve(style),
            small: TextStyle::Small.resolve(style),
            mono: TextStyle::Monospace.resolve(style),
            strong: crate::controls::medium_font(ui, TextStyle::Body),
        }
    }

    /// `font` at the semibold weight when it is proportional.
    pub fn bold(ui: &Ui, font: &FontId) -> FontId {
        if font.family == FontFamily::Proportional {
            crate::controls::semibold_font_sized(ui, font.size)
        } else {
            font.clone()
        }
    }
}

/// A small spinner tinted `color` centred in `rect`, without moving the cursor.
pub(crate) fn spinner_at(ui: &mut Ui, rect: egui::Rect, color: egui::Color32) {
    ui.place(rect, |ui: &mut Ui| {
        crate::controls::tinted_spinner(ui, crate::controls::SpinnerSize::Sm, color)
    });
}

/// Labels of mention targets, refreshed when the stamp changes.
#[derive(Clone, Default)]
struct ResolveCache {
    stamp: u64,
    labels: HashMap<String, Option<String>>,
}

/// How a mention chip is drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MentionView {
    pub label: String,
    /// The target resolved, or no source was given to check it.
    pub known: bool,
    pub clickable: bool,
}

/// Colours, fonts, options and collected actions for one frame of chat drawing.
pub(crate) struct RowCx<'a> {
    /// Base id that row and item ids derive from.
    pub id: Id,
    pub p: ChatPalette,
    pub t: Tokens,
    pub m: Metrics,
    pub icons: ChatIcons,
    pub f: Fonts,
    pub touch: bool,
    /// Labels may be selected with the pointer.
    pub selectable: bool,
    /// Clicks of this press stopped a fling and are ignored.
    pub suppress: bool,
    pub options: TranscriptOptions,
    pub time_format: &'a dyn TimeFormat,
    pub mentions: Option<&'a dyn MentionSource>,
    /// Stamp of the mention labels; `None` resolves again every pass.
    pub mention_stamp: Option<u64>,
    pub actions: Vec<ChatAction>,
}

impl<'a> RowCx<'a> {
    pub fn new(ui: &Ui, id: Id) -> Self {
        let touch = ui.input(|i| i.has_touch_screen());
        Self {
            id,
            p: ChatPalette::of(ui),
            t: Tokens::get(ui.ctx()),
            m: Metrics::get(ui.ctx()),
            icons: ChatIcons::get(ui.ctx()),
            f: Fonts::of(ui),
            touch,
            selectable: ui.style().interaction.selectable_labels && !touch,
            suppress: false,
            options: TranscriptOptions::default(),
            time_format: &DEFAULT_CLOCK,
            mentions: None,
            mention_stamp: None,
            actions: Vec::new(),
        }
    }

    /// Whether `response` was clicked by a tap that counts.
    pub fn tapped(&self, response: &Response) -> bool {
        response.clicked() && !self.suppress
    }

    /// Places `text` on the clipboard and reports it.
    pub fn copy(&mut self, ui: &Ui, text: String) {
        ui.ctx().copy_text(text.clone());
        self.actions.push(ChatAction::Copied(text));
    }

    /// The short label of `t`.
    pub fn time(&self, t: UnixMs) -> String {
        self.time_format
            .label(t, self.options.now.or_else(UnixMs::now))
    }

    /// Height of one row of `font`.
    pub fn row_h(ui: &Ui, font: &FontId) -> f32 {
        ui.ctx().fonts_mut(|f| f.row_height(font))
    }

    /// How a mention of `m` is drawn; agent-authored chips click only when they resolve.
    pub fn mention(&self, ui: &Ui, m: &MentionRef, user: bool) -> MentionView {
        let Some(source) = self.mentions else {
            return MentionView {
                label: m.label.clone(),
                known: true,
                clickable: user,
            };
        };
        let resolved = self.resolve(ui, source, m);
        let known = resolved.is_some();
        MentionView {
            label: resolved.unwrap_or_else(|| m.label.clone()),
            known,
            clickable: known,
        }
    }

    fn resolve(&self, ui: &Ui, source: &dyn MentionSource, m: &MentionRef) -> Option<String> {
        let cache_id = self.id.with("egui_theme_kit.chat.mention_labels");
        let stamp = self
            .mention_stamp
            .unwrap_or_else(|| ui.ctx().cumulative_pass_nr());
        let target = m.target();
        let hit = ui.ctx().data_mut(|d| {
            let cache = d.get_temp_mut_or_default::<ResolveCache>(cache_id);
            if cache.stamp != stamp {
                cache.stamp = stamp;
                cache.labels.clear();
            }
            cache.labels.get(&target).cloned()
        });
        if let Some(hit) = hit {
            return hit;
        }
        let label = source.resolve(m);
        ui.ctx().data_mut(|d| {
            d.get_temp_mut_or_default::<ResolveCache>(cache_id)
                .labels
                .insert(target, label.clone());
        });
        label
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::mention::MentionCandidate;
    use std::cell::Cell;

    struct Counting<'c> {
        calls: &'c Cell<u32>,
    }

    impl MentionSource for Counting<'_> {
        fn suggest(&self, _query: &str, _limit: usize) -> Vec<MentionCandidate> {
            Vec::new()
        }

        fn resolve(&self, target: &MentionRef) -> Option<String> {
            self.calls.set(self.calls.get() + 1);
            (target.id == "1").then(|| "Renamed".to_owned())
        }
    }

    #[test]
    fn mentions_resolve_once_per_stamp_and_unknown_agent_chips_do_not_click() {
        let ctx = egui::Context::default();
        let calls = Cell::new(0);
        let source = Counting { calls: &calls };
        let known = MentionRef::new("session", "1", "Old name");
        let unknown = MentionRef::new("session", "2", "Spoof");
        for _ in 0..3 {
            let mut out = ctx.run_ui(Default::default(), |ui| {
                let mut rc = RowCx::new(ui, Id::unique("mention-test"));
                rc.mentions = Some(&source);
                rc.mention_stamp = Some(7);
                let view = rc.mention(ui, &known, false);
                assert_eq!(view.label, "Renamed");
                assert!(view.clickable);
                let spoof = rc.mention(ui, &unknown, false);
                assert!(!spoof.known && !spoof.clickable);
                assert_eq!(spoof.label, "Spoof");
            });
            out.textures_delta.clear();
        }
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn without_a_source_only_user_chips_click() {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(Default::default(), |ui| {
            let rc = RowCx::new(ui, Id::unique("mention-test-2"));
            let m = MentionRef::new("session", "1", "A");
            assert!(rc.mention(ui, &m, true).clickable);
            assert!(!rc.mention(ui, &m, false).clickable);
        });
        out.textures_delta.clear();
    }
}
