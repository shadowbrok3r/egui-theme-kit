//! Chat-styled markdown, code blocks, shell lines and payloads for use outside the transcript.

use egui::{AsIdSalt, Id, IdSalt, Ui};

use super::entry::Payload;
use super::markdown;
use super::payload::{self, Part};
use super::row::RowCx;
use crate::chat::mention::MentionSource;
use crate::chat::types::ChatAction;

/// Draws chat content outside a transcript; each call returns what its links, mentions and copy buttons asked for.
pub struct ChatText<'a> {
    id_salt: IdSalt,
    mentions: Option<&'a dyn MentionSource>,
    touch: Option<bool>,
}

impl<'a> ChatText<'a> {
    pub fn new(id_salt: impl AsIdSalt) -> Self {
        Self {
            id_salt: IdSalt::new(id_salt),
            mentions: None,
            touch: None,
        }
    }

    /// Resolves mention chips to current labels.
    pub fn mentions(mut self, source: &'a dyn MentionSource) -> Self {
        self.mentions = Some(source);
        self
    }

    /// Forces click-only tree tokens and unselectable text on or off; defaults to whether the screen is touched.
    pub fn touch(mut self, touch: bool) -> Self {
        self.touch = Some(touch);
        self
    }

    fn run(self, ui: &mut Ui, draw: impl FnOnce(&mut Ui, &mut RowCx<'_>, Id)) -> Vec<ChatAction> {
        let id = ui.make_persistent_id(self.id_salt);
        let mut rc = RowCx::new(ui, id);
        rc.mentions = self.mentions;
        if let Some(touch) = self.touch {
            rc.touch = touch;
            rc.selectable = ui.style().interaction.selectable_labels && !touch;
        }
        draw(ui, &mut rc, id);
        rc.actions
    }

    /// Markdown in the agent message style.
    pub fn markdown(self, ui: &mut Ui, text: &str) -> Vec<ChatAction> {
        self.run(ui, |ui, rc, id| {
            let ink = rc.p.text;
            markdown::show(ui, rc, text, ink, id, false);
        })
    }

    /// A highlighted code block with a copy button.
    pub fn code(self, ui: &mut Ui, lang: &str, code: &str) -> Vec<ChatAction> {
        self.run(ui, |ui, rc, id| {
            markdown::code_block(ui, rc, lang, code, id)
        })
    }

    /// A `$ command` line with shell colouring.
    pub fn shell_line(self, ui: &mut Ui, line: &str) -> Vec<ChatAction> {
        self.run(ui, |ui, rc, _| markdown::shell_line(ui, rc, line))
    }

    /// A payload as a JSON tree opened two levels, or capped monospace text.
    pub fn payload(self, ui: &mut Ui, payload: &Payload<'_>) -> Vec<ChatAction> {
        self.run(ui, |ui, rc, id| {
            payload::show(ui, rc, payload, id, Part::Detail, None)
        })
    }

    /// Capped monospace text with a copy button.
    pub fn mono(self, ui: &mut Ui, text: &str) -> Vec<ChatAction> {
        self.run(ui, |ui, rc, id| {
            let ink = rc.p.text;
            payload::mono_text(ui, rc, text, ink, id);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    #[test]
    fn every_view_draws_outside_a_transcript() {
        let ctx = egui::Context::default();
        let mut texts = Vec::new();
        for _ in 0..2 {
            let mut out = ctx.run_ui(Default::default(), |ui| {
                let mut actions = ChatText::new("md").markdown(ui, "**bold** and `code`");
                actions.extend(ChatText::new("code").code(ui, "rust", "fn main() {}"));
                actions.extend(ChatText::new("sh").shell_line(ui, "$ cargo test --all"));
                let json = Payload::Text(Cow::Borrowed("{\"a\": [1, 2]}"));
                actions.extend(ChatText::new("json").payload(ui, &json));
                actions.extend(ChatText::new("mono").mono(ui, "plain output"));
                assert!(actions.is_empty(), "{actions:?}");
            });
            out.textures_delta.clear();
            texts = out
                .shapes
                .iter()
                .filter_map(|c| match &c.shape {
                    egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
                    _ => None,
                })
                .collect::<Vec<_>>();
        }
        for needle in ["bold", "cargo test --all", "plain output"] {
            assert!(
                texts.iter().any(|t| t.contains(needle)),
                "{needle} missing: {texts:?}"
            );
        }
    }

    #[test]
    fn touch_forces_the_row_context_either_way() {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(Default::default(), |ui| {
            let selectable = ui.style().interaction.selectable_labels;
            let mut seen = Vec::new();
            for touch in [None, Some(true), Some(false)] {
                let mut text = ChatText::new(("touch", touch));
                if let Some(touch) = touch {
                    text = text.touch(touch);
                }
                let _ = text.run(ui, |_, rc, _| seen.push((rc.touch, rc.selectable)));
            }
            assert_eq!(
                seen,
                [(false, selectable), (true, false), (false, selectable)]
            );
        });
        out.textures_delta.clear();
    }

    /// Senses of every widget `draw` adds in its second frame on `ctx`.
    fn senses(ctx: &egui::Context, mut draw: impl FnMut(&mut Ui)) -> Vec<egui::Sense> {
        let mut senses = Vec::new();
        for _ in 0..2 {
            let mut out = ctx.run_ui(Default::default(), &mut draw);
            out.textures_delta.clear();
            senses = ctx.viewport(|v| {
                v.this_pass
                    .widgets
                    .layers()
                    .flat_map(|(_, w)| w.iter().map(|w| w.sense))
                    .collect()
            });
        }
        senses
    }

    #[test]
    fn touch_text_is_not_selectable() {
        let ctx = egui::Context::default();
        let drags = |touch: bool| {
            senses(&ctx, |ui| {
                let text = ChatText::new(("md", touch)).touch(touch);
                let _ = text.markdown(ui, "plain words and more words");
            })
            .iter()
            .any(|s| s.senses_drag())
        };
        assert!(drags(false));
        assert!(!drags(true));
    }

    #[cfg(feature = "chat-json")]
    #[test]
    fn a_touch_payload_draws_click_only_tree_tokens() {
        let ctx = egui::Context::default();
        ctx.global_style_mut(|s| s.interaction.selectable_labels = false);
        let json = Payload::Text(Cow::Borrowed("{\"key\": [1, 2]}"));
        let drags = |touch: bool| {
            senses(&ctx, |ui| {
                let _ = ChatText::new(("payload", touch))
                    .touch(touch)
                    .payload(ui, &json);
            })
            .iter()
            .filter(|s| s.senses_drag())
            .count()
        };
        // Spacing delimiters sense drags in both modes.
        assert!(
            drags(true) < drags(false),
            "{} {}",
            drags(true),
            drags(false)
        );
    }
}
