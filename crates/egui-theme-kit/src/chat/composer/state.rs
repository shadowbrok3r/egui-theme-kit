//! The composer's text buffer with mention spans, and the message it exports.

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use super::edit::{self, byte_at, from_wire};
use super::popup::Popup;
use super::touch::TouchState;
use crate::chat::mention::{MentionRef, MentionSource, Resolution, encode};
use crate::controls::CommandPaletteState;

/// A mention chip in the buffer: the chars of its label and its target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MentionSpan {
    /// Char range of the label in the buffer.
    pub chars: Range<usize>,
    pub target: MentionRef,
}

/// A message ready to send.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OutgoingText {
    /// What the user saw.
    pub display: String,
    /// What the agent gets, mentions encoded.
    pub wire: String,
    pub mentions: Vec<MentionRef>,
}

/// `(kind, id)` of a mention target.
type TargetKey = (String, String);

fn key_of(target: &MentionRef) -> TargetKey {
    (target.kind.clone(), target.id.clone())
}

/// Labels from [`MentionSource::resolve`], kept while the revision holds.
#[derive(Clone, Debug, Default)]
pub(super) struct LabelCache {
    revision: u64,
    labels: HashMap<TargetKey, Resolution>,
}

impl LabelCache {
    /// The resolved label of `target`, asking `source` once per revision.
    pub fn resolve(
        &mut self,
        source: &dyn MentionSource,
        revision: u64,
        target: &MentionRef,
    ) -> Resolution {
        if revision != self.revision {
            self.labels.clear();
            self.revision = revision;
        }
        self.labels
            .entry(key_of(target))
            .or_insert_with(|| source.resolve(target))
            .clone()
    }
}

/// Text, mention spans and popup state the app keeps per composer.
#[derive(Clone, Debug, Default)]
pub struct ComposerState {
    text: String,
    spans: Vec<MentionSpan>,
    /// Deleted chips an undo may bring back.
    pub(super) graveyard: Vec<MentionSpan>,
    pub(super) popup: Option<Popup>,
    /// Char index of an `@` whose popup was dismissed.
    pub(super) dismissed: Option<usize>,
    pub(super) plus: CommandPaletteState,
    /// Focus returns to the text edit once the `+` menu closes.
    pub(super) refocus: bool,
    /// Focus is requested again on the next frame, when the closed `+` menu no longer blocks it.
    pub(super) refocus_again: bool,
    /// Cursor to store on the next frame after a change outside the text edit.
    pub(super) pending_cursor: Option<usize>,
    pub(super) labels: LabelCache,
    /// Targets picked from suggestions, trusted without resolving.
    picked: HashSet<TargetKey>,
    /// Width of the controls left of the text, from the last frame.
    pub(super) left_w: f32,
    /// Width of the controls right of the text, from the last frame.
    pub(super) right_w: f32,
    /// Height of the text area, from the last frame.
    pub(super) text_h: f32,
    pub(super) touch: TouchState,
}

impl ComposerState {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }

    /// Empties the buffer; an undo in the text edit can still bring the chips back.
    pub fn clear(&mut self) {
        self.text.clear();
        edit::bury(&mut self.graveyard, self.spans.drain(..));
        self.popup = None;
        self.dismissed = None;
        self.pending_cursor = Some(0);
    }

    /// Mention chips in buffer order.
    pub fn spans(&self) -> &[MentionSpan] {
        &self.spans
    }

    /// The buffer as display text, wire text and mention targets.
    pub fn message(&self) -> OutgoingText {
        let mut wire = String::with_capacity(self.text.len());
        let mut last = 0;
        for span in &self.spans {
            let (start, end) = (
                byte_at(&self.text, span.chars.start),
                byte_at(&self.text, span.chars.end),
            );
            wire.push_str(&self.text[last..start]);
            wire.push_str(&encode(&span.target));
            last = end;
        }
        wire.push_str(&self.text[last..]);
        OutgoingText {
            display: self.text.clone(),
            wire,
            mentions: self.spans.iter().map(|s| s.target.clone()).collect(),
        }
    }

    /// Replaces the buffer with wire text, turning encoded mentions back into chips.
    pub fn set_wire(&mut self, wire: &str) {
        self.clear();
        let (text, spans) = from_wire(wire);
        self.text = text;
        self.spans = spans;
        self.pending_cursor = Some(self.char_len());
    }

    /// Appends a chip for `target` and a trailing space; the target is trusted without resolving.
    pub fn insert_mention(&mut self, target: MentionRef) {
        self.picked.insert(key_of(&target));
        if !self.text.is_empty() && !self.text.ends_with(char::is_whitespace) {
            self.text.push(' ');
        }
        let end = self.char_len();
        let cursor = edit::insert_chip(&mut self.text, &mut self.spans, end..end, target);
        self.pending_cursor = Some(cursor);
    }

    /// Whether `target` was picked from suggestions or inserted by the app in this composer.
    pub fn was_picked(&self, target: &MentionRef) -> bool {
        self.picked.contains(&key_of(target))
    }

    /// Cancels the focus the text edit would take back after the `+` menu closes.
    pub fn release_focus(&mut self) {
        self.refocus = false;
        self.refocus_again = false;
    }

    pub(super) fn char_len(&self) -> usize {
        self.text.chars().count()
    }

    pub(super) fn buffer_mut(&mut self) -> &mut String {
        &mut self.text
    }

    pub(super) fn parts_mut(&mut self) -> (&mut String, &mut Vec<MentionSpan>) {
        (&mut self.text, &mut self.spans)
    }

    pub(super) fn set_parts(&mut self, text: String, spans: Vec<MentionSpan>) {
        self.text = text;
        self.spans = spans;
    }

    /// Puts a chip for `target` and one space over chars `range`; returns the cursor after the space.
    pub(super) fn accept_mention(&mut self, range: Range<usize>, target: MentionRef) -> usize {
        self.picked.insert(key_of(&target));
        let spaced = self
            .text
            .chars()
            .nth(range.end)
            .is_some_and(char::is_whitespace);
        let range = if spaced {
            range.start..range.end + 1
        } else {
            range
        };
        edit::insert_chip(&mut self.text, &mut self.spans, range, target)
    }

    /// Replaces chars `range` with plain `with`; returns the cursor after it.
    pub(super) fn replace(&mut self, range: Range<usize>, with: &str) -> usize {
        let dead = edit::splice(&mut self.text, &mut self.spans, range.clone(), with);
        edit::bury(&mut self.graveyard, dead);
        range.start + with.chars().count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn released_focus_is_not_taken_back() {
        let mut state = ComposerState {
            refocus: true,
            refocus_again: true,
            ..ComposerState::default()
        };
        state.release_focus();
        assert!(!state.refocus && !state.refocus_again);
    }

    #[test]
    fn wire_round_trips_through_the_buffer() {
        let target = MentionRef::new("session", "desk/t1", "Fix the scroll jump");
        let mut state = ComposerState::default();
        state.text.push_str("ask");
        state.insert_mention(target.clone());
        state.text.push_str("about ümlauts");
        let msg = state.message();
        assert_eq!(msg.display, "ask @Fix the scroll jump about ümlauts");
        assert_eq!(msg.mentions, vec![target.clone()]);
        assert_eq!(msg.wire, format!("ask {} about ümlauts", encode(&target)));

        let mut restored = ComposerState::default();
        restored.set_wire(&msg.wire);
        assert_eq!(restored.message(), msg);
        assert_eq!(restored.spans()[0].chars, 4..24);
        assert_eq!(restored.pending_cursor, Some(msg.display.chars().count()));
    }

    #[test]
    fn clear_buries_chips_for_undo() {
        let mut state = ComposerState::default();
        state.insert_mention(MentionRef::new("session", "1", "a"));
        state.clear();
        assert!(state.is_empty() && state.spans().is_empty());
        assert_eq!(state.graveyard.len(), 1);
        assert_eq!(state.pending_cursor, Some(0));
    }

    #[test]
    fn accepting_a_mention_replaces_the_query_and_trusts_the_target() {
        let mut state = ComposerState::default();
        state.text.push_str("ping @fi please");
        let target = MentionRef::new("session", "7", "Fix it");
        let cursor = state.accept_mention(5..8, target.clone());
        assert_eq!(state.text(), "ping @Fix it please");
        assert_eq!(state.spans()[0].chars, 5..12);
        assert_eq!(cursor, 13);
        assert!(state.was_picked(&target));
    }

    #[test]
    fn the_label_cache_asks_once_per_revision() {
        use std::cell::Cell;
        struct Counting(Cell<usize>);
        impl MentionSource for Counting {
            fn suggest(&self, _: &str, _: usize) -> Vec<crate::chat::MentionCandidate> {
                Vec::new()
            }
            fn resolve(&self, t: &MentionRef) -> Resolution {
                self.0.set(self.0.get() + 1);
                Resolution::Label(format!("{}!", t.label))
            }
        }
        let source = Counting(Cell::new(0));
        let target = MentionRef::new("session", "1", "a");
        let mut cache = LabelCache::default();
        assert_eq!(
            cache.resolve(&source, 0, &target),
            Resolution::Label("a!".into())
        );
        cache.resolve(&source, 0, &target);
        assert_eq!(source.0.get(), 1);
        cache.resolve(&source, 1, &target);
        assert_eq!(source.0.get(), 2);
    }
}
