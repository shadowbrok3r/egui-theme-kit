//! The composer's text buffer with mention spans, and the message it exports.

use std::ops::Range;

use crate::chat::mention::{MentionRef, WireSegment, clean_label, encode, segments};

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

/// Text, mention spans and popup state the app keeps per composer.
#[derive(Clone, Debug, Default)]
pub struct ComposerState {
    text: String,
    spans: Vec<MentionSpan>,
}

impl ComposerState {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.spans.clear();
    }

    /// Mention chips in buffer order.
    pub fn spans(&self) -> &[MentionSpan] {
        &self.spans
    }

    pub(super) fn buffer_mut(&mut self) -> &mut String {
        &mut self.text
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
        for segment in segments(wire) {
            match segment {
                WireSegment::Text(text) => self.text.push_str(text),
                WireSegment::Mention(target) => self.push_span(target),
            }
        }
    }

    /// Appends a chip for `target` and a trailing space.
    pub fn insert_mention(&mut self, target: MentionRef) {
        if !self.text.is_empty() && !self.text.ends_with(char::is_whitespace) {
            self.text.push(' ');
        }
        self.push_span(target);
        self.text.push(' ');
    }

    fn push_span(&mut self, target: MentionRef) {
        let label = format!("@{}", clean_label(&target.label));
        let start = self.text.chars().count();
        self.text.push_str(&label);
        let end = start + label.chars().count();
        self.spans.push(MentionSpan {
            chars: start..end,
            target,
        });
    }
}

/// Byte offset of char index `chars` in `text`.
fn byte_at(text: &str, chars: usize) -> usize {
    text.char_indices()
        .nth(chars)
        .map_or(text.len(), |(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }
}
