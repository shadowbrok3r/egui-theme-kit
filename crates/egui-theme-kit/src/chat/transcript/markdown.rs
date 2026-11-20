//! Chat markdown: headings, lists, quotes, rules, fenced code, inline spans, mentions and JSON bodies.

use std::sync::Arc;

use egui::{
    Align, Color32, FontId, Id, Layout, Margin, Sense, Stroke, TextFormat, Ui, Vec2,
    cache::{ComputerMut, FrameCache},
    text::LayoutJob,
    vec2,
};
use serde_json::Value;

use super::payload::{self, Part};
use super::rich::{Hit, Rich};
use super::row::{Fonts, RowCx};
use super::shell;
use crate::chat::mention::{MentionRef, parse_mention};
use crate::chat::surface::{SurfaceKind, surface_frame};
use crate::controls::{IconBox, IconButton, icons};

/// Points each list nesting level is inset by.
const LIST_INDENT: f32 = 12.0;
/// Inset of a list marker.
const MARKER_INSET: f32 = 6.0;
const QUOTE_INSET: f32 = 10.0;
/// Heading size as a multiple of body text, by level.
const HEADING_SCALE: [f32; 6] = [1.4, 1.25, 1.12, 1.05, 1.0, 1.0];
/// Longest code block drawn; the rest is copied but not laid out.
pub(crate) const CODE_MAX_CHARS: usize = 20_000;

/// How a list item is numbered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Marker<'a> {
    Bullet,
    Number(&'a str),
}

/// One block of a message body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Block<'a> {
    Heading {
        level: usize,
        text: &'a str,
    },
    Line {
        depth: usize,
        text: &'a str,
    },
    Item {
        depth: usize,
        marker: Marker<'a>,
        text: &'a str,
    },
    Quote(&'a str),
    /// A `$ command` line.
    Shell(&'a str),
    Code {
        lang: &'a str,
        code: &'a str,
    },
    Rule,
    Blank,
}

/// How an inline run is styled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpanKind<'a> {
    Plain,
    Bold,
    Italic,
    Code,
    Link(&'a str),
    /// A wire-form mention; the span text is the raw wire text.
    Mention(MentionRef),
}

/// One styled run within a line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span<'a> {
    pub text: &'a str,
    pub kind: SpanKind<'a>,
}

/// Lines of `text` with the byte offsets of their start and of the next line.
fn lines(text: &str) -> impl Iterator<Item = (&str, usize, usize)> {
    let mut pos = 0;
    text.split_inclusive('\n').map(move |raw| {
        let start = pos;
        pos += raw.len();
        let line = raw.strip_suffix('\n').unwrap_or(raw);
        (line.strip_suffix('\r').unwrap_or(line), start, pos)
    })
}

/// Splits a message body into blocks; an unclosed fence runs to the end of the text.
pub fn blocks(text: &str) -> Vec<Block<'_>> {
    let mut out = Vec::new();
    let mut iter = lines(text);
    while let Some((line, _, after)) = iter.next() {
        if let Some(lang) = line.trim_start().strip_prefix("```") {
            let mut end = text.len();
            for (inner, start, _) in iter.by_ref() {
                if inner.trim_start().starts_with("```") {
                    end = start;
                    break;
                }
            }
            let code = text[after.min(end)..end].trim_end_matches(['\n', '\r']);
            out.push(Block::Code {
                lang: lang.trim(),
                code,
            });
            continue;
        }
        out.push(line_block(line));
    }
    out
}

fn line_block(line: &str) -> Block<'_> {
    let body = line.trim_start();
    let depth = indent_depth(&line[..line.len() - body.len()]);
    let body = body.trim_end();
    if body.is_empty() {
        return Block::Blank;
    }
    if let Some((level, text)) = heading(body) {
        return Block::Heading { level, text };
    }
    if is_rule(body) {
        return Block::Rule;
    }
    if let Some((marker, text)) = list_item(body) {
        return Block::Item {
            depth,
            marker,
            text,
        };
    }
    if let Some(text) = body.strip_prefix('>') {
        return Block::Quote(text.trim_start());
    }
    if body.starts_with("$ ") {
        return Block::Shell(body);
    }
    Block::Line { depth, text: body }
}

/// Nesting level of a line's leading whitespace, two columns per level and a tab as four.
fn indent_depth(ws: &str) -> usize {
    let cols: usize = ws.chars().map(|c| if c == '\t' { 4 } else { 1 }).sum();
    (cols / 2).min(8)
}

fn heading(body: &str) -> Option<(usize, &str)> {
    let level = body.bytes().take_while(|b| *b == b'#').count();
    let rest = body.get(level..)?;
    ((1..=6).contains(&level) && rest.starts_with(' '))
        .then(|| (level, rest.trim().trim_end_matches('#').trim_end()))
}

/// Three or more of one of `-`, `*`, `_`, optionally spaced.
fn is_rule(body: &str) -> bool {
    let mut marks = body.chars().filter(|c| !c.is_whitespace());
    let Some(first) = marks.next().filter(|c| matches!(c, '-' | '*' | '_')) else {
        return false;
    };
    let mut count = 1;
    for c in marks {
        if c != first {
            return false;
        }
        count += 1;
    }
    count >= 3
}

fn list_item(body: &str) -> Option<(Marker<'_>, &str)> {
    for bullet in ["- ", "* ", "+ "] {
        if let Some(rest) = body.strip_prefix(bullet) {
            return Some((Marker::Bullet, rest.trim_start()));
        }
    }
    let digits = body.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 || digits > 9 {
        return None;
    }
    let rest = &body[digits..];
    (rest.starts_with(". ") || rest.starts_with(") "))
        .then(|| (Marker::Number(&body[..=digits]), rest[2..].trim_start()))
}

/// Splits a line into code, mention, bold, italic and link runs; unpaired markers stay literal.
pub fn inline_spans(line: &str) -> Vec<Span<'_>> {
    let mut out = Vec::new();
    let mut plain = 0;
    let mut i = 0;
    while i < line.len() {
        if let Some((consumed, span)) = styled_at(line, i) {
            if plain < i {
                out.push(Span {
                    text: &line[plain..i],
                    kind: SpanKind::Plain,
                });
            }
            out.push(span);
            i += consumed;
            plain = i;
            continue;
        }
        i += line[i..].chars().next().map_or(1, char::len_utf8);
    }
    if plain < line.len() {
        out.push(Span {
            text: &line[plain..],
            kind: SpanKind::Plain,
        });
    }
    out
}

/// The styled run opening at byte `i`, with the number of bytes it spans.
fn styled_at(line: &str, i: usize) -> Option<(usize, Span<'_>)> {
    let rest = &line[i..];
    if let Some(body) = rest.strip_prefix('`') {
        let n = body.find('`').filter(|n| *n > 0)?;
        return Some((
            n + 2,
            Span {
                text: &body[..n],
                kind: SpanKind::Code,
            },
        ));
    }
    if rest.starts_with("@[")
        && let Some((mention, len)) = parse_mention(rest)
    {
        return Some((
            len,
            Span {
                text: &rest[..len],
                kind: SpanKind::Mention(mention),
            },
        ));
    }
    if let Some(body) = rest.strip_prefix("**") {
        let n = closing(body, "**", false)?;
        return Some((
            n + 4,
            Span {
                text: &body[..n],
                kind: SpanKind::Bold,
            },
        ));
    }
    if rest.starts_with('[') {
        return link(rest);
    }
    let (marker, word_bound) = match rest.as_bytes().first() {
        Some(b'*') => ("*", false),
        Some(b'_') if !prev_is_word(line, i) => ("_", true),
        _ => return None,
    };
    let body = &rest[1..];
    let n = closing(body, marker, word_bound)?;
    Some((
        n + 2,
        Span {
            text: &body[..n],
            kind: SpanKind::Italic,
        },
    ))
}

/// Offset of a closing marker; the run has no edge whitespace and a `word_bound` closer ends a word.
fn closing(body: &str, marker: &str, word_bound: bool) -> Option<usize> {
    if body.chars().next().is_none_or(char::is_whitespace) {
        return None;
    }
    body.match_indices(marker).map(|(n, _)| n).find(|&n| {
        n > 0
            && body[..n]
                .chars()
                .next_back()
                .is_some_and(|c| !c.is_whitespace())
            && !(word_bound && next_is_word(body, n + marker.len()))
    })
}

fn link(rest: &str) -> Option<(usize, Span<'_>)> {
    let close = rest.find("](")?;
    let label = &rest[1..close];
    if label.is_empty() || label.contains('[') {
        return None;
    }
    let after = &rest[close + 2..];
    let end = after.find(')')?;
    let url = &after[..end];
    if url.is_empty() || url.contains(char::is_whitespace) {
        return None;
    }
    Some((
        close + 3 + end,
        Span {
            text: label,
            kind: SpanKind::Link(url),
        },
    ))
}

/// True when the character before byte `i` is part of a word.
fn prev_is_word(line: &str, i: usize) -> bool {
    line[..i]
        .chars()
        .next_back()
        .is_some_and(char::is_alphanumeric)
}

/// True when the character at byte `i` is part of a word.
fn next_is_word(line: &str, i: usize) -> bool {
    line.get(i..)
        .and_then(|s| s.chars().next())
        .is_some_and(char::is_alphanumeric)
}

/// Parses text that is nothing but JSON, allowing several values back to back.
pub fn as_json(text: &str) -> Option<Vec<Value>> {
    let t = text.trim();
    if !(t.starts_with('{') || t.starts_with('[')) {
        return None;
    }
    let mut stream = serde_json::Deserializer::from_str(t).into_iter::<Value>();
    let mut out = Vec::new();
    for v in stream.by_ref() {
        match v {
            Ok(v) => out.push(v),
            Err(_) => return repair_cut_json(t).map(|v| vec![v]),
        }
    }
    (stream.byte_offset() >= t.len() && !out.is_empty()).then_some(out)
}

/// JSON cut short with a trailing `…`, closed back up: the open string, then each open bracket.
fn repair_cut_json(text: &str) -> Option<Value> {
    let body = text.trim_end().strip_suffix('\u{2026}')?.trim_end();
    let mut open: Vec<char> = Vec::new();
    let mut in_string = false;
    let mut escaped = false;
    let mut last_comma: Option<(usize, Vec<char>)> = None;
    for (i, c) in body.char_indices() {
        if in_string {
            match c {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' => open.push('}'),
            '[' => open.push(']'),
            '}' | ']' => {
                open.pop();
            }
            ',' => last_comma = Some((i, open.clone())),
            _ => {}
        }
    }
    let mut whole = body.to_string();
    if in_string {
        if escaped {
            whole.pop();
        }
        whole.push('"');
    }
    whole.extend(open.iter().rev());
    serde_json::from_str(&whole).ok().or_else(|| {
        // Drops the member after the last comma when that member itself was cut.
        let (at, open) = last_comma?;
        let mut head = body[..at].to_string();
        head.extend(open.iter().rev());
        serde_json::from_str(&head).ok()
    })
}

#[derive(Default)]
struct JsonParser;

impl ComputerMut<&str, Option<Arc<Vec<Value>>>> for JsonParser {
    fn compute(&mut self, text: &str) -> Option<Arc<Vec<Value>>> {
        as_json(text).map(Arc::new)
    }
}

type JsonCache = FrameCache<Option<Arc<Vec<Value>>>, JsonParser>;

/// [`as_json`], cached by content while the same text is drawn every frame.
pub(crate) fn cached_json(ctx: &egui::Context, text: &str) -> Option<Arc<Vec<Value>>> {
    let t = text.trim_start();
    if !(t.starts_with('{') || t.starts_with('[')) {
        return None;
    }
    ctx.memory_mut(|m| m.caches.cache::<JsonCache>().get(text).clone())
}

/// Width text may wrap to: never more than the visible viewport.
pub(crate) fn wrap_width(ui: &Ui) -> f32 {
    ui.available_width().min(ui.clip_rect().width()).max(48.0)
}

/// Renders a message body in `ink`; trees and code blocks are keyed under `id`.
pub(crate) fn show(ui: &mut Ui, rc: &mut RowCx<'_>, text: &str, ink: Color32, id: Id, user: bool) {
    if let Some(values) = cached_json(ui.ctx(), text) {
        payload::values(ui, rc, &values, id.with("json"), Part::Result);
        return;
    }
    let mut blank = true;
    for (n, block) in blocks(text).into_iter().enumerate() {
        if block == Block::Blank {
            if !blank {
                ui.add_space(rc.f.body.size * 0.35);
            }
            blank = true;
            continue;
        }
        blank = false;
        match block {
            Block::Heading { level, text } => {
                ui.add_space(rc.m.gap.y / 2.0);
                let scale = HEADING_SCALE
                    .get(level.saturating_sub(1))
                    .copied()
                    .unwrap_or(1.0);
                let font = crate::controls::semibold_font_sized(ui, rc.f.body.size * scale);
                let ink = rc.p.text;
                spans_label(ui, rc, text, &font, ink, user);
            }
            Block::Line { depth, text } => indented(ui, depth, |ui| {
                let font = rc.f.body.clone();
                spans_label(ui, rc, text, &font, ink, user);
            }),
            Block::Item {
                depth,
                marker,
                text,
            } => {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.add_space(MARKER_INSET + depth as f32 * LIST_INDENT);
                    let row_h = RowCx::row_h(ui, &rc.f.body);
                    match marker {
                        Marker::Bullet => {
                            let (slot, _) = ui.allocate_exact_size(
                                vec2(rc.f.body.size * 0.6, row_h),
                                Sense::hover(),
                            );
                            let r = (rc.f.body.size * 0.14).max(1.5);
                            ui.painter().circle_filled(slot.center(), r, rc.p.text_dim);
                        }
                        Marker::Number(n) => {
                            let job = LayoutJob::simple_singleline(
                                n.to_owned(),
                                rc.f.body.clone(),
                                rc.p.text_dim,
                            );
                            let galley = ui.ctx().fonts_mut(|f| f.layout_job(job));
                            ui.add(egui::Label::new(galley).selectable(false));
                        }
                    }
                    let font = rc.f.body.clone();
                    spans_label(ui, rc, text, &font, ink, user);
                });
            }
            Block::Quote(text) => {
                let quoted = ui.horizontal_top(|ui| {
                    ui.add_space(QUOTE_INSET);
                    let (font, dim) = (rc.f.body.clone(), rc.p.text_dim);
                    spans_label(ui, rc, text, &font, dim, user);
                });
                let r = quoted.response.rect;
                ui.painter().vline(
                    r.left() + 3.0,
                    r.y_range(),
                    Stroke::new(2.0, rc.p.separator),
                );
            }
            Block::Shell(line) => shell_line(ui, rc, line),
            Block::Code { lang, code } => code_block(ui, rc, lang, code, id.with(("code", n))),
            Block::Rule => rule(ui, rc),
            Block::Blank => {}
        }
    }
}

fn indented(ui: &mut Ui, depth: usize, add: impl FnOnce(&mut Ui)) {
    if depth == 0 {
        add(ui);
        return;
    }
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(depth as f32 * LIST_INDENT);
        add(ui);
    });
}

/// Lays out one line of inline spans as a single wrapped label.
pub(crate) fn spans_label(
    ui: &mut Ui,
    rc: &mut RowCx<'_>,
    line: &str,
    font: &FontId,
    color: Color32,
    user: bool,
) {
    let mut rich = Rich::new(wrap_width(ui));
    append_spans(ui, rc, &mut rich, line, font, color, user);
    rich.show(ui, rc);
}

/// Appends the inline spans of `line` to `rich`.
pub(crate) fn append_spans(
    ui: &Ui,
    rc: &RowCx<'_>,
    rich: &mut Rich,
    line: &str,
    font: &FontId,
    color: Color32,
    user: bool,
) {
    let ctx = ui.ctx();
    let mono = FontId::new(font.size * 0.95, rc.f.mono.family.clone());
    let plain = |font: &FontId, color| TextFormat::simple(font.clone(), color);
    for span in inline_spans(line) {
        match span.kind {
            SpanKind::Plain => rich.text(ctx, span.text, plain(font, color)),
            SpanKind::Bold => rich.text(ctx, span.text, plain(&Fonts::bold(ui, font), color)),
            SpanKind::Italic => rich.text(
                ctx,
                span.text,
                TextFormat {
                    italics: true,
                    ..plain(font, color)
                },
            ),
            SpanKind::Code => rich.text(
                ctx,
                span.text,
                TextFormat {
                    background: rc.p.code_chip,
                    ..plain(&mono, rc.p.text)
                },
            ),
            SpanKind::Link(url) => rich.target(
                ctx,
                span.text,
                TextFormat {
                    underline: Stroke::new(1.0, rc.p.link),
                    ..plain(font, rc.p.link)
                },
                Some(Hit::Link(url.to_owned())),
                Some(url.to_owned()),
            ),
            SpanKind::Mention(m) => rich.mention(ui, rc, &m, font, user),
        }
    }
}

/// A `$ command` line with the prompt faint and the command coloured.
pub(crate) fn shell_line(ui: &mut Ui, rc: &mut RowCx<'_>, line: &str) {
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap_width(ui);
    job.wrap.break_anywhere = true;
    let cmd = match line.strip_prefix("$ ") {
        Some(rest) => {
            job.append(
                "$ ",
                0.0,
                TextFormat::simple(rc.f.mono.clone(), rc.p.text_faint),
            );
            rest
        }
        None => line,
    };
    shell::append(&mut job, cmd, &rc.f.mono, &rc.p.shell);
    let galley = ui.ctx().fonts_mut(|f| f.layout_job(job));
    ui.add(egui::Label::new(galley).selectable(rc.selectable));
}

/// `text` cut to `max` characters, with the number of characters left out.
pub(crate) fn capped(text: &str, max: usize) -> (&str, usize) {
    match text.char_indices().nth(max) {
        Some((cut, _)) => (&text[..cut], text[cut..].chars().count()),
        None => (text, 0),
    }
}

/// A fenced code block on a plate with its language, a copy button and highlighting.
pub(crate) fn code_block(ui: &mut Ui, rc: &mut RowCx<'_>, lang: &str, code: &str, id: Id) {
    let json_fence = lang.is_empty() || lang.eq_ignore_ascii_case("json");
    if json_fence && let Some(values) = cached_json(ui.ctx(), code) {
        payload::values(ui, rc, &values, id, Part::Result);
        return;
    }
    let margin = Margin::symmetric(rc.m.gap.x as i8, (rc.m.gap.y / 2.0) as i8);
    surface_frame(ui, id, SurfaceKind::Plate, margin, |ui| {
        ui.set_min_width(ui.available_width());
        let shell_lang = shell::is_shell_lang(lang);
        ui.horizontal(|ui| {
            let icon = if shell_lang {
                rc.icons.command
            } else {
                icons::FILE
            };
            let (slot, _) = ui.allocate_exact_size(Vec2::splat(rc.m.icon_sm), Sense::hover());
            icon.paint(ui, slot, rc.m.icon_sm, rc.p.text_faint);
            let name = if lang.is_empty() { "code" } else { lang };
            let job =
                LayoutJob::simple_singleline(name.to_owned(), rc.f.small.clone(), rc.p.text_faint);
            let galley = ui.ctx().fonts_mut(|f| f.layout_job(job));
            ui.add(egui::Label::new(galley).selectable(false));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let copy = IconButton::new(rc.icons.copy, IconBox::Sm)
                    .tip("Copy code")
                    .show(ui);
                if rc.tapped(&copy) {
                    rc.copy(ui, code.to_owned());
                }
            });
        });
        let (shown, left) = capped(code, CODE_MAX_CHARS);
        let mut job = if shell_lang {
            let mut job = LayoutJob::default();
            shell::append(&mut job, shown, &rc.f.mono, &rc.p.shell);
            job
        } else {
            highlighted(ui, rc, shown, lang)
        };
        job.wrap.max_width = wrap_width(ui);
        job.wrap.break_anywhere = true;
        let galley = ui.ctx().fonts_mut(|f| f.layout_job(job));
        ui.add(egui::Label::new(galley).selectable(rc.selectable));
        if left > 0 {
            more_note(ui, rc, left);
        }
    });
}

/// A faint line saying how many characters were left out.
pub(crate) fn more_note(ui: &mut Ui, rc: &RowCx<'_>, left: usize) {
    let job = LayoutJob::simple_singleline(
        format!("{left} more characters; copy to see all"),
        rc.f.small.clone(),
        rc.p.text_faint,
    );
    let galley = ui.ctx().fonts_mut(|f| f.layout_job(job));
    ui.add(egui::Label::new(galley).selectable(false));
}

#[cfg(feature = "chat-syntax")]
fn highlighted(ui: &Ui, rc: &RowCx<'_>, code: &str, lang: &str) -> LayoutJob {
    if lang.is_empty() {
        return LayoutJob::simple(code.to_owned(), rc.f.mono.clone(), rc.p.text, f32::INFINITY);
    }
    let mut style = (**ui.style()).clone();
    style.visuals.dark_mode = rc.p.dark;
    let theme = egui_extras::syntax_highlighting::CodeTheme::from_style(&style);
    egui_extras::syntax_highlighting::highlight(ui.ctx(), &style, &theme, code, lang)
}

#[cfg(not(feature = "chat-syntax"))]
fn highlighted(_ui: &Ui, rc: &RowCx<'_>, code: &str, _lang: &str) -> LayoutJob {
    LayoutJob::simple(code.to_owned(), rc.f.mono.clone(), rc.p.text, f32::INFINITY)
}

fn rule(ui: &mut Ui, rc: &RowCx<'_>) {
    let (rect, _) = ui.allocate_exact_size(vec2(wrap_width(ui), 7.0), Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        Stroke::new(1.0, rc.p.separator),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::mention::encode;

    fn kinds<'a>(spans: &[Span<'a>]) -> Vec<(&'a str, SpanKind<'a>)> {
        spans.iter().map(|s| (s.text, s.kind.clone())).collect()
    }

    #[test]
    fn a_body_splits_into_headings_lists_quotes_rules_and_lines() {
        let text = "# Title\n\nSome **prose** here.\n- one\n  - nested\n12. twelve\n> quoted\n---\n$ ls -la\n   indented";
        assert_eq!(
            blocks(text),
            vec![
                Block::Heading {
                    level: 1,
                    text: "Title"
                },
                Block::Blank,
                Block::Line {
                    depth: 0,
                    text: "Some **prose** here."
                },
                Block::Item {
                    depth: 0,
                    marker: Marker::Bullet,
                    text: "one"
                },
                Block::Item {
                    depth: 1,
                    marker: Marker::Bullet,
                    text: "nested"
                },
                Block::Item {
                    depth: 0,
                    marker: Marker::Number("12."),
                    text: "twelve"
                },
                Block::Quote("quoted"),
                Block::Rule,
                Block::Shell("$ ls -la"),
                Block::Line {
                    depth: 1,
                    text: "indented"
                },
            ]
        );
    }

    #[test]
    fn a_fence_keeps_its_language_and_its_lines_verbatim() {
        let text = "before\n```rust\nfn main() {\n    let x = 1;\n}\n```\nafter";
        assert_eq!(
            blocks(text),
            vec![
                Block::Line {
                    depth: 0,
                    text: "before"
                },
                Block::Code {
                    lang: "rust",
                    code: "fn main() {\n    let x = 1;\n}"
                },
                Block::Line {
                    depth: 0,
                    text: "after"
                },
            ]
        );
    }

    #[test]
    fn an_unclosed_fence_runs_to_the_end_of_a_streaming_body() {
        assert_eq!(
            blocks("```json\n{\"a\": 1,\n"),
            vec![Block::Code {
                lang: "json",
                code: "{\"a\": 1,"
            }]
        );
        assert_eq!(
            blocks("text\n```"),
            vec![
                Block::Line {
                    depth: 0,
                    text: "text"
                },
                Block::Code { lang: "", code: "" }
            ]
        );
    }

    #[test]
    fn crlf_bodies_split_like_lf_ones() {
        assert_eq!(
            blocks("a\r\n```\r\nx\r\n```\r\nb"),
            vec![
                Block::Line {
                    depth: 0,
                    text: "a"
                },
                Block::Code {
                    lang: "",
                    code: "x"
                },
                Block::Line {
                    depth: 0,
                    text: "b"
                }
            ]
        );
    }

    #[test]
    fn near_misses_stay_plain_lines() {
        for text in ["#hashtag", "-no space", "--", "3.14 is pi", "####### seven"] {
            assert_eq!(line_block(text), Block::Line { depth: 0, text }, "{text}");
        }
        assert_eq!(line_block("* * *"), Block::Rule);
        assert_eq!(
            line_block("## Closing hashes ##"),
            Block::Heading {
                level: 2,
                text: "Closing hashes"
            }
        );
    }

    #[test]
    fn inline_spans_style_code_bold_italic_and_links() {
        assert_eq!(
            kinds(&inline_spans(
                "plain `code` **bold** *it* see [docs](https://x.y/z) end"
            )),
            vec![
                ("plain ", SpanKind::Plain),
                ("code", SpanKind::Code),
                (" ", SpanKind::Plain),
                ("bold", SpanKind::Bold),
                (" ", SpanKind::Plain),
                ("it", SpanKind::Italic),
                (" see ", SpanKind::Plain),
                ("docs", SpanKind::Link("https://x.y/z")),
                (" end", SpanKind::Plain),
            ]
        );
    }

    #[test]
    fn mentions_parse_before_links() {
        let m = MentionRef::new("session", "desk/t1", "Fix [the] scroll");
        let wire = encode(&m);
        let line = format!("ask {wire} or [docs](https://x.y)");
        let spans = inline_spans(&line);
        assert_eq!(spans[0].kind, SpanKind::Plain);
        assert_eq!(spans[1].kind, SpanKind::Mention(m));
        assert_eq!(spans[1].text, wire);
        assert_eq!(spans[3].kind, SpanKind::Link("https://x.y"));
        let code_line = format!("`{wire}`");
        let coded = inline_spans(&code_line);
        assert_eq!(coded[0].kind, SpanKind::Code);
    }

    #[test]
    fn underscores_inside_a_word_are_not_italics() {
        let v = inline_spans("query_surrealdb reads get_client_info.json");
        assert_eq!(
            kinds(&v),
            vec![(
                "query_surrealdb reads get_client_info.json",
                SpanKind::Plain
            )]
        );
        assert_eq!(
            kinds(&inline_spans("say _this_ loudly"))[1],
            ("this", SpanKind::Italic)
        );
    }

    #[test]
    fn unmatched_or_spaced_markers_stay_literal() {
        for text in [
            "2 * 3 = 6",
            "a **b",
            "rate: 5* ",
            "2 * 3 * 4",
            "[not a link]",
            "[a](has space)",
            "`",
            "``",
            "@[open",
        ] {
            assert_eq!(
                kinds(&inline_spans(text)),
                vec![(text, SpanKind::Plain)],
                "{text}"
            );
        }
    }

    #[test]
    fn bold_wins_over_italic_and_multibyte_text_survives() {
        assert_eq!(
            kinds(&inline_spans("**both**")),
            vec![("both", SpanKind::Bold)]
        );
        let v = inline_spans("h\u{e9}llo **w\u{f6}rld** end");
        assert_eq!(
            kinds(&v),
            vec![
                ("h\u{e9}llo ", SpanKind::Plain),
                ("w\u{f6}rld", SpanKind::Bold),
                (" end", SpanKind::Plain)
            ]
        );
    }

    #[test]
    fn only_whole_json_is_treated_as_json() {
        assert_eq!(as_json(r#"  {"a":1}  "#).map(|v| v.len()), Some(1));
        assert_eq!(as_json("{\"a\":1}\n{\"b\":2}").map(|v| v.len()), Some(2));
        assert!(as_json("here is {\"a\":1}").is_none());
        assert!(as_json("{not json").is_none());
        assert!(as_json("{\"a\":1}\nthen some words").is_none());
    }

    #[test]
    fn json_cut_short_by_an_ellipsis_is_closed_back_up() {
        let v = as_json("{\"query\": \"select:a,b,cre\u{2026}").expect("open string");
        assert_eq!(v[0]["query"], "select:a,b,cre");
        let v = as_json("{\"a\": 1, \"b\": [1, 2\u{2026}").expect("open array");
        assert_eq!(v[0]["b"], serde_json::json!([1, 2]));
        let v = as_json("{\"a\": 1, \"b\"\u{2026}").expect("cut member dropped");
        assert_eq!(v[0], serde_json::json!({ "a": 1 }));
        assert!(as_json("{\"a\": tr\u{2026}").is_none());
    }

    #[test]
    fn the_parse_cache_follows_the_content() {
        let ctx = egui::Context::default();
        let a = cached_json(&ctx, "{\"a\":1}").expect("parsed");
        let again = cached_json(&ctx, "{\"a\":1}").expect("cached");
        assert!(Arc::ptr_eq(&a, &again));
        let grown = cached_json(&ctx, "{\"a\":1,\"b\":2}").expect("reparsed");
        assert_eq!(grown[0]["b"], 2);
        assert!(cached_json(&ctx, "prose").is_none());
    }

    #[test]
    fn capping_counts_the_characters_left_out() {
        assert_eq!(capped("abcdef", 4), ("abcd", 2));
        assert_eq!(capped("ab", 4), ("ab", 0));
    }
}
