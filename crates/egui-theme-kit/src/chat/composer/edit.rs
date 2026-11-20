//! Keeps mention chips in step with buffer edits: edit detection, whole-chip deletion, wire paste, undo restore and cursor snapping.

use std::ops::Range;

use super::state::MentionSpan;
use crate::chat::mention::{MentionRef, WireSegment, clean_label, encode, segments};

/// Most deleted chips kept for undo restore.
const GRAVEYARD_MAX: usize = 32;

/// Old chars `old` replaced by `inserted` new chars starting at `old.start`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Edit {
    pub old: Range<usize>,
    pub inserted: usize,
}

/// The buffer, chips and cursor after an edit was reconciled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Outcome {
    pub text: String,
    pub spans: Vec<MentionSpan>,
    /// Cursor to store; `None` keeps the text edit's.
    pub cursor: Option<usize>,
    /// The text differs from what the text edit produced.
    pub rewritten: bool,
}

/// The text a chip for `target` shows.
pub(super) fn chip_text(target: &MentionRef) -> String {
    format!("@{}", clean_label(&target.label))
}

/// The edit turning `old` into `new`, anchored on the selection before and the cursor after, else by common prefix and suffix.
pub(super) fn find_edit(
    old: &[char],
    new: &[char],
    before: Option<Range<usize>>,
    after: Option<usize>,
) -> Option<Edit> {
    if old == new {
        return None;
    }
    let anchored = before
        .zip(after)
        .and_then(|(sel, cursor)| anchored_edit(old, new, sel, cursor));
    Some(anchored.unwrap_or_else(|| diff_edit(old, new)))
}

fn anchored_edit(old: &[char], new: &[char], sel: Range<usize>, cursor: usize) -> Option<Edit> {
    if cursor > new.len() {
        return None;
    }
    let lo = sel.start.min(old.len()).min(cursor);
    let hi = old.len().checked_sub(new.len() - cursor)?;
    if hi < lo || old[..lo] != new[..lo] || old[hi..] != new[cursor..] {
        return None;
    }
    let mut edit = Edit {
        old: lo..hi,
        inserted: cursor - lo,
    };
    trim(&mut edit, old, new);
    Some(edit)
}

/// Drops chars both sides of `edit` share at its ends.
fn trim(edit: &mut Edit, old: &[char], new: &[char]) {
    while !edit.old.is_empty() && edit.inserted > 0 && old[edit.old.start] == new[edit.old.start] {
        edit.old.start += 1;
        edit.inserted -= 1;
    }
    while !edit.old.is_empty()
        && edit.inserted > 0
        && old[edit.old.end - 1] == new[edit.old.start + edit.inserted - 1]
    {
        edit.old.end -= 1;
        edit.inserted -= 1;
    }
}

fn diff_edit(old: &[char], new: &[char]) -> Edit {
    let shorter = old.len().min(new.len());
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let suffix = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take(shorter - prefix)
        .take_while(|(a, b)| a == b)
        .count();
    Edit {
        old: prefix..old.len() - suffix,
        inserted: new.len() - suffix - prefix,
    }
}

/// Char ranges of `spans` after `old` became `new`; `None` for chips the edit touches.
pub(super) fn follow_edit(
    old: &str,
    new: &str,
    spans: &[MentionSpan],
) -> Vec<Option<Range<usize>>> {
    let old_c: Vec<char> = old.chars().collect();
    let new_c: Vec<char> = new.chars().collect();
    let Some(edit) = find_edit(&old_c, &new_c, None, None) else {
        return spans.iter().map(|s| Some(s.chars.clone())).collect();
    };
    spans
        .iter()
        .map(|s| {
            if touches(&edit, &s.chars) {
                None
            } else if s.chars.start >= edit.old.end {
                let shift = |c: usize| c - edit.old.len() + edit.inserted;
                Some(shift(s.chars.start)..shift(s.chars.end))
            } else {
                Some(s.chars.clone())
            }
        })
        .collect()
}

/// Whether `edit` changes chars of `span`; an insertion only counts strictly inside.
fn touches(edit: &Edit, span: &Range<usize>) -> bool {
    if edit.old.is_empty() {
        span.start < edit.old.start && edit.old.start < span.end
    } else {
        edit.old.start < span.end && edit.old.end > span.start
    }
}

/// `text` as display text and chips with offsets relative to it.
pub(super) fn from_wire(text: &str) -> (String, Vec<MentionSpan>) {
    let mut display = String::with_capacity(text.len());
    let mut spans = Vec::new();
    let mut len = 0;
    for segment in segments(text) {
        match segment {
            WireSegment::Text(t) => {
                display.push_str(t);
                len += t.chars().count();
            }
            WireSegment::Mention(target) => {
                let chip = chip_text(&target);
                let n = chip.chars().count();
                display.push_str(&chip);
                spans.push(MentionSpan {
                    chars: len..len + n,
                    target,
                });
                len += n;
            }
        }
    }
    (display, spans)
}

/// Adds `dead` to `graveyard`, dropping the oldest past the cap.
pub(super) fn bury(graveyard: &mut Vec<MentionSpan>, dead: impl IntoIterator<Item = MentionSpan>) {
    graveyard.extend(dead);
    let excess = graveyard.len().saturating_sub(GRAVEYARD_MAX);
    graveyard.drain(..excess);
}

/// Reconciles chips after `old` became `new`: touched chips go whole, pasted wire becomes chips, undo restores buried chips.
pub(super) fn reconcile(
    old: &str,
    new: &str,
    spans: &[MentionSpan],
    graveyard: &mut Vec<MentionSpan>,
    before: Option<Range<usize>>,
    after: Option<usize>,
    undo: bool,
) -> Outcome {
    let old_c: Vec<char> = old.chars().collect();
    let new_c: Vec<char> = new.chars().collect();
    let Some(edit) = find_edit(&old_c, &new_c, before, after) else {
        return Outcome {
            text: new.to_owned(),
            spans: spans.to_vec(),
            cursor: None,
            rewritten: false,
        };
    };
    let inserted: String = new_c[edit.old.start..edit.old.start + edit.inserted]
        .iter()
        .collect();
    let (display, mut fresh) = from_wire(&inserted);
    let ins_len = display.chars().count();

    let (touched, kept): (Vec<_>, Vec<_>) = spans
        .iter()
        .cloned()
        .partition(|s| touches(&edit, &s.chars));
    let start = touched
        .iter()
        .map(|s| s.chars.start)
        .fold(edit.old.start, usize::min);
    let end = touched
        .iter()
        .map(|s| s.chars.end)
        .fold(edit.old.end, usize::max);
    bury(graveyard, touched);

    let mut text: String = old_c[..start].iter().collect();
    text.push_str(&display);
    text.extend(&old_c[end..]);

    if undo {
        let text_c: Vec<char> = text.chars().collect();
        let region = start..start + ins_len;
        let mut i = graveyard.len();
        while i > 0 {
            i -= 1;
            let g = &graveyard[i];
            let fits = g.chars.end <= text_c.len()
                && g.chars.start < region.end
                && g.chars.end > region.start
                && g.chars.start >= start
                && !fresh.iter().any(|f| {
                    f.chars.start + start < g.chars.end && g.chars.start < f.chars.end + start
                });
            let matches = fits
                && text_c[g.chars.clone()]
                    .iter()
                    .copied()
                    .eq(chip_text(&g.target).chars());
            if matches {
                let g = graveyard.remove(i);
                fresh.push(MentionSpan {
                    chars: g.chars.start - start..g.chars.end - start,
                    target: g.target,
                });
            }
        }
    }

    let removed = end - start;
    let mut out: Vec<MentionSpan> = kept
        .into_iter()
        .map(|mut s| {
            if s.chars.start >= end {
                s.chars = s.chars.start - removed + ins_len..s.chars.end - removed + ins_len;
            }
            s
        })
        .collect();
    out.extend(fresh.into_iter().map(|mut s| {
        s.chars = s.chars.start + start..s.chars.end + start;
        s
    }));
    out.sort_by_key(|s| s.chars.start);

    let rewritten = text != new;
    Outcome {
        cursor: rewritten.then_some(start + ins_len),
        text,
        spans: out,
        rewritten,
    }
}

/// Replaces chars `range` of `text` with `with`, shifting chips after it; chips inside are dropped.
pub(super) fn splice(
    text: &mut String,
    spans: &mut Vec<MentionSpan>,
    range: Range<usize>,
    with: &str,
) -> Vec<MentionSpan> {
    let (a, b) = (byte_at(text, range.start), byte_at(text, range.end));
    text.replace_range(a..b, with);
    let n = with.chars().count();
    let removed = range.len();
    let (dead, live): (Vec<_>, Vec<_>) = spans
        .drain(..)
        .partition(|s| s.chars.start < range.end && s.chars.end > range.start);
    *spans = live
        .into_iter()
        .map(|mut s| {
            if s.chars.start >= range.end {
                s.chars = s.chars.start - removed + n..s.chars.end - removed + n;
            }
            s
        })
        .collect();
    dead
}

/// Inserts a chip for `target` plus a trailing space over chars `range`; returns the cursor after the space.
pub(super) fn insert_chip(
    text: &mut String,
    spans: &mut Vec<MentionSpan>,
    range: Range<usize>,
    target: MentionRef,
) -> usize {
    let chip = chip_text(&target);
    let n = chip.chars().count();
    splice(text, spans, range.clone(), &format!("{chip} "));
    spans.push(MentionSpan {
        chars: range.start..range.start + n,
        target,
    });
    spans.sort_by_key(|s| s.chars.start);
    range.start + n + 1
}

/// Rewrites chip `index` to show `label`; returns its old char range and new char length.
pub(super) fn relabel(
    text: &mut String,
    spans: &mut [MentionSpan],
    index: usize,
    label: &str,
) -> (Range<usize>, usize) {
    let old = spans[index].chars.clone();
    spans[index].target.label = clean_label(label);
    let chip = chip_text(&spans[index].target);
    let n = chip.chars().count();
    let (a, b) = (byte_at(text, old.start), byte_at(text, old.end));
    text.replace_range(a..b, &chip);
    spans[index].chars = old.start..old.start + n;
    for s in spans.iter_mut().filter(|s| s.chars.start >= old.end) {
        s.chars = s.chars.start - old.len() + n..s.chars.end - old.len() + n;
    }
    (old, n)
}

/// Cursor `c` after chars `old` became `new_len` chars; inside goes to the new end.
pub(super) fn map_cursor(c: usize, old: &Range<usize>, new_len: usize) -> usize {
    if c <= old.start {
        c
    } else if c >= old.end {
        c - old.len() + new_len
    } else {
        old.start + new_len
    }
}

/// `head` moved out of any chip: past it when entering from before, back when from after, else to the nearer edge.
pub(super) fn snap(spans: &[MentionSpan], prev: Option<usize>, head: usize) -> usize {
    let Some(s) = spans
        .iter()
        .find(|s| s.chars.start < head && head < s.chars.end)
    else {
        return head;
    };
    match prev {
        Some(p) if p <= s.chars.start => s.chars.end,
        Some(p) if p >= s.chars.end => s.chars.start,
        _ if head - s.chars.start < s.chars.end - head => s.chars.start,
        _ => s.chars.end,
    }
}

/// Selection end `anchor` moved to the chip edge away from `head`.
pub(super) fn snap_outward(spans: &[MentionSpan], anchor: usize, head: usize) -> usize {
    match spans
        .iter()
        .find(|s| s.chars.start < anchor && anchor < s.chars.end)
    {
        Some(s) if anchor < head => s.chars.start,
        Some(s) => s.chars.end,
        None => anchor,
    }
}

/// Chars `range` of `text` in wire form, or `None` when no whole chip lies inside.
pub(super) fn wire_slice(text: &str, spans: &[MentionSpan], range: Range<usize>) -> Option<String> {
    let inside: Vec<&MentionSpan> = spans
        .iter()
        .filter(|s| s.chars.start >= range.start && s.chars.end <= range.end)
        .collect();
    if inside.is_empty() {
        return None;
    }
    let chars: Vec<char> = text.chars().collect();
    let end = range.end.min(chars.len());
    let mut out = String::new();
    let mut at = range.start.min(end);
    for s in inside {
        out.extend(&chars[at..s.chars.start]);
        out.push_str(&encode(&s.target));
        at = s.chars.end;
    }
    out.extend(&chars[at..end]);
    Some(out)
}

/// Byte offset of char index `chars` in `text`.
pub(super) fn byte_at(text: &str, chars: usize) -> usize {
    text.char_indices()
        .nth(chars)
        .map_or(text.len(), |(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(id: &str, label: &str) -> MentionRef {
        MentionRef::new("session", id, label)
    }

    /// `prefix`, a chip for `t`, then `suffix`, with the chip's span.
    fn with_chip(prefix: &str, t: &MentionRef, suffix: &str) -> (String, Vec<MentionSpan>) {
        let chip = chip_text(t);
        let start = prefix.chars().count();
        let span = MentionSpan {
            chars: start..start + chip.chars().count(),
            target: t.clone(),
        };
        (format!("{prefix}{chip}{suffix}"), vec![span])
    }

    fn label_at(text: &str, span: &MentionSpan) -> String {
        text.chars()
            .skip(span.chars.start)
            .take(span.chars.len())
            .collect()
    }

    #[test]
    fn typing_at_before_a_chip_shifts_it() {
        let t = target("1", "label");
        let (old, spans) = with_chip("x ", &t, " y");
        let new = "x @@label y";
        let mut grave = Vec::new();
        let out = reconcile(&old, new, &spans, &mut grave, Some(2..2), Some(3), false);
        assert!(!out.rewritten);
        assert_eq!(out.spans[0].chars, 3..9);
        assert_eq!(label_at(&out.text, &out.spans[0]), "@label");
    }

    #[test]
    fn prefix_diff_alone_would_place_that_edit_inside_the_chip() {
        let old: Vec<char> = "x @label y".chars().collect();
        let new: Vec<char> = "x @@label y".chars().collect();
        assert_eq!(diff_edit(&old, &new).old, 3..3);
        let anchored = find_edit(&old, &new, Some(2..2), Some(3)).unwrap();
        assert_eq!(anchored.old, 2..2);
    }

    #[test]
    fn backspace_at_a_chip_end_deletes_the_whole_chip() {
        let t = target("1", "Fix the scroll");
        let (old, spans) = with_chip("hi ", &t, " ok");
        let end = spans[0].chars.end;
        let mut new: Vec<char> = old.chars().collect();
        new.remove(end - 1);
        let new: String = new.into_iter().collect();
        let mut grave = Vec::new();
        let out = reconcile(
            &old,
            &new,
            &spans,
            &mut grave,
            Some(end..end),
            Some(end - 1),
            false,
        );
        assert_eq!(out.text, "hi  ok");
        assert!(out.spans.is_empty());
        assert!(out.rewritten);
        assert_eq!(out.cursor, Some(3));
        assert_eq!(grave.len(), 1);
    }

    #[test]
    fn delete_at_a_chip_start_deletes_the_whole_chip() {
        let t = target("1", "abc");
        let (old, spans) = with_chip("a ", &t, "");
        let mut grave = Vec::new();
        let out = reconcile(
            &old,
            "a abc",
            &spans,
            &mut grave,
            Some(2..2),
            Some(2),
            false,
        );
        assert_eq!(out.text, "a ");
        assert!(out.spans.is_empty());
    }

    #[test]
    fn deleting_a_selection_across_a_chip_removes_the_rest_of_it() {
        let t = target("1", "Fix the scroll");
        let (old, spans) = with_chip("ab ", &t, " cd");
        let mut grave = Vec::new();
        let out = reconcile(
            &old,
            "ab Fix the scroll cd",
            &spans,
            &mut grave,
            None,
            None,
            false,
        );
        assert_eq!(out.text, "ab  cd");
        assert!(out.spans.is_empty());
    }

    #[test]
    fn an_ime_replacing_a_word_inside_a_chip_removes_the_chip_text() {
        let t = target("1", "fix it");
        let (old, spans) = with_chip("", &t, " now");
        let new = "@fix It now";
        let mut grave = Vec::new();
        let out = reconcile(&old, new, &spans, &mut grave, Some(7..7), Some(7), false);
        assert_eq!(out.text, "I now");
        assert!(out.spans.is_empty());
        assert_eq!(out.cursor, Some(1));
    }

    #[test]
    fn an_ime_replacing_a_plain_word_leaves_chips_alone() {
        let t = target("1", "fix");
        let (old, spans) = with_chip("", &t, " teh");
        let new = "@fix the";
        let mut grave = Vec::new();
        let out = reconcile(&old, new, &spans, &mut grave, Some(8..8), Some(8), false);
        assert!(!out.rewritten);
        assert_eq!(out.spans, spans);
    }

    #[test]
    fn edits_after_a_chip_leave_it_and_edits_before_shift_it() {
        let t = target("1", "a");
        let (old, spans) = with_chip("xy ", &t, " z");
        let mut grave = Vec::new();
        let after = reconcile(
            &old,
            "xy @a zq",
            &spans,
            &mut grave,
            Some(7..7),
            Some(8),
            false,
        );
        assert_eq!(after.spans[0].chars, 3..5);
        let before = reconcile(
            &old,
            "x @a z",
            &spans,
            &mut grave,
            Some(2..2),
            Some(1),
            false,
        );
        assert_eq!(before.spans[0].chars, 2..4);
        assert!(grave.is_empty());
    }

    #[test]
    fn pasted_wire_text_becomes_chips() {
        let t = target("desk/t1", "Fix scroll");
        let pasted = format!("see {} now", encode(&t));
        let new = format!("ab{pasted}");
        let mut grave = Vec::new();
        let out = reconcile(
            "ab",
            &new,
            &[],
            &mut grave,
            Some(2..2),
            Some(new.chars().count()),
            false,
        );
        assert_eq!(out.text, "absee @Fix scroll now");
        assert_eq!(out.spans.len(), 1);
        assert_eq!(out.spans[0].target, t);
        assert_eq!(label_at(&out.text, &out.spans[0]), "@Fix scroll");
        assert_eq!(out.cursor, Some(out.text.chars().count()));
    }

    #[test]
    fn undo_restores_a_deleted_chip_but_retyping_does_not() {
        let t = target("1", "abc");
        let (old, spans) = with_chip("a ", &t, " b");
        let mut grave = Vec::new();
        let cut = reconcile(
            &old,
            "a @ab b",
            &spans,
            &mut grave,
            Some(6..6),
            Some(5),
            false,
        );
        assert_eq!(cut.text, "a  b");
        assert_eq!(grave.len(), 1);

        let retyped = reconcile(
            &cut.text,
            &old,
            &cut.spans,
            &mut grave,
            Some(2..2),
            Some(6),
            false,
        );
        assert!(retyped.spans.is_empty());
        assert_eq!(grave.len(), 1);

        let undone = reconcile(
            &cut.text,
            &old,
            &cut.spans,
            &mut grave,
            Some(2..2),
            Some(6),
            true,
        );
        assert_eq!(undone.spans, spans);
        assert!(grave.is_empty());
    }

    #[test]
    fn undo_restores_the_newest_of_two_chips_with_one_label() {
        let (a, b) = (target("A", "Same"), target("B", "Same"));
        let (old, spans_a) = with_chip("", &a, "");
        let mut grave = Vec::new();
        let cut = reconcile(
            &old,
            "@Sam",
            &spans_a,
            &mut grave,
            Some(5..5),
            Some(4),
            false,
        );
        assert_eq!(cut.text, "");
        let (_, spans_b) = with_chip("", &b, "");
        let cut = reconcile(
            &old,
            "@Sam",
            &spans_b,
            &mut grave,
            Some(5..5),
            Some(4),
            false,
        );
        assert_eq!(cut.text, "");
        assert_eq!(grave.len(), 2);
        let undone = reconcile(&cut.text, &old, &[], &mut grave, Some(0..0), Some(5), true);
        assert_eq!(undone.spans.len(), 1);
        assert_eq!(undone.spans[0].target.id, "B");
        assert_eq!(grave.len(), 1);
        assert_eq!(grave[0].target.id, "A");
    }

    #[test]
    fn chips_follow_an_edit_before_them_and_drop_when_touched() {
        let t = target("1", "Same");
        let (old, spans) = with_chip("ab ", &t, " z");
        assert_eq!(follow_edit(&old, &old, &spans), vec![Some(3..8)]);
        assert_eq!(
            follow_edit(&old, &format!("x{old}"), &spans),
            vec![Some(4..9)]
        );
        assert_eq!(follow_edit(&old, "ab @Sme z", &spans), vec![None]);
        assert_eq!(
            follow_edit(&old, &format!("{old}!"), &spans),
            vec![Some(3..8)]
        );
    }

    #[test]
    fn the_graveyard_is_capped() {
        let mut grave = Vec::new();
        let t = target("1", "a");
        bury(
            &mut grave,
            (0..GRAVEYARD_MAX + 5).map(|i| MentionSpan {
                chars: i..i + 1,
                target: t.clone(),
            }),
        );
        assert_eq!(grave.len(), GRAVEYARD_MAX);
        assert_eq!(grave[0].chars.start, 5);
    }

    #[test]
    fn cursors_snap_out_of_chips_by_direction() {
        let t = target("1", "abcd");
        let (_, spans) = with_chip("x ", &t, " y");
        assert_eq!(snap(&spans, Some(2), 3), 7);
        assert_eq!(snap(&spans, Some(7), 6), 2);
        assert_eq!(snap(&spans, None, 3), 2);
        assert_eq!(snap(&spans, None, 6), 7);
        assert_eq!(snap(&spans, None, 2), 2);
        assert_eq!(snap_outward(&spans, 4, 0), 7);
        assert_eq!(snap_outward(&spans, 4, 9), 2);
    }

    #[test]
    fn copies_covering_a_chip_use_the_wire_form() {
        let t = target("1", "ab");
        let (text, spans) = with_chip("x ", &t, " y");
        assert_eq!(
            wire_slice(&text, &spans, 0..text.chars().count()),
            Some(format!("x {} y", encode(&t)))
        );
        assert_eq!(wire_slice(&text, &spans, 0..2), None);
        assert_eq!(wire_slice(&text, &spans, 0..3), None);
    }

    #[test]
    fn relabelling_shifts_later_chips_and_the_cursor() {
        let (a, b) = (target("1", "old"), target("2", "b"));
        let mut text = String::new();
        let mut spans = Vec::new();
        let c1 = insert_chip(&mut text, &mut spans, 0..0, a);
        let len = text.chars().count();
        let c2 = insert_chip(&mut text, &mut spans, len..len, b);
        assert_eq!((c1, c2), (5, 8));
        let (old, n) = relabel(&mut text, &mut spans, 0, "renamed\nlabel");
        assert_eq!(text, "@renamed label @b ");
        assert_eq!(spans[1].chars, 15..17);
        assert_eq!(map_cursor(8, &old, n), 18);
        assert_eq!(map_cursor(2, &old, n), 14);
        assert_eq!(map_cursor(0, &old, n), 0);
    }

    #[test]
    fn splicing_over_a_chip_drops_it() {
        let t = target("1", "ab");
        let (mut text, mut spans) = with_chip("x ", &t, " y");
        let dead = splice(&mut text, &mut spans, 1..4, "Q");
        assert_eq!(text, "xQb y");
        assert_eq!(dead.len(), 1);
        assert!(spans.is_empty());
    }
}
