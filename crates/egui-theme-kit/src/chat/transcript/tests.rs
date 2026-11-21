//! Frame tests of the transcript: every entry kind, virtualisation, following, jumps and anchoring.

use std::borrow::Cow;

use egui::{Context, Pos2, RawInput, Rect, vec2};

use super::*;
use crate::chat::mention::{MentionRef, encode};
use crate::chat::types::{CardAction, CowList, Tone};
use crate::kit::ButtonKind;

fn input() -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
        ..RawInput::default()
    }
}

fn user(key: &'static str, text: String) -> ChatEntry<'static> {
    ChatEntry::User(UserMessage {
        key: Cow::Borrowed(key),
        text: Cow::Owned(text),
        author: None,
        time: Some(UnixMs(1_763_647_500_000)),
        attachments: CowList::default(),
        delivery: Delivery::Sent,
    })
}

fn agent(key: String, text: impl Into<Cow<'static, str>>) -> ChatEntry<'static> {
    ChatEntry::Agent(AgentMessage {
        key: Cow::Owned(key),
        text: text.into(),
        author: Some(Cow::Borrowed("Agent")),
        time: None,
        streaming: false,
    })
}

fn work(key: &'static str, kind: WorkKind, status: WorkStatus) -> ChatEntry<'static> {
    ChatEntry::Work(WorkItem {
        key: Cow::Borrowed(key),
        kind,
        name: Cow::Borrowed("query"),
        summary: Cow::Borrowed("ls -la /tmp"),
        status,
        started: Some(UnixMs(0)),
        duration_ms: Some(1_500),
        arguments: Some(Payload::Text(Cow::Borrowed("{\"command\": \"ls -la\"}"))),
        result: Some(Payload::Text(Cow::Borrowed("plain output"))),
        body: Some(Cow::Borrowed("--- a\n+++ b\n+added\n-removed")),
        files: CowList::Owned(vec![Cow::Borrowed("src/lib.rs")]),
    })
}

static CHOICES: [CardChoice<'static>; 2] = [
    CardChoice {
        label: Cow::Borrowed("Approve"),
        kind: ButtonKind::Primary,
        shortcut: Some(Cow::Borrowed("Y")),
        while_locked: false,
    },
    CardChoice {
        label: Cow::Borrowed("Deny"),
        kind: ButtonKind::Danger,
        shortcut: None,
        while_locked: false,
    },
];

/// One entry of every variant, with every work kind and card state.
fn every_kind() -> Vec<ChatEntry<'static>> {
    let mention = encode(&MentionRef::new("session", "desk/t1", "Fix the scroll"));
    let items = vec![
        QuestionItem {
            id: Cow::Borrowed("a"),
            label: Cow::Borrowed("Alpha"),
            detail: Some(Cow::Borrowed("first")),
            recommended: true,
            reason: Some(Cow::Borrowed("safe")),
        },
        QuestionItem {
            id: Cow::Borrowed("b"),
            label: Cow::Borrowed("Beta"),
            detail: None,
            recommended: false,
            reason: None,
        },
    ];
    let question = |id: &'static str, form| Question {
        id: Cow::Borrowed(id),
        prompt: Cow::Borrowed("Which one?"),
        form,
        items: CowList::Owned(items.clone()),
        free_text: true,
        header: Some(Cow::Borrowed("Pick")),
        why: Some(Cow::Borrowed("Needed to continue")),
    };
    vec![
        ChatEntry::Notice(Notice {
            key: Cow::Borrowed("n0"),
            text: Cow::Borrowed("Reconnected \u{2705}"),
            time: Some(UnixMs(0)),
            tone: Tone::Info,
        }),
        user("u0", format!("look at {mention} please")),
        ChatEntry::User(UserMessage {
            key: Cow::Borrowed("u1"),
            text: Cow::Borrowed("queued with a file"),
            author: Some(Cow::Borrowed("Logan")),
            time: None,
            attachments: CowList::Owned(vec![
                AttachmentChip {
                    reading: true,
                    ..AttachmentChip::new("dump.dmp")
                },
                AttachmentChip {
                    failed: true,
                    ..AttachmentChip::new("trace.png")
                },
            ]),
            delivery: Delivery::Failed,
        }),
        work("w0", WorkKind::Reasoning, WorkStatus::Ok),
        work("w1", WorkKind::Tool, WorkStatus::Failed),
        work("w2", WorkKind::Command, WorkStatus::Running),
        work("w3", WorkKind::FileChange, WorkStatus::Ok),
        work("w4", WorkKind::Other, WorkStatus::Declined),
        agent(
            "a0".into(),
            "# Done\nSome **bold** and `code` with [a link](https://x.y).\n- item\n> quote\n---\n$ ls -la\n```rust\nfn main() {}\n```\n```json\n{\"a\": 1}\n```",
        ),
        ChatEntry::Divider {
            key: Cow::Borrowed("d0"),
            caption: Cow::Borrowed("Earlier"),
        },
        ChatEntry::Approval(ApprovalCard {
            key: Cow::Borrowed("ap0"),
            context: Cow::Borrowed("SN 1 \u{b7} HOST"),
            title: Cow::Borrowed("Run a command"),
            command: Some(Cow::Borrowed("cargo test --all")),
            detail: Some(Payload::Text(Cow::Borrowed("{\"cwd\": \"/src\"}"))),
            reason: Some(Cow::Borrowed("Verifies the fix")),
            choices: CowList::Borrowed(&CHOICES),
            state: CardState::Open,
        }),
        ChatEntry::Approval(ApprovalCard {
            key: Cow::Borrowed("ap1"),
            context: Cow::Borrowed("desk"),
            title: Cow::Borrowed("Locked"),
            command: None,
            detail: None,
            reason: None,
            choices: CowList::Borrowed(&CHOICES),
            state: CardState::Locked {
                reason: Cow::Borrowed("Assigned to someone else"),
            },
        }),
        ChatEntry::Question(QuestionCard {
            key: Cow::Borrowed("q0"),
            context: Cow::Borrowed("desk"),
            header: Some(Cow::Borrowed("A few questions")),
            why: None,
            questions: CowList::Owned(vec![
                question("choice", QuestionForm::Choice),
                question("multi", QuestionForm::Multi),
                question("check", QuestionForm::Checklist),
                question("confirm", QuestionForm::Confirm),
                question("text", QuestionForm::Text),
            ]),
            state: CardState::Busy,
        }),
        ChatEntry::Question(QuestionCard {
            key: Cow::Borrowed("q1"),
            context: Cow::Borrowed("desk"),
            header: None,
            why: None,
            questions: CowList::Owned(vec![question("only", QuestionForm::Confirm)]),
            state: CardState::Failed(Cow::Borrowed("Send failed")),
        }),
        ChatEntry::Plan(PlanCard {
            key: Cow::Borrowed("p0"),
            title: None,
            explanation: Some(Cow::Borrowed("Three steps")),
            steps: CowList::Owned(vec![
                PlanStep {
                    text: Cow::Borrowed("Scan"),
                    status: PlanStatus::Completed,
                },
                PlanStep {
                    text: Cow::Borrowed("Fix"),
                    status: PlanStatus::InProgress,
                },
                PlanStep {
                    text: Cow::Borrowed("Verify"),
                    status: PlanStatus::Pending,
                },
            ]),
            dismiss: Some(Cow::Borrowed("Hide plan")),
            open: None,
        }),
        ChatEntry::Custom {
            key: Cow::Borrowed("c0"),
        },
        ChatEntry::Agent(AgentMessage {
            key: Cow::Borrowed("a1"),
            text: Cow::Borrowed(""),
            author: None,
            time: None,
            streaming: true,
        }),
    ]
}

/// Runs `frames` frames of a transcript over `entries`; returns the last response and its shapes.
fn run_shapes(
    ctx: &Context,
    state: &mut TranscriptState,
    entries: &[ChatEntry<'_>],
    options: TranscriptOptions,
    older: Option<LoadState>,
    frames: usize,
) -> (TranscriptResponse, Vec<egui::epaint::ClippedShape>) {
    let mut last = TranscriptResponse::default();
    let mut shapes = Vec::new();
    for _ in 0..frames {
        let mut out = ctx.run_ui(input(), |ui| {
            let mut t = Transcript::new("chat").options(options);
            if let Some(o) = older {
                t = t.older(o);
            }
            last = t.show(ui, state, entries, |ui, key| {
                ui.label(key);
            });
        });
        out.textures_delta.clear();
        shapes = out.shapes;
    }
    (last, shapes)
}

fn run(
    ctx: &Context,
    state: &mut TranscriptState,
    entries: &[ChatEntry<'_>],
    options: TranscriptOptions,
    older: Option<LoadState>,
    frames: usize,
) -> TranscriptResponse {
    run_shapes(ctx, state, entries, options, older, frames).0
}

/// Screen top of the visible text galley that reads `needle`.
fn text_top(shapes: &[egui::epaint::ClippedShape], needle: &str) -> Option<f32> {
    shapes.iter().find_map(|c| match &c.shape {
        egui::Shape::Text(t)
            if t.galley.text() == needle
                && c.clip_rect
                    .intersects(Rect::from_min_size(t.pos, t.galley.size())) =>
        {
            Some(t.pos.y)
        }
        _ => None,
    })
}

#[test]
fn every_entry_kind_draws_in_both_detail_levels_with_and_without_thinking() {
    let entries = every_kind();
    for detail in [DetailLevel::Simple, DetailLevel::Detailed] {
        for show_thinking in [false, true] {
            let ctx = Context::default();
            let mut state = TranscriptState::default();
            let options = TranscriptOptions {
                detail,
                show_thinking,
                max_width: Some(700.0),
                now: Some(UnixMs(4_000)),
                show_user_author: detail == DetailLevel::Detailed,
            };
            let out = run(
                &ctx,
                &mut state,
                &entries,
                options,
                Some(LoadState::Available),
                3,
            );
            assert!(out.actions.is_empty(), "{:?}", out.actions);
            assert_eq!(out.nav.total, 2);
            assert!(!state.heights.is_empty());
            for (key, open) in [("w0", true), ("w0", false)] {
                state.set_group_open(key, Some(open));
                run(
                    &ctx,
                    &mut state,
                    &entries,
                    options,
                    Some(LoadState::Failed),
                    1,
                );
            }
        }
    }
}

#[test]
fn group_auto_open_follows_the_detail_level() {
    let running = [work("r0", WorkKind::Tool, WorkStatus::Running)];
    let stats = WorkStats::of(
        running.iter().filter_map(|e| match e {
            ChatEntry::Work(w) => Some(w),
            _ => None,
        }),
        None,
    );
    assert!(work::auto_open(&stats, DetailLevel::Detailed));
    assert!(!work::auto_open(&stats, DetailLevel::Simple));
}

fn long_history(n: usize) -> Vec<ChatEntry<'static>> {
    (0..n)
        .map(|i| {
            if i % 10 == 0 {
                let mut e = user("u", format!("question {i}"));
                if let ChatEntry::User(m) = &mut e {
                    m.key = Cow::Owned(format!("u{i}"));
                }
                e
            } else {
                agent(format!("m{i}"), format!("reply {i}"))
            }
        })
        .collect()
}

#[test]
fn only_rows_near_the_viewport_are_laid_out() {
    let entries = long_history(400);
    let ctx = Context::default();
    let mut state = TranscriptState::default();
    let (out, shapes) = run_shapes(
        &ctx,
        &mut state,
        &entries,
        TranscriptOptions::default(),
        None,
        4,
    );
    assert!(state.following());
    assert!(out.at_bottom);
    assert!(text_top(&shapes, "reply 399").is_some_and(|y| y < 600.0));
    assert!(text_top(&shapes, "reply 100").is_none());
    assert!(
        state.heights.len() < 120,
        "{} rows measured",
        state.heights.len()
    );
    assert_eq!(out.nav.total, 40);
    assert_eq!(out.nav.current, Some(39));
}

#[test]
fn a_jump_scrolls_to_its_row_stops_following_and_flashes() {
    let entries = long_history(200);
    let ctx = Context::default();
    let mut state = TranscriptState::default();
    let options = TranscriptOptions::default();
    run(&ctx, &mut state, &entries, options, None, 3);
    state.jump(Jump::Key("m55".into()));
    let (_, shapes) = run_shapes(&ctx, &mut state, &entries, options, None, 4);
    assert!(!state.following());
    assert!(state.flash.is_some_and(|(h, _)| h == key_hash("m55")));
    let top = text_top(&shapes, "reply 55").expect("jumped-to row drawn");
    assert!((0.0..60.0).contains(&top), "row top at {top}");
    state.jump(Jump::Bottom);
    let (out, shapes) = run_shapes(&ctx, &mut state, &entries, options, None, 3);
    assert!(state.following() && out.at_bottom);
    assert!(text_top(&shapes, "reply 199").is_some());
}

#[test]
fn prepending_history_keeps_the_anchored_row_in_place() {
    let all = long_history(240);
    let tail = all[100..].to_vec();
    let ctx = Context::default();
    let mut state = TranscriptState::default();
    let options = TranscriptOptions::default();
    run(
        &ctx,
        &mut state,
        &tail,
        options,
        Some(LoadState::Available),
        3,
    );
    state.jump(Jump::Key("m155".into()));
    let (_, shapes) = run_shapes(
        &ctx,
        &mut state,
        &tail,
        options,
        Some(LoadState::Available),
        4,
    );
    let before = text_top(&shapes, "reply 155").expect("anchored row drawn");
    let (_, shapes) = run_shapes(&ctx, &mut state, &all, options, Some(LoadState::Loading), 4);
    let after = text_top(&shapes, "reply 155").expect("anchored row still drawn");
    assert!(
        (before - after).abs() < 2.0,
        "moved from {before} to {after}"
    );
    assert!(!state.following());
}

#[test]
fn a_jump_into_a_work_group_opens_it() {
    let mut entries = long_history(60);
    entries.insert(30, work("w-in", WorkKind::Tool, WorkStatus::Ok));
    entries.insert(30, work("w-first", WorkKind::Tool, WorkStatus::Ok));
    let ctx = Context::default();
    let mut state = TranscriptState::default();
    run(
        &ctx,
        &mut state,
        &entries,
        TranscriptOptions::default(),
        None,
        2,
    );
    assert_eq!(state.group_open("w-first"), None);
    state.jump(Jump::Key("w-in".into()));
    run(
        &ctx,
        &mut state,
        &entries,
        TranscriptOptions::default(),
        None,
        2,
    );
    assert_eq!(state.group_open("w-first"), Some(true));
}

#[test]
fn stepping_through_user_messages_moves_the_nav_position() {
    let entries = long_history(200);
    let ctx = Context::default();
    let mut state = TranscriptState::default();
    let options = TranscriptOptions::default();
    run(&ctx, &mut state, &entries, options, None, 3);
    state.jump(Jump::User(3));
    let (out, shapes) = run_shapes(&ctx, &mut state, &entries, options, None, 4);
    assert_eq!(out.nav.current, Some(3));
    assert!(text_top(&shapes, "question 30").is_some_and(|y| y < 60.0));
    state.jump(Jump::PrevUser);
    let out = run(&ctx, &mut state, &entries, options, None, 4);
    assert_eq!(out.nav.current, Some(2));
    state.jump(Jump::NextUser);
    let out = run(&ctx, &mut state, &entries, options, None, 4);
    assert_eq!(out.nav.current, Some(3));
}

#[test]
fn stick_for_keeps_following_for_its_frames() {
    let entries = long_history(100);
    let ctx = Context::default();
    let mut state = TranscriptState::default();
    run(
        &ctx,
        &mut state,
        &entries,
        TranscriptOptions::default(),
        None,
        2,
    );
    state.jump(Jump::User(0));
    run(
        &ctx,
        &mut state,
        &entries,
        TranscriptOptions::default(),
        None,
        3,
    );
    assert!(!state.following());
    state.stick_for(2);
    run(
        &ctx,
        &mut state,
        &entries,
        TranscriptOptions::default(),
        None,
        1,
    );
    assert!(state.following());
    assert_eq!(state.stick_frames, 1);
    let out = run(
        &ctx,
        &mut state,
        &entries,
        TranscriptOptions::default(),
        None,
        3,
    );
    assert!(out.at_bottom);
    assert_eq!(state.stick_frames, 0);
}

#[test]
fn heights_rescale_with_width_until_measured_again() {
    let h = Height {
        sig: 1,
        h: 100.0,
        width: 400.0,
    };
    assert_eq!(h.guess(1, 800.0), 100.0);
    assert_eq!(h.guess(2, 800.0), 50.0);
    assert_eq!(h.guess(2, 100.0), 200.0);
}

#[test]
fn a_press_during_a_fling_is_not_a_tap() {
    assert!(suppress_taps(false, true, true, false, true));
    assert!(suppress_taps(true, false, true, false, false));
    assert!(suppress_taps(true, false, false, true, false));
    assert!(!suppress_taps(true, false, false, false, false));
    assert!(!suppress_taps(false, true, true, false, false));
}

#[test]
fn the_user_author_shows_without_hover_when_asked() {
    let mut entry = user("u-a", "hello".to_owned());
    if let ChatEntry::User(m) = &mut entry {
        m.author = Some(Cow::Borrowed("Logan"));
    }
    let entries = vec![entry];
    for (always, shown) in [(false, false), (true, true)] {
        let ctx = Context::default();
        let mut state = TranscriptState::default();
        let options = TranscriptOptions {
            show_user_author: always,
            ..TranscriptOptions::default()
        };
        let (_, shapes) = run_shapes(&ctx, &mut state, &entries, options, None, 2);
        assert_eq!(
            text_top(&shapes, "Logan").is_some(),
            shown,
            "always {always}"
        );
    }
}

#[test]
fn an_over_wide_row_does_not_widen_later_rows() {
    let entries = vec![
        ChatEntry::Custom {
            key: Cow::Borrowed("before"),
        },
        ChatEntry::Custom {
            key: Cow::Borrowed("wide"),
        },
        ChatEntry::Custom {
            key: Cow::Borrowed("after"),
        },
    ];
    let ctx = Context::default();
    let mut state = TranscriptState::default();
    let mut widths = std::collections::HashMap::new();
    for _ in 0..3 {
        let mut out = ctx.run_ui(input(), |ui| {
            Transcript::new("wide").show(ui, &mut state, &entries, |ui, key| {
                widths.insert(key.to_owned(), ui.max_rect().width());
                if key == "wide" {
                    ui.allocate_exact_size(vec2(3000.0, 10.0), egui::Sense::hover());
                }
            });
        });
        out.textures_delta.clear();
    }
    assert!(widths["before"] <= 800.0, "{widths:?}");
    assert_eq!(widths["before"], widths["after"], "{widths:?}");
}

#[test]
fn a_work_group_keeps_its_choice_and_anchor_when_history_is_prepended() {
    let mut tail = vec![
        work("w-a", WorkKind::Tool, WorkStatus::Ok),
        work("w-b", WorkKind::Tool, WorkStatus::Ok),
    ];
    tail.extend(long_history(40).into_iter().skip(1));
    let mut all = long_history(20);
    for (i, e) in all.iter_mut().enumerate() {
        match e {
            ChatEntry::User(m) => m.key = Cow::Owned(format!("old-u{i}")),
            ChatEntry::Agent(m) => m.key = Cow::Owned(format!("old-m{i}")),
            _ => {}
        }
    }
    all.push(work("w-p", WorkKind::Tool, WorkStatus::Ok));
    all.extend(tail.iter().cloned());

    let mut state = TranscriptState::default();
    state.set_group_open("w-a", Some(false));
    let ctx = Context::default();
    let options = TranscriptOptions::default();
    run(
        &ctx,
        &mut state,
        &tail,
        options,
        Some(LoadState::Available),
        2,
    );
    state.following = false;
    state.anchor = Some((key_hash("w-a"), 0.0));
    let (_, shapes) = run_shapes(
        &ctx,
        &mut state,
        &tail,
        options,
        Some(LoadState::Available),
        3,
    );
    let before = text_top(&shapes, "reply 1").expect("row after the group drawn");

    let kinds: Vec<EntryKind> = all.iter().map(|e| e.kind()).collect();
    let group = runs(kinds.iter().copied())
        .into_iter()
        .find_map(|r| match r {
            Run::Work(range) => Some(range),
            Run::Single(_) => None,
        })
        .expect("a work group");
    assert_eq!(all[group.start].key(), "w-p");
    assert_eq!(group_choice(&state, all.as_slice(), group), Some(false));

    let (_, shapes) = run_shapes(&ctx, &mut state, &all, options, Some(LoadState::Loading), 3);
    assert!(!state.following());
    assert_eq!(state.anchor.map(|a| a.0), Some(key_hash("w-p")));
    let after = text_top(&shapes, "reply 1").expect("row after the group still drawn");
    let slack = crate::controls::Metrics::get(&ctx).gap.y / 2.0 + 1.0;
    assert!(
        (before - after).abs() < slack,
        "moved from {before} to {after}"
    );
}

#[test]
fn claimed_lines_split_out_of_agent_text_except_inside_fences() {
    let text = "intro\n[media:a.png]\nmiddle\n```\n[media:b.png]\n```\n[media:c.png]";
    let pieces = bubble::split_claimed(text, |l| l.starts_with("[media:"));
    use bubble::Piece::{Line, Text};
    assert_eq!(
        pieces,
        vec![
            Text("intro\n"),
            Line("[media:a.png]"),
            Text("middle\n```\n[media:b.png]\n```\n"),
            Line("[media:c.png]"),
        ]
    );
    assert_eq!(
        bubble::split_claimed("plain", |_| false),
        vec![Text("plain")]
    );
}

/// Draws lines holding a media tag as a "picture" label and records each `(key, line)` it drew.
#[derive(Default)]
struct Media(std::cell::RefCell<Vec<(String, String)>>);

impl AgentLines for Media {
    fn claims(&self, line: &str) -> bool {
        line.contains("[media:")
    }

    fn show(&self, ui: &mut Ui, key: &str, line: &str) {
        ui.label("picture");
        self.0.borrow_mut().push((key.to_owned(), line.to_owned()));
    }
}

/// Claims no line.
struct NoLines;

impl AgentLines for NoLines {
    fn claims(&self, _line: &str) -> bool {
        false
    }

    fn show(&self, _ui: &mut Ui, _key: &str, _line: &str) {}
}

#[test]
fn the_app_draws_claimed_agent_lines_in_place() {
    let media = Media::default();
    let entries = vec![agent("a1".into(), "before\n[media:x.png]\nafter")];
    let ctx = Context::default();
    let mut state = TranscriptState::default();
    let mut shapes = Vec::new();
    for _ in 0..2 {
        let mut out = ctx.run_ui(input(), |ui| {
            Transcript::new("media")
                .agent_lines(&media)
                .show(ui, &mut state, &entries, |_, _| {});
        });
        out.textures_delta.clear();
        shapes = out.shapes;
    }
    let before = text_top(&shapes, "before").expect("text before drawn");
    let picture = text_top(&shapes, "picture").expect("app line drawn");
    let after = text_top(&shapes, "after").expect("text after drawn");
    assert!(
        before < picture && picture < after,
        "{before} {picture} {after}"
    );
    assert!(text_top(&shapes, "[media:x.png]").is_none());
    assert_eq!(
        media.0.borrow()[0],
        ("a1".to_owned(), "[media:x.png]".to_owned())
    );
}

/// A tool call with media in its text result and body, a command with media in its output and a JSON
/// result, and a file change whose diff mentions media.
fn media_work() -> Vec<ChatEntry<'static>> {
    let mut tool = work("w-media", WorkKind::Tool, WorkStatus::Ok);
    if let ChatEntry::Work(w) = &mut tool {
        w.arguments = Some(Payload::Text(Cow::Borrowed("[media:args.png]")));
        w.result = Some(Payload::Text(Cow::Borrowed("before\n[media:x.png]\nafter")));
        w.body = Some(Cow::Borrowed("intro\n[media:y.png]"));
    }
    let mut command = work("w-cmd", WorkKind::Command, WorkStatus::Ok);
    if let ChatEntry::Work(w) = &mut command {
        w.result = Some(Payload::Json(Cow::Owned(
            serde_json::json!({ "path": "[media:z.png]" }),
        )));
        w.body = Some(Cow::Borrowed("out 1\n[media:out.png]\nout 2"));
    }
    let mut diff = work("w-diff", WorkKind::FileChange, WorkStatus::Ok);
    if let ChatEntry::Work(w) = &mut diff {
        w.body = Some(Cow::Borrowed("+[media:diff.png]"));
    }
    vec![tool, command, diff]
}

/// Frames of `entries` on a tall screen with the work group open and, when `opened`, each call opened too.
fn work_frames(
    ctx: &Context,
    entries: &[ChatEntry<'_>],
    lines: Option<&dyn AgentLines>,
    opened: bool,
) -> Vec<egui::epaint::ClippedShape> {
    let mut state = TranscriptState::default();
    state.set_group_open("w-media", Some(true));
    let mut shapes = Vec::new();
    let tall = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 2000.0))),
        ..RawInput::default()
    };
    for _ in 0..3 {
        let mut out = ctx.run_ui(tall.clone(), |ui| {
            let view = ui
                .scope_id()
                .with(("egui_theme_kit.chat.transcript", IdSalt::new("work-media")));
            for e in entries {
                let id = crate::chat::types::item_id(view, e.key(), "call_open");
                ui.data_mut(|d| d.insert_temp(id, opened));
            }
            let mut t = Transcript::new("work-media");
            if let Some(lines) = lines {
                t = t.agent_lines(lines);
            }
            t.show(ui, &mut state, entries, |_, _| {});
        });
        out.textures_delta.clear();
        shapes = out.shapes;
    }
    shapes
}

#[test]
fn opened_work_items_route_claimed_text_lines_to_the_app() {
    let entries = media_work();
    let media = Media::default();
    work_frames(&Context::default(), &entries, Some(&media), false);
    assert!(media.0.borrow().is_empty(), "{:?}", media.0.borrow());

    let shapes = work_frames(&Context::default(), &entries, Some(&media), true);
    let drawn = media.0.borrow().clone();
    for (key, line) in [
        ("w-media", "[media:x.png]"),
        ("w-media", "[media:y.png]"),
        ("w-cmd", "[media:out.png]"),
    ] {
        assert!(
            drawn.contains(&(key.to_owned(), line.to_owned())),
            "{line} not drawn by the app: {drawn:?}"
        );
    }
    for line in ["[media:args.png]", "[media:z.png]", "+[media:diff.png]"] {
        assert!(
            drawn.iter().all(|(_, l)| l != line),
            "{line} offered: {drawn:?}"
        );
    }
    let top = |needle| text_top(&shapes, needle).unwrap_or_else(|| panic!("{needle} drawn"));
    assert!(top("before") < top("after"));
    assert!(top("out 1") < top("out 2"));
    assert!(text_top(&shapes, "[media:x.png]").is_none());
    assert!(text_top(&shapes, "[media:out.png]").is_none());
}

#[test]
fn opened_work_items_draw_unclaimed_text_as_without_lines() {
    let entries = media_work();
    let texts = |shapes: Vec<egui::epaint::ClippedShape>| -> Vec<(String, Pos2)> {
        shapes
            .into_iter()
            .filter_map(|c| match c.shape {
                egui::Shape::Text(t) => Some((t.galley.text().to_owned(), t.pos)),
                _ => None,
            })
            .collect()
    };
    let plain = texts(work_frames(&Context::default(), &entries, None, true));
    let unclaimed = texts(work_frames(
        &Context::default(),
        &entries,
        Some(&NoLines),
        true,
    ));
    assert!(
        plain
            .iter()
            .any(|(t, _)| t == "before\n[media:x.png]\nafter")
    );
    assert_eq!(plain, unclaimed);
}

/// Screen rect of the visible text galley that reads `needle`.
fn text_rect(shapes: &[egui::epaint::ClippedShape], needle: &str) -> Vec<Rect> {
    shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Text(t) if t.galley.text() == needle => {
                Some(t.galley.rect.translate(t.pos.to_vec2()))
            }
            _ => None,
        })
        .collect()
}

fn press(pos: Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

#[test]
fn a_failed_attachment_offers_a_retry_that_reports_its_index() {
    let entries = vec![ChatEntry::User(UserMessage {
        key: Cow::Borrowed("u-f"),
        text: Cow::Borrowed("see attached"),
        author: None,
        time: None,
        attachments: CowList::Owned(vec![
            AttachmentChip::new("ok.txt"),
            AttachmentChip {
                failed: true,
                ..AttachmentChip::new("trace.png")
            },
            AttachmentChip {
                failed: true,
                reading: true,
                ..AttachmentChip::new("again.png")
            },
        ]),
        delivery: Delivery::Sent,
    })];
    let ctx = Context::default();
    let mut state = TranscriptState::default();
    let mut actions = Vec::new();
    let mut frame = |events: Vec<egui::Event>| {
        let raw = RawInput { events, ..input() };
        let mut out = ctx.run_ui(raw, |ui| {
            let r = Transcript::new("retry").show(ui, &mut state, &entries, |_, _| {});
            actions.extend(r.actions);
        });
        out.textures_delta.clear();
        out.shapes
    };
    frame(Vec::new());
    let shapes = frame(Vec::new());
    let retries = text_rect(&shapes, "Retry");
    assert_eq!(retries.len(), 1, "one retry, none while reading");
    let at = retries[0].center();
    frame(vec![egui::Event::PointerMoved(at)]);
    frame(vec![press(at, true)]);
    frame(vec![press(at, false)]);
    frame(Vec::new());
    assert_eq!(
        actions,
        vec![ChatAction::RetryAttachment {
            key: "u-f".to_owned(),
            index: 1,
        }]
    );
}

#[test]
fn custom_rows_see_the_tap_that_stops_a_fling_as_suppressed() {
    let entries: Vec<ChatEntry<'static>> = (0..60)
        .map(|i| ChatEntry::Custom {
            key: Cow::Owned(format!("c{i}")),
        })
        .collect();
    let ctx = Context::default();
    let mut state = TranscriptState::default();
    let mut taps: Vec<(String, bool)> = Vec::new();
    let mut time = 0.0;
    let mut frame =
        |events: Vec<egui::Event>, state: &mut TranscriptState, taps: &mut Vec<(String, bool)>| {
            time += 1.0 / 60.0;
            let touch_screen = egui::Event::Touch {
                device_id: egui::TouchDeviceId(1),
                id: egui::TouchId(1),
                phase: egui::TouchPhase::Cancel,
                pos: Pos2::ZERO,
                force: None,
            };
            let raw = RawInput {
                time: Some(time),
                events: std::iter::once(touch_screen).chain(events).collect(),
                ..input()
            };
            let mut out = ctx.run_ui(raw, |ui| {
                Transcript::new("fling").show(ui, state, &entries, |ui, key| {
                    let size = vec2(ui.available_width(), 40.0);
                    let (_, r) = ui.allocate_exact_size(size, egui::Sense::click());
                    if r.clicked() {
                        taps.push((key.to_owned(), Transcript::tap_suppressed(ui.ctx())));
                    }
                });
            });
            out.textures_delta.clear();
        };
    let start = Pos2::new(400.0, 150.0);
    for _ in 0..3 {
        frame(
            vec![egui::Event::PointerMoved(start)],
            &mut state,
            &mut taps,
        );
    }
    frame(vec![press(start, true)], &mut state, &mut taps);
    let mut at = start;
    for _ in 0..5 {
        at.y += 50.0;
        frame(vec![egui::Event::PointerMoved(at)], &mut state, &mut taps);
    }
    frame(vec![press(at, false)], &mut state, &mut taps);
    let sid = state.scroll_id.expect("scroll area drawn");
    let speed = ScrollState::load(&ctx, sid).map_or(0.0, |s| s.velocity().length());
    assert!(speed > FLING_SPEED, "no fling: {speed}");
    assert!(taps.is_empty(), "{taps:?}");

    frame(vec![press(at, true)], &mut state, &mut taps);
    assert!(state.tap_suppressed());
    frame(vec![press(at, false)], &mut state, &mut taps);
    assert!(state.tap_suppressed());
    assert!(!Transcript::tap_suppressed(&ctx));
    assert_eq!(taps.len(), 1, "{taps:?}");
    assert!(
        taps[0].1,
        "the tap that stopped the fling counted: {taps:?}"
    );

    for _ in 0..600 {
        frame(Vec::new(), &mut state, &mut taps);
        if ScrollState::load(&ctx, sid).is_some_and(|s| s.velocity().length() < 1.0) {
            break;
        }
    }
    frame(vec![press(at, true)], &mut state, &mut taps);
    frame(vec![press(at, false)], &mut state, &mut taps);
    assert!(!state.tap_suppressed());
    assert_eq!(taps.len(), 2, "{taps:?}");
    assert!(!taps[1].1, "a tap at rest is suppressed: {taps:?}");
}

fn plan_card(
    key: &'static str,
    dismiss: Option<&'static str>,
    open: Option<bool>,
) -> ChatEntry<'static> {
    let step = |text, status| PlanStep {
        text: Cow::Borrowed(text),
        status,
    };
    ChatEntry::Plan(PlanCard {
        key: Cow::Borrowed(key),
        steps: CowList::Owned(vec![
            step("Scan", PlanStatus::Completed),
            step("Fix", PlanStatus::InProgress),
            step("Verify", PlanStatus::Pending),
        ]),
        dismiss: dismiss.map(Cow::Borrowed),
        open,
        ..PlanCard::default()
    })
}

/// One frame of floating cards; returns the actions and painted shapes.
fn float_frame(
    ctx: &Context,
    cards: &[ChatEntry<'_>],
    events: Vec<egui::Event>,
) -> (Vec<(String, CardAction)>, Vec<egui::epaint::ClippedShape>) {
    let mut actions = Vec::new();
    let raw = RawInput { events, ..input() };
    let mut out = ctx.run_ui(raw, |ui| actions = floating_cards(ui, "float", cards));
    out.textures_delta.clear();
    (actions, out.shapes)
}

/// Clickable small icon buttons drawn last frame, left to right.
fn header_buttons(ctx: &Context) -> Vec<Rect> {
    let side = crate::controls::IconBox::Sm.side(&crate::controls::Metrics::get(ctx));
    let mut out: Vec<Rect> = ctx.viewport(|v| {
        v.prev_pass
            .widgets
            .layers()
            .flat_map(|(_, w)| w.iter())
            .filter(|w| w.sense.senses_click() && (w.rect.width() - side).abs() < 0.5)
            .map(|w| w.rect)
            .collect()
    });
    out.sort_by(|a, b| a.left().total_cmp(&b.left()));
    out
}

/// Clicks `at` over three frames; returns the actions reported.
fn float_click(ctx: &Context, cards: &[ChatEntry<'_>], at: Pos2) -> Vec<(String, CardAction)> {
    let mut acted = float_frame(ctx, cards, vec![egui::Event::PointerMoved(at)]).0;
    acted.extend(float_frame(ctx, cards, vec![press(at, true)]).0);
    acted.extend(float_frame(ctx, cards, vec![press(at, false)]).0);
    acted
}

#[test]
fn a_plan_close_button_sits_left_of_the_fold_and_reports_dismiss() {
    let ctx = Context::default();
    let plain = [plan_card("p-plain", None, None)];
    float_frame(&ctx, &plain, Vec::new());
    float_frame(&ctx, &plain, Vec::new());
    assert_eq!(header_buttons(&ctx).len(), 1, "fold only");

    let cards = [plan_card("p-close", Some("Hide plan"), None)];
    float_frame(&ctx, &cards, Vec::new());
    let (_, shapes) = float_frame(&ctx, &cards, Vec::new());
    let buttons = header_buttons(&ctx);
    assert_eq!(buttons.len(), 2, "{buttons:?}");
    let (close, fold) = (buttons[0], buttons[1]);
    assert!(close.right() <= fold.left(), "{close:?} {fold:?}");
    assert_eq!(close.top(), fold.top());
    let title = text_rect(&shapes, "Plan")[0];
    assert!(title.bottom() > close.top() && title.top() < close.bottom());
    assert_eq!(
        float_click(&ctx, &cards, close.center()),
        vec![("p-close".to_owned(), CardAction::Dismiss)]
    );
    assert!(float_click(&ctx, &cards, fold.center()).is_empty());
}

#[test]
fn a_plan_starts_in_its_open_state_and_resets_on_a_new_key_or_value() {
    let ctx = Context::default();
    let shows = |cards: &[ChatEntry<'_>]| {
        let (_, shapes) = float_frame(&ctx, cards, Vec::new());
        ["Scan", "Fix", "Verify"].map(|t| !text_rect(&shapes, t).is_empty())
    };
    let open = [true, true, true];
    let folded = [false, true, false];

    assert_eq!(shows(&[plan_card("p-1", None, None)]), open);
    assert_eq!(shows(&[plan_card("p-1", None, Some(false))]), folded);

    let running = [plan_card("p-1", None, Some(true))];
    assert_eq!(shows(&running), open);
    let fold = header_buttons(&ctx)[0];
    float_click(&ctx, &running, fold.center());
    assert_eq!(shows(&running), folded, "the user's fold holds");
    assert_eq!(shows(&running), folded);

    let finished = [plan_card("p-1", None, Some(false))];
    assert_eq!(shows(&finished), folded);
    let fold = header_buttons(&ctx)[0];
    float_click(&ctx, &finished, fold.center());
    assert_eq!(shows(&finished), open, "unfolding a finished plan holds");

    assert_eq!(shows(&[plan_card("p-2", None, Some(false))]), folded);
    assert_eq!(shows(&[plan_card("p-3", None, None)]), open);
}

#[test]
fn transcript_thumbnails_crop_to_a_centred_square() {
    let ctx = Context::default();
    let image = egui::ColorImage::filled([30, 60], egui::Color32::BLUE);
    let tall = ctx.load_texture("tall", image, egui::TextureOptions::default());
    let entries = vec![ChatEntry::User(UserMessage {
        key: Cow::Borrowed("u-t"),
        text: Cow::Borrowed("look"),
        author: None,
        time: None,
        attachments: CowList::Owned(vec![AttachmentChip {
            image: Some(tall.id()),
            ..AttachmentChip::new("tall.png")
        }]),
        delivery: Delivery::Sent,
    })];
    let mut state = TranscriptState::default();
    let mut uv = None;
    for _ in 0..2 {
        let mut out = ctx.run_ui(input(), |ui| {
            Transcript::new("thumbs").show(ui, &mut state, &entries, |_, _| {});
        });
        out.textures_delta.clear();
        uv = out.shapes.iter().find_map(|c| match &c.shape {
            egui::Shape::Rect(r) => r
                .brush
                .as_ref()
                .filter(|b| b.fill_texture_id == tall.id())
                .map(|b| b.uv),
            _ => None,
        });
    }
    assert_eq!(
        uv,
        Some(Rect::from_min_max(
            Pos2::new(0.0, 0.25),
            Pos2::new(1.0, 0.75)
        ))
    );
}

const LOCK_REASON: &str = "Only the release owner can run this";

static LOCK_CHOICES: [CardChoice<'static>; 3] = [
    CardChoice {
        label: Cow::Borrowed("Run"),
        kind: ButtonKind::Primary,
        shortcut: Some(Cow::Borrowed("Y")),
        while_locked: false,
    },
    CardChoice {
        label: Cow::Borrowed("Deny"),
        kind: ButtonKind::Danger,
        shortcut: None,
        while_locked: false,
    },
    CardChoice {
        label: Cow::Borrowed("Dismiss"),
        kind: ButtonKind::Ghost,
        shortcut: Some(Cow::Borrowed("D")),
        while_locked: true,
    },
];

fn lock_card(state: CardState<'static>) -> [ChatEntry<'static>; 1] {
    [ChatEntry::Approval(ApprovalCard {
        key: Cow::Borrowed("ap-lock"),
        context: Cow::Borrowed("desk"),
        title: Cow::Borrowed("Publish the crate"),
        command: None,
        detail: None,
        reason: None,
        choices: CowList::Borrowed(&LOCK_CHOICES),
        state,
    })]
}

fn locked() -> CardState<'static> {
    CardState::Locked {
        reason: Cow::Borrowed(LOCK_REASON),
    }
}

/// A context that shows tooltips without delay.
fn tooltip_ctx() -> Context {
    let ctx = Context::default();
    ctx.all_styles_mut(|s| {
        s.interaction.tooltip_delay = 0.0;
        s.interaction.show_tooltips_only_when_still = false;
    });
    ctx
}

/// Centre of the button labelled `label` and whether it was enabled last frame.
fn float_button(ctx: &Context, cards: &[ChatEntry<'_>], label: &str) -> (Pos2, bool) {
    let shapes = float_frame(ctx, cards, Vec::new()).1;
    let at = text_rect(&shapes, label)
        .first()
        .unwrap_or_else(|| panic!("{label} is drawn"))
        .center();
    let enabled = ctx.viewport(|v| {
        v.prev_pass
            .widgets
            .layers()
            .flat_map(|(_, w)| w.iter())
            .filter(|w| w.sense.senses_click() && w.rect.contains(at))
            .min_by(|a, b| a.rect.area().total_cmp(&b.rect.area()))
            .map(|w| w.enabled)
    });
    (at, enabled.unwrap_or_else(|| panic!("{label} is a button")))
}

/// Times the lock reason is painted while the pointer rests on `at`.
fn reasons_on_hover(ctx: &Context, cards: &[ChatEntry<'_>], at: Pos2) -> usize {
    let mut shapes = Vec::new();
    for _ in 0..3 {
        shapes = float_frame(ctx, cards, vec![egui::Event::PointerMoved(at)]).1;
    }
    text_rect(&shapes, LOCK_REASON).len()
}

fn key_press(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

#[test]
fn a_locked_card_draws_its_choices_disabled_with_the_reason_as_tooltip() {
    let ctx = tooltip_ctx();
    let cards = lock_card(locked());
    float_frame(&ctx, &cards, Vec::new());
    let shapes = float_frame(&ctx, &cards, Vec::new()).1;
    assert_eq!(text_rect(&shapes, LOCK_REASON).len(), 1, "reason line kept");
    let (run, run_on) = float_button(&ctx, &cards, "Run");
    let (_, deny_on) = float_button(&ctx, &cards, "Deny");
    assert!(!run_on && !deny_on, "locked choices draw disabled");
    assert_eq!(
        reasons_on_hover(&ctx, &cards, run),
        2,
        "reason line and tooltip"
    );
    assert!(
        float_click(&ctx, &cards, run).is_empty(),
        "a disabled choice does nothing"
    );
    float_frame(&ctx, &cards, vec![egui::Event::PointerMoved(run)]);
    assert!(
        float_frame(&ctx, &cards, vec![key_press(egui::Key::Y)])
            .0
            .is_empty()
    );
}

#[test]
fn a_while_locked_choice_stays_usable_and_keeps_its_index() {
    let ctx = tooltip_ctx();
    let cards = lock_card(locked());
    float_frame(&ctx, &cards, Vec::new());
    let (dismiss, enabled) = float_button(&ctx, &cards, "Dismiss");
    assert!(enabled);
    assert_eq!(reasons_on_hover(&ctx, &cards, dismiss), 1, "no tooltip");
    assert_eq!(
        float_frame(&ctx, &cards, vec![key_press(egui::Key::D)]).0,
        vec![("ap-lock".to_owned(), CardAction::Choose(2))]
    );
    assert_eq!(
        float_click(&ctx, &cards, dismiss),
        vec![("ap-lock".to_owned(), CardAction::Choose(2))]
    );
}

#[test]
fn an_unlocked_card_keeps_every_choice_and_index() {
    let ctx = tooltip_ctx();
    let cards = lock_card(CardState::Open);
    float_frame(&ctx, &cards, Vec::new());
    for label in ["Run", "Deny", "Dismiss"] {
        assert!(float_button(&ctx, &cards, label).1, "{label} is enabled");
    }
    let (run, _) = float_button(&ctx, &cards, "Run");
    assert_eq!(reasons_on_hover(&ctx, &cards, run), 0);
    assert_eq!(
        float_click(&ctx, &cards, run),
        vec![("ap-lock".to_owned(), CardAction::Choose(0))]
    );
    let (dismiss, _) = float_button(&ctx, &cards, "Dismiss");
    assert_eq!(
        float_click(&ctx, &cards, dismiss),
        vec![("ap-lock".to_owned(), CardAction::Choose(2))]
    );
}

#[test]
fn a_locked_question_card_draws_its_buttons_disabled_with_the_reason_as_tooltip() {
    let ctx = tooltip_ctx();
    let cards = [ChatEntry::Question(QuestionCard {
        key: Cow::Borrowed("q-lock"),
        context: Cow::Borrowed("desk"),
        header: None,
        why: None,
        questions: CowList::Owned(vec![Question {
            id: Cow::Borrowed("why"),
            prompt: Cow::Borrowed("Why publish now?"),
            form: QuestionForm::Text,
            items: CowList::default(),
            free_text: false,
            header: None,
            why: None,
        }]),
        state: locked(),
    })];
    float_frame(&ctx, &cards, Vec::new());
    let (skip, enabled) = float_button(&ctx, &cards, "Skip");
    assert!(!enabled && !float_button(&ctx, &cards, "Send answer").1);
    assert_eq!(reasons_on_hover(&ctx, &cards, skip), 2);
    assert!(float_click(&ctx, &cards, skip).is_empty());
}
