//! Folding consecutive work items into groups and summarising them.

use std::ops::Range;

use super::entry::{EntryKind, WorkItem, WorkKind, WorkStatus};
use crate::chat::types::{UnixMs, format_duration};

/// A transcript row after folding: one entry, or a run of work entries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Run {
    Single(usize),
    Work(Range<usize>),
}

/// Folds consecutive work entries of `kinds` into [`Run::Work`] ranges.
pub fn runs(kinds: impl IntoIterator<Item = EntryKind>) -> Vec<Run> {
    let mut out = Vec::new();
    let mut work_start = None;
    let mut len = 0;
    for (i, kind) in kinds.into_iter().enumerate() {
        len = i + 1;
        match (kind, work_start) {
            (EntryKind::Work(_), None) => work_start = Some(i),
            (EntryKind::Work(_), Some(_)) => {}
            (_, start) => {
                if let Some(start) = start {
                    out.push(Run::Work(start..i));
                    work_start = None;
                }
                out.push(Run::Single(i));
            }
        }
    }
    if let Some(start) = work_start {
        out.push(Run::Work(start..len));
    }
    out
}

/// Totals of one work group.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorkStats {
    pub elapsed_ms: u64,
    /// Items other than reasoning.
    pub calls: u32,
    pub failed: u32,
    pub files: u32,
    pub running: bool,
    /// Every item is reasoning.
    pub thinking_only: bool,
}

impl WorkStats {
    /// Totals of `items`; running items count up to `now`.
    pub fn of<'b, 'a: 'b>(
        items: impl IntoIterator<Item = &'b WorkItem<'a>>,
        now: Option<UnixMs>,
    ) -> Self {
        let mut stats = Self {
            thinking_only: true,
            ..Self::default()
        };
        let (mut first, mut last, mut summed) = (None::<UnixMs>, None::<UnixMs>, 0u64);
        let mut any = false;
        for item in items {
            any = true;
            let running = item.status == WorkStatus::Running;
            stats.running |= running;
            if item.kind != WorkKind::Reasoning {
                stats.thinking_only = false;
                stats.calls += 1;
            }
            if item.status == WorkStatus::Failed {
                stats.failed += 1;
            }
            if item.kind == WorkKind::FileChange {
                stats.files += u32::try_from(item.files.len()).unwrap_or(u32::MAX);
            }
            summed += item.duration_ms.unwrap_or(0);
            if let Some(start) = item.started {
                let end = match (item.duration_ms, running, now) {
                    (_, true, Some(now)) => now,
                    (Some(d), _, _) => UnixMs(start.0.saturating_add(d as i64)),
                    _ => start,
                };
                first = Some(first.map_or(start, |f| f.min(start)));
                last = Some(last.map_or(end, |l| l.max(end)));
            }
        }
        stats.thinking_only &= any;
        stats.elapsed_ms = match (first, last) {
            (Some(first), Some(last)) => last.since(first),
            _ => summed,
        };
        stats
    }
}

/// `Worked 42 s · 6 tool calls · 1 failed · 2 files`, `Working 12 s · 3 tool calls` or `Thought 8 s`.
pub fn work_summary(s: &WorkStats) -> String {
    let verb = match (s.thinking_only, s.running) {
        (true, true) => "Thinking",
        (true, false) => "Thought",
        (false, true) => "Working",
        (false, false) => "Worked",
    };
    let mut out = verb.to_owned();
    if s.elapsed_ms > 0 {
        out.push(' ');
        out.push_str(&format_duration(s.elapsed_ms));
    }
    let plural =
        |n: u32, one: &str, many: &str| format!(" · {n} {}", if n == 1 { one } else { many });
    if !s.thinking_only {
        out.push_str(&plural(s.calls, "tool call", "tool calls"));
    }
    if s.failed > 0 {
        out.push_str(&format!(" · {} failed", s.failed));
    }
    if s.files > 0 {
        out.push_str(&plural(s.files, "file", "files"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::types::CowList;
    use std::borrow::Cow;

    fn item(kind: WorkKind, status: WorkStatus, start: i64, ms: Option<u64>) -> WorkItem<'static> {
        WorkItem {
            key: Cow::Borrowed("k"),
            kind,
            name: Cow::Borrowed("tool"),
            summary: Cow::Borrowed(""),
            status,
            started: Some(UnixMs(start)),
            duration_ms: ms,
            arguments: None,
            result: None,
            body: None,
            files: CowList::default(),
        }
    }

    #[test]
    fn consecutive_work_folds_into_one_run() {
        let w = EntryKind::Work(WorkKind::Tool);
        let kinds = [EntryKind::User, w, w, EntryKind::Agent, w];
        assert_eq!(
            runs(kinds),
            vec![
                Run::Single(0),
                Run::Work(1..3),
                Run::Single(3),
                Run::Work(4..5)
            ]
        );
        assert!(runs([]).is_empty());
    }

    #[test]
    fn summaries_count_calls_failures_and_files() {
        let mut edit = item(WorkKind::FileChange, WorkStatus::Ok, 30_000, Some(2_000));
        edit.files = CowList::Owned(vec![Cow::Borrowed("a.rs"), Cow::Borrowed("b.rs")]);
        let items = [
            item(WorkKind::Reasoning, WorkStatus::Ok, 0, Some(5_000)),
            item(WorkKind::Tool, WorkStatus::Failed, 5_000, Some(10_000)),
            edit,
        ];
        let stats = WorkStats::of(&items, None);
        assert_eq!(stats.elapsed_ms, 32_000);
        assert_eq!(
            work_summary(&stats),
            "Worked 32 s · 2 tool calls · 1 failed · 2 files"
        );
    }

    #[test]
    fn running_and_thinking_groups_read_in_the_present() {
        let running = [item(WorkKind::Tool, WorkStatus::Running, 0, None)];
        let stats = WorkStats::of(&running, Some(UnixMs(12_000)));
        assert_eq!(work_summary(&stats), "Working 12 s · 1 tool call");
        let thought = [item(WorkKind::Reasoning, WorkStatus::Ok, 0, Some(8_000))];
        assert_eq!(work_summary(&WorkStats::of(&thought, None)), "Thought 8 s");
    }
}
