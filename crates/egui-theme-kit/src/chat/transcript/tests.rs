//! Frame tests of the transcript: every entry kind, virtualisation, following, jumps and anchoring.

use std::borrow::Cow;

use egui::{Context, Pos2, RawInput, Rect, vec2};

use super::*;
use crate::chat::mention::{MentionRef, encode};
use crate::chat::types::{CowList, Tone};
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
    },
    CardChoice {
        label: Cow::Borrowed("Deny"),
        kind: ButtonKind::Danger,
        shortcut: None,
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
            attachments: CowList::Owned(vec![AttachmentChip {
                name: Cow::Borrowed("dump.dmp"),
                image: None,
                reading: true,
            }]),
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
