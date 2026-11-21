//! Mentions of sessions and other targets: the wire form `@[label](mention:kind/id)` and its providers.

use serde::{Deserialize, Serialize};

use super::types::SessionStatus;

/// Longest label a mention keeps, in characters.
pub const MAX_LABEL_CHARS: usize = 80;

const OPEN: &str = "@[";
const SCHEME: &str = "](mention:";

/// A stable reference to a mentioned target; `label` is the name shown when it was inserted.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MentionRef {
    /// Target class, such as `session`; never contains `/`.
    pub kind: String,
    /// Stable id within `kind`.
    pub id: String,
    pub label: String,
}

impl MentionRef {
    /// A reference with a cleaned label.
    pub fn new(kind: impl Into<String>, id: impl Into<String>, label: &str) -> Self {
        Self {
            kind: kind.into(),
            id: id.into(),
            label: clean_label(label),
        }
    }

    /// `kind/id`, the canonical target string a tool or context expansion accepts.
    pub fn target(&self) -> String {
        format!("{}/{}", self.kind, self.id)
    }
}

/// `label` with control characters turned into spaces, runs of spaces collapsed, trimmed and capped at [`MAX_LABEL_CHARS`].
pub fn clean_label(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    for c in label.chars() {
        let c = if c.is_control() { ' ' } else { c };
        if c == ' ' && (out.is_empty() || out.ends_with(' ')) {
            continue;
        }
        out.push(c);
    }
    let trimmed = out.trim_end();
    match trimmed.char_indices().nth(MAX_LABEL_CHARS) {
        Some((cut, _)) => trimmed[..cut].trim_end().to_owned(),
        None => trimmed.to_owned(),
    }
}

/// The wire form of `m`: label escapes `\`, `[` and `]`; the id escapes `%`, parentheses, whitespace and controls.
pub fn encode(m: &MentionRef) -> String {
    let mut out = String::with_capacity(m.label.len() + m.kind.len() + m.id.len() + 16);
    out.push_str(OPEN);
    for c in clean_label(&m.label).chars() {
        if matches!(c, '\\' | '[' | ']') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push_str(SCHEME);
    push_escaped(&mut out, &m.kind, true);
    out.push('/');
    push_escaped(&mut out, &m.id, false);
    out.push(')');
    out
}

fn push_escaped(out: &mut String, part: &str, escape_slash: bool) {
    for c in part.chars() {
        let reserved = matches!(c, '%' | '(' | ')') || c.is_whitespace() || c.is_control();
        if reserved || (escape_slash && c == '/') {
            let mut buf = [0u8; 4];
            for b in c.encode_utf8(&mut buf).bytes() {
                out.push_str(&format!("%{b:02X}"));
            }
        } else {
            out.push(c);
        }
    }
}

fn unescape_percent(part: &str) -> Option<String> {
    let bytes = part.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = part.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// A run of wire text: plain text or one decoded mention.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WireSegment<'a> {
    Text(&'a str),
    Mention(MentionRef),
}

/// Splits wire text into plain runs and mentions; malformed mention syntax stays text.
pub fn segments(wire: &str) -> WireSegments<'_> {
    WireSegments { rest: wire }
}

/// Iterator returned by [`segments`].
#[derive(Clone, Debug)]
pub struct WireSegments<'a> {
    rest: &'a str,
}

impl<'a> Iterator for WireSegments<'a> {
    type Item = WireSegment<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.rest.is_empty() {
            return None;
        }
        let mut from = 0;
        while let Some(at) = self.rest[from..].find(OPEN).map(|i| i + from) {
            if let Some((mention, len)) = parse_mention(&self.rest[at..]) {
                if at > 0 {
                    let text = &self.rest[..at];
                    self.rest = &self.rest[at..];
                    return Some(WireSegment::Text(text));
                }
                self.rest = &self.rest[len..];
                return Some(WireSegment::Mention(mention));
            }
            from = at + OPEN.len();
        }
        let text = self.rest;
        self.rest = "";
        Some(WireSegment::Text(text))
    }
}

/// The mention at the start of `s` and its byte length.
pub(crate) fn parse_mention(s: &str) -> Option<(MentionRef, usize)> {
    let body = s.strip_prefix(OPEN)?;
    let mut label = String::new();
    let mut chars = body.char_indices();
    let close = loop {
        let (i, c) = chars.next()?;
        match c {
            '\\' => label.push(chars.next()?.1),
            ']' => break i,
            '[' | '\n' | '\r' => return None,
            c => label.push(c),
        }
    };
    let after = body[close..].strip_prefix(SCHEME)?;
    let end = after.find(')')?;
    let (kind, id) = after[..end].split_once('/')?;
    if kind.is_empty() || id.is_empty() || after[..end].contains(char::is_whitespace) {
        return None;
    }
    let mention = MentionRef {
        kind: unescape_percent(kind)?,
        id: unescape_percent(id)?,
        label: clean_label(&label),
    };
    let len = OPEN.len() + close + SCHEME.len() + end + 1;
    Some((mention, len))
}

/// One suggestion for an `@` query.
#[derive(Clone, Debug, PartialEq)]
pub struct MentionCandidate {
    pub target: MentionRef,
    /// Dim text after the label, such as the host or folder.
    pub detail: String,
    pub status: Option<SessionStatus>,
    /// Group the candidate is listed under.
    pub section: Option<String>,
}

/// What a [`MentionSource`] knows about a mention target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolution {
    /// The target exists under this label.
    Label(String),
    /// The target does not exist or the user cannot reach it.
    Missing,
    /// The source does not check targets; chips keep their own label.
    Unknown,
}

impl Resolution {
    /// `Label` for `Some`, `Missing` for `None`.
    pub fn found(label: Option<String>) -> Self {
        label.map_or(Self::Missing, Self::Label)
    }
}

/// Supplies mention suggestions and current labels; the app checks access in both.
pub trait MentionSource {
    /// Up to `limit` candidates for `query`, best first.
    fn suggest(&self, query: &str, limit: usize) -> Vec<MentionCandidate>;

    /// The target's current label; [`Resolution::Unknown`] unless the source checks targets.
    fn resolve(&self, _target: &MentionRef) -> Resolution {
        Resolution::Unknown
    }
}

impl<F: Fn(&str, usize) -> Vec<MentionCandidate>> MentionSource for F {
    fn suggest(&self, query: &str, limit: usize) -> Vec<MentionCandidate> {
        self(query, limit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str, label: &str) -> MentionRef {
        MentionRef::new("session", id, label)
    }

    #[test]
    fn encode_and_segments_round_trip_reserved_characters() {
        let m = session("desk/t 1(a)%", "Fix [the] \\ scroll");
        let wire = encode(&m);
        assert_eq!(
            wire,
            "@[Fix \\[the\\] \\\\ scroll](mention:session/desk/t%201%28a%29%25)"
        );
        let text = format!("see {wire}, then reply");
        let parts: Vec<_> = segments(&text).collect();
        assert_eq!(
            parts,
            vec![
                WireSegment::Text("see "),
                WireSegment::Mention(m),
                WireSegment::Text(", then reply"),
            ]
        );
    }

    #[test]
    fn malformed_mentions_and_plain_links_stay_text() {
        for text in [
            "@[open",
            "@[x](https://example.com)",
            "@[x](mention:session)",
            "@[x](mention:/id)",
            "@[a\nb](mention:session/1)",
            "@[x](mention:session/%ZZ)",
        ] {
            let parts: Vec<_> = segments(text).collect();
            assert_eq!(parts, vec![WireSegment::Text(text)], "{text}");
        }
        assert_eq!(segments("").count(), 0);
    }

    #[test]
    fn adjacent_mentions_split_cleanly() {
        let (a, b) = (session("1", "A"), session("2", "B"));
        let text = format!("@[x{}{}", encode(&a), encode(&b));
        let parts: Vec<_> = segments(&text).collect();
        assert_eq!(
            parts,
            vec![
                WireSegment::Text("@[x"),
                WireSegment::Mention(a),
                WireSegment::Mention(b),
            ]
        );
    }

    #[test]
    fn labels_lose_controls_and_runs_of_spaces_and_are_capped() {
        assert_eq!(clean_label("  a\n\tb   c  "), "a b c");
        let long = "x".repeat(MAX_LABEL_CHARS + 20);
        assert_eq!(clean_label(&long).chars().count(), MAX_LABEL_CHARS);
    }
}
