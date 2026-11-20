//! Detects an `@` mention query or a leading `/` command at the cursor.

use super::state::MentionSpan;

/// Longest mention query, in characters.
pub(super) const MAX_QUERY_CHARS: usize = 48;

/// What the text before the cursor asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Trigger {
    /// `@query` starting at char `at`.
    Mention { at: usize, query: String },
    /// `/query` at the start of the buffer.
    Slash { query: String },
}

impl Trigger {
    /// Char index of the `@` or `/`.
    pub fn at(&self) -> usize {
        match self {
            Self::Mention { at, .. } => *at,
            Self::Slash { .. } => 0,
        }
    }

    pub fn query(&self) -> &str {
        match self {
            Self::Mention { query, .. } | Self::Slash { query } => query,
        }
    }
}

/// The trigger the collapsed cursor at char `cursor` sits in, if any.
pub(super) fn detect(
    text: &str,
    cursor: usize,
    spans: &[MentionSpan],
    slash: bool,
    mention: bool,
) -> Option<Trigger> {
    let chars: Vec<char> = text.chars().collect();
    let cursor = cursor.min(chars.len());
    if slash && chars.first() == Some(&'/') && cursor >= 1 {
        let token_end = chars
            .iter()
            .position(|c| c.is_whitespace())
            .unwrap_or(chars.len());
        if cursor <= token_end {
            return Some(Trigger::Slash {
                query: chars[1..cursor].iter().collect(),
            });
        }
    }
    if !mention {
        return None;
    }
    let floor = cursor.saturating_sub(MAX_QUERY_CHARS);
    let at = (floor..cursor)
        .rev()
        .find(|&i| matches!(chars[i], '@' | '\n'))?;
    if chars[at] != '@' {
        return None;
    }
    let boundary = at == 0 || !(chars[at - 1].is_alphanumeric() || chars[at - 1] == '_');
    let in_chip = spans
        .iter()
        .any(|s| s.chars.start < cursor && at < s.chars.end);
    let query: String = chars[at + 1..cursor].iter().collect();
    let closed = query.contains("  ") || query.chars().count() >= MAX_QUERY_CHARS;
    (boundary && !in_chip && !closed).then_some(Trigger::Mention { at, query })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::mention::MentionRef;

    fn at(text: &str, cursor: usize) -> Option<Trigger> {
        detect(text, cursor, &[], true, true)
    }

    fn mention(at: usize, query: &str) -> Option<Trigger> {
        Some(Trigger::Mention {
            at,
            query: query.to_owned(),
        })
    }

    #[test]
    fn mentions_start_at_a_boundary_and_allow_single_spaces() {
        assert_eq!(at("@", 1), mention(0, ""));
        assert_eq!(at("hi @fix the", 11), mention(3, "fix the"));
        assert_eq!(at("(@fi", 4), mention(1, "fi"));
        assert_eq!(at("mail a@b", 8), None);
        assert_eq!(at("@fix  x", 7), None);
        assert_eq!(at("@a\nb", 4), None);
        assert_eq!(at("@fix", 2), mention(0, "f"));
    }

    #[test]
    fn long_queries_close() {
        let long = format!("@{}", "x".repeat(MAX_QUERY_CHARS));
        assert_eq!(at(&long, long.chars().count()), None);
    }

    #[test]
    fn chips_do_not_trigger() {
        let spans = [MentionSpan {
            chars: 0..4,
            target: MentionRef::new("session", "1", "abc"),
        }];
        assert_eq!(detect("@abc x", 6, &spans, true, true), None);
        assert_eq!(detect("@abc x", 2, &spans, true, true), None);
        assert_eq!(detect("@abc @x", 7, &spans, true, true), mention(5, "x"));
    }

    #[test]
    fn slash_needs_the_cursor_in_the_first_token() {
        assert_eq!(
            at("/comp", 5),
            Some(Trigger::Slash {
                query: "comp".into()
            })
        );
        assert_eq!(at("/compact now", 12), None);
        assert_eq!(detect("/comp", 5, &[], false, true), None);
        assert_eq!(at("x /comp", 7), None);
    }
}
