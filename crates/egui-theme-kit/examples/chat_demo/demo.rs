//! A chat window built from the kit chat widgets over synthetic sessions.

use std::borrow::Cow;

use egui::{Align, CentralPanel, ComboBox, Frame, Layout, Rect, Ui, UiBuilder, pos2, vec2};
use egui_theme_kit::PresetStyles;
use egui_theme_kit::chat::{
    Activity, AgentMessage, AppBarAction, ApprovalCard, AttachmentChip, BarButton, BoardAction,
    BoardGroup, CardAction, CardChoice, CardState, ChatAction, ChatAppBar, ChatEntry, ChipKind,
    Composer, ComposerAction, ComposerMode, ComposerState, CowList, Delivery, DetailLevel,
    EnterBehavior, Jump, LoadState, MentionCandidate, MentionRef, MentionSource, MenuEntry,
    MessageNav, NavStep, NewSessionButton, Notice, OptionChip, Payload, PlanCard, PlanStatus,
    PlanStep, PlusItem, Question, QuestionCard, QuestionForm, QuestionItem, SendIntent, SessionBar,
    SessionBarAction, SessionList, SessionListAction, SessionRow, SessionSection, SessionStatus,
    SlashCommand, Tone, Transcript, TranscriptOptions, TranscriptState, UnixMs, UserMessage,
    ViewItem, ViewMenu, WaitingBoard, WorkItem, WorkKind, WorkStatus, encode,
};
use egui_theme_kit::controls::{IconBox, PaletteItem, icon_button, icons};
use egui_theme_kit::kit::ButtonKind;
use serde_json::json;

/// Main column width at and above which the session list shows beside the chat.
const WIDE: f32 = 760.0;
const SIDEBAR_W: f32 = 272.0;
/// Width cap of the transcript and composer column.
const COLUMN_W: f32 = 860.0;
const MINUTE: i64 = 60_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum View {
    Chats,
    Waiting,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum BarId {
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Ctl {
    Model,
    Access,
    Context,
    AutoReview,
    Branch,
    Files,
    Terminal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum RowMenu {
    Rename,
    Archive,
}

/// A synthetic session in the list.
struct DemoSession {
    key: u32,
    /// Mention id, `machine/thread`.
    id: &'static str,
    title: &'static str,
    subtitle: &'static str,
    status: SessionStatus,
    minutes_ago: i64,
    unread: u32,
    pinned: bool,
    today: bool,
}

const SESSIONS: [DemoSession; 6] = [
    DemoSession {
        key: 1,
        id: "bench-07/t-131",
        title: "Fix the flaky transcript scroll test",
        subtitle: "kit-chat \u{b7} BENCH-07",
        status: SessionStatus::Working,
        minutes_ago: 0,
        unread: 0,
        pinned: false,
        today: true,
    },
    DemoSession {
        key: 2,
        id: "bench-07/t-118",
        title: "Virtual scroll rewrite",
        subtitle: "kit-chat \u{b7} BENCH-07",
        status: SessionStatus::Idle,
        minutes_ago: 95,
        unread: 0,
        pinned: true,
        today: true,
    },
    DemoSession {
        key: 3,
        id: "lab-02/t-044",
        title: "Release notes draft",
        subtitle: "docs \u{b7} LAB-02",
        status: SessionStatus::Waiting,
        minutes_ago: 12,
        unread: 2,
        pinned: false,
        today: true,
    },
    DemoSession {
        key: 4,
        id: "lab-02/t-040",
        title: "Printer queue cleanup",
        subtitle: "ops \u{b7} LAB-02",
        status: SessionStatus::Error,
        minutes_ago: 41,
        unread: 1,
        pinned: false,
        today: true,
    },
    DemoSession {
        key: 5,
        id: "bench-03/t-007",
        title: "Nightly backup audit",
        subtitle: "ops \u{b7} BENCH-03",
        status: SessionStatus::Closed,
        minutes_ago: 60 * 26,
        unread: 0,
        pinned: false,
        today: false,
    },
    DemoSession {
        key: 6,
        id: "bench-03/t-002",
        title: "Font fallback investigation",
        subtitle: "kit \u{b7} BENCH-03",
        status: SessionStatus::Offline,
        minutes_ago: 60 * 50,
        unread: 0,
        pinned: false,
        today: false,
    },
];

fn session(key: u32) -> &'static DemoSession {
    SESSIONS
        .iter()
        .find(|s| s.key == key)
        .unwrap_or(&SESSIONS[0])
}

fn mention_of(s: &DemoSession) -> MentionRef {
    MentionRef::new("session", s.id, s.title)
}

/// Session mentions over [`SESSIONS`].
struct Mentions;

impl MentionSource for Mentions {
    fn suggest(&self, query: &str, limit: usize) -> Vec<MentionCandidate> {
        let query = query.to_lowercase();
        SESSIONS
            .iter()
            .filter(|s| s.title.to_lowercase().contains(&query))
            .take(limit)
            .map(|s| MentionCandidate {
                target: mention_of(s),
                detail: s.subtitle.to_owned(),
                status: Some(s.status),
                section: Some(if s.today { "Today" } else { "Earlier" }.to_owned()),
            })
            .collect()
    }

    fn resolve(&self, target: &MentionRef) -> Option<String> {
        SESSIONS
            .iter()
            .find(|s| target.kind == "session" && s.id == target.id)
            .map(|s| s.title.to_owned())
    }
}

/// Slash commands whose name contains every character of `query` in order.
fn slash_commands(query: &str) -> Vec<SlashCommand<'static>> {
    let all = [
        (
            "compact",
            "",
            "Summarise older turns to free context",
            "Session",
        ),
        ("rename", "<title>", "Rename this session", "Session"),
        (
            "model",
            "<name>",
            "Switch the model for new turns",
            "Session",
        ),
        ("review", "<target>", "Review the current diff", "Code"),
        ("diff", "", "Show changed files", "Code"),
        ("test", "<filter>", "Run tests matching a filter", "Code"),
    ];
    let query = query.to_lowercase();
    all.into_iter()
        .filter(|(name, ..)| {
            let mut rest = name.chars();
            query.chars().all(|q| rest.any(|c| c == q))
        })
        .map(|(name, args, about, section)| SlashCommand {
            id: name.into(),
            name: format!("/{name}").into(),
            args: args.into(),
            about: about.into(),
            section: Some(section.into()),
            icon: Some(if section == "Code" {
                icons::TERMINAL
            } else {
                icons::SLASH
            }),
            enabled: true,
            run_on_pick: args.is_empty(),
        })
        .collect()
}

static APPROVE: [CardChoice<'static>; 3] = [
    CardChoice {
        label: Cow::Borrowed("Approve"),
        kind: ButtonKind::Primary,
        shortcut: Some(Cow::Borrowed("Y")),
    },
    CardChoice {
        label: Cow::Borrowed("Always for this command"),
        kind: ButtonKind::Neutral,
        shortcut: None,
    },
    CardChoice {
        label: Cow::Borrowed("Deny"),
        kind: ButtonKind::Danger,
        shortcut: Some(Cow::Borrowed("N")),
    },
];

static PLUS: [PlusItem<'static>; 2] = [
    PlusItem {
        id: Cow::Borrowed("new-session"),
        label: Cow::Borrowed("New session"),
        icon: Some(icons::PLUS),
        detail: Some(Cow::Borrowed("Same folder and machine")),
    },
    PlusItem {
        id: Cow::Borrowed("screenshot"),
        label: Cow::Borrowed("Attach a screenshot"),
        icon: Some(icons::PAPERCLIP),
        detail: None,
    },
];

const MODELS: [Cow<'static, str>; 3] = [
    Cow::Borrowed("Large"),
    Cow::Borrowed("Medium"),
    Cow::Borrowed("Fast"),
];
const ACCESS: [Cow<'static, str>; 3] = [
    Cow::Borrowed("Ask first"),
    Cow::Borrowed("Auto-edit"),
    Cow::Borrowed("Full access"),
];

fn text(s: &'static str) -> Cow<'static, str> {
    Cow::Borrowed(s)
}

fn user(key: &str, wire: String, at: UnixMs) -> ChatEntry<'static> {
    ChatEntry::User(UserMessage {
        key: key.to_owned().into(),
        text: wire.into(),
        author: None,
        time: Some(at),
        attachments: CowList::default(),
        delivery: Delivery::Sent,
    })
}

fn agent(key: &'static str, body: &'static str, at: UnixMs) -> ChatEntry<'static> {
    ChatEntry::Agent(AgentMessage {
        key: text(key),
        text: text(body),
        author: Some(text("Agent")),
        time: Some(at),
        streaming: false,
    })
}

fn notice(key: String, body: String, tone: Tone, at: UnixMs) -> ChatEntry<'static> {
    ChatEntry::Notice(Notice {
        key: key.into(),
        text: body.into(),
        time: Some(at),
        tone,
    })
}

/// A finished work item with no timing, payloads or body.
fn work(
    key: &'static str,
    kind: WorkKind,
    name: &'static str,
    summary: &'static str,
) -> WorkItem<'static> {
    WorkItem {
        key: text(key),
        kind,
        name: text(name),
        summary: text(summary),
        status: WorkStatus::Ok,
        started: None,
        duration_ms: None,
        arguments: None,
        result: None,
        body: None,
        files: CowList::default(),
    }
}

fn json(value: serde_json::Value) -> Option<Payload<'static>> {
    Some(Payload::Json(Cow::Owned(value)))
}

fn item(
    id: &'static str,
    label: &'static str,
    detail: Option<&'static str>,
    recommended: bool,
) -> QuestionItem<'static> {
    QuestionItem {
        id: text(id),
        label: text(label),
        detail: detail.map(text),
        recommended,
        reason: None,
    }
}

/// History before the first loaded entry, prepended on Load earlier.
fn older_entries(now: UnixMs) -> Vec<ChatEntry<'static>> {
    let at = |min: i64| UnixMs(now.0 - min * MINUTE);
    vec![
        user(
            "u-old",
            "Start by listing the transcript tests that touch scrolling.".into(),
            at(58),
        ),
        agent(
            "a-old",
            "Six tests touch scrolling. Two of them, `follows_bottom_after_prepend` and `holds_anchor_on_resize`, set the scroll offset directly.",
            at(57),
        ),
    ]
}

/// The open session's history.
fn entries(now: UnixMs) -> Vec<ChatEntry<'static>> {
    let at = |min: i64| UnixMs(now.0 - min * MINUTE);
    let earlier = mention_of(session(2));
    let mut reasoning = work(
        "w-plan",
        WorkKind::Reasoning,
        "Thinking",
        "Where the offset is read",
    );
    reasoning.body = Some(text(
        "The failure only shows on CI, so it is timing related. The test reads the offset right after `jump`, which applies on the next frame.",
    ));
    reasoning.duration_ms = Some(3_400);
    let mut read = work(
        "w-read",
        WorkKind::Tool,
        "read_file",
        "src/chat/transcript.rs",
    );
    read.arguments = json(json!({ "path": "src/chat/transcript.rs", "range": [180, 260] }));
    read.result = json(json!({ "lines": 80, "truncated": false }));
    read.duration_ms = Some(120);
    let mut grep = work(
        "w-grep",
        WorkKind::Command,
        "Search",
        "rg -n \"stick_frames\" src/chat",
    );
    grep.body = Some(text(
        "src/chat/transcript.rs:208:    stick_frames: u32,\nsrc/chat/transcript.rs:241:        self.stick_frames = self.stick_frames.max(frames);",
    ));
    grep.duration_ms = Some(310);

    let mut run = work(
        "w-run",
        WorkKind::Tool,
        "run_tests",
        "transcript::follows, 20 repeats",
    );
    run.status = WorkStatus::Failed;
    run.duration_ms = Some(48_200);
    run.arguments = json(json!({
        "filter": "transcript::follows_bottom_after_prepend",
        "repeat": 20,
        "features": ["chat", "chat-json"],
        "env": { "RUST_BACKTRACE": "1" }
    }));
    run.result = json(json!({
        "passed": 16,
        "failed": 4,
        "first_failure": {
            "run": 3,
            "message": "assertion failed: response.at_bottom",
            "frame": 2,
            "offset": { "expected": 1840.0, "actual": 1796.5 }
        },
        "duration_s": 48.2
    }));
    let mut log = work(
        "w-log",
        WorkKind::Command,
        "Shell",
        "cargo test transcript:: -- --nocapture",
    );
    log.status = WorkStatus::Failed;
    log.duration_ms = Some(6_800);
    log.body = Some(text(
        "running 1 test\ntest chat::transcript::tests::follows_bottom_after_prepend ... FAILED\n\nfailures:\n    assertion failed: response.at_bottom\n\ntest result: FAILED. 0 passed; 1 failed",
    ));

    let mut patch = work(
        "w-edit",
        WorkKind::FileChange,
        "Edit",
        "Hold the anchor for one frame",
    );
    patch.files = CowList::Owned(vec![
        text("src/chat/transcript.rs"),
        text("src/chat/transcript/tests.rs"),
    ]);
    patch.body = Some(text(
        "@@ -238,6 +238,7 @@\n     pub fn stick_for(&mut self, frames: u32) {\n-        self.stick_frames = frames;\n+        self.stick_frames = self.stick_frames.max(frames);\n         self.following = true;",
    ));
    patch.duration_ms = Some(900);
    let mut rerun = work(
        "w-rerun",
        WorkKind::Command,
        "Shell",
        "cargo test transcript:: --repeat 20",
    );
    rerun.status = WorkStatus::Running;
    rerun.started = Some(UnixMs(now.0 - 14_000));

    vec![
        notice(
            "n-start".into(),
            "Session resumed on BENCH-07".into(),
            Tone::Info,
            at(42),
        ),
        ChatEntry::User(UserMessage {
            key: text("u-1"),
            text: text(
                "The follows_bottom_after_prepend test fails about one run in five on CI. Can you find out why and fix it? The CI log is attached.",
            ),
            author: None,
            time: Some(at(41)),
            attachments: CowList::Owned(vec![AttachmentChip {
                name: text("ci-run-4812.log"),
                image: None,
                reading: false,
            }]),
            delivery: Delivery::Sent,
        }),
        ChatEntry::Work(reasoning),
        ChatEntry::Work(read),
        ChatEntry::Work(grep),
        agent(
            "a-1",
            "## What I found\n\nThe test reads the scroll offset **one frame early**:\n\n1. `jump(Jump::Bottom)` only records the target.\n2. The next frame applies it, but the test asserts right after the first frame.\n3. On a slow runner the font atlas grows in between, so row heights change and the offset moves.\n\n```rust\nstate.jump(Jump::Bottom);\nlet r = run(&ctx, &mut state, &entries);\nassert!(r.at_bottom); // runs before the jump lands\n```\n\nI'll repeat the test to confirm before changing anything.",
            at(38),
        ),
        ChatEntry::Work(run),
        ChatEntry::Work(log),
        notice(
            "n-compact".into(),
            "Context compacted \u{b7} 38% used".into(),
            Tone::Neutral,
            at(30),
        ),
        user(
            "u-2",
            format!(
                "Compare with how {} handled anchoring before you change anything.",
                encode(&earlier)
            ),
            at(29),
        ),
        agent(
            "a-2",
            "That session fixed the same race by holding the anchor for one extra frame after a jump. I'll do the same here and add a test that waits two frames:\n\n- keep `stick_frames` at the larger of the old and new values\n- assert `at_bottom` only after the jump has landed\n\nSee [the egui scroll docs](https://docs.rs/egui) for how offsets apply.",
            at(27),
        ),
        ChatEntry::Plan(PlanCard {
            key: text("p-1"),
            title: Some(text("Fix plan")),
            explanation: None,
            steps: CowList::Owned(vec![
                PlanStep {
                    text: text("Reproduce the failure 20 times"),
                    status: PlanStatus::Completed,
                },
                PlanStep {
                    text: text("Hold the anchor for one frame"),
                    status: PlanStatus::Completed,
                },
                PlanStep {
                    text: text("Re-run the suite"),
                    status: PlanStatus::InProgress,
                },
                PlanStep {
                    text: text("Commit on feat/chat"),
                    status: PlanStatus::Pending,
                },
            ]),
        }),
        ChatEntry::Work(patch),
        ChatEntry::Work(rerun),
        ChatEntry::Approval(ApprovalCard {
            key: text("ap-1"),
            context: text("BENCH-07 \u{b7} kit-chat"),
            title: text("Run a command"),
            command: Some(text(
                "cargo test -p egui-theme-kit --features chat transcript:: -- --test-threads 1",
            )),
            detail: json(json!({
                "cwd": "E:/work/kit-chat",
                "timeout_s": 600,
                "env": { "RUST_BACKTRACE": "1", "CARGO_BUILD_JOBS": "1" }
            })),
            reason: Some(text(
                "Runs the transcript tests one at a time to rule out shared state.",
            )),
            choices: CowList::Borrowed(&APPROVE),
            state: CardState::Open,
        }),
        ChatEntry::Question(QuestionCard {
            key: text("q-1"),
            context: text("Fix the flaky transcript scroll test"),
            header: Some(text("Before I commit")),
            why: Some(text("The answers decide where the regression test goes.")),
            questions: CowList::Owned(vec![
                Question {
                    id: text("where"),
                    prompt: text("Where should the regression test live?"),
                    form: QuestionForm::Choice,
                    items: CowList::Owned(vec![
                        item(
                            "unit",
                            "transcript/tests.rs",
                            Some("Next to the other frame tests"),
                            true,
                        ),
                        item(
                            "integration",
                            "tests/scroll.rs",
                            Some("Public API only"),
                            false,
                        ),
                    ]),
                    free_text: false,
                    header: None,
                    why: None,
                },
                Question {
                    id: text("push"),
                    prompt: text("Push the branch after committing?"),
                    form: QuestionForm::Confirm,
                    items: CowList::default(),
                    free_text: false,
                    header: None,
                    why: None,
                },
            ]),
            state: CardState::Open,
        }),
    ]
}

/// The demo app state.
pub struct ChatDemo {
    now: UnixMs,
    entries: Vec<ChatEntry<'static>>,
    older: Option<LoadState>,
    transcript: TranscriptState,
    pub composer: ComposerState,
    attachments: Vec<AttachmentChip<'static>>,
    detail: DetailLevel,
    thinking: bool,
    view: View,
    search: String,
    selected: u32,
    sidebar_wide: bool,
    sidebar_narrow: bool,
    nav: MessageNav,
    model: usize,
    access: usize,
    auto_review: bool,
    title: String,
    error: Option<String>,
    preset: PresetStyles,
    next_key: u64,
    focus_pending: bool,
    /// Composer height measured last frame.
    composer_h: f32,
}

impl ChatDemo {
    /// A demo whose times are relative to `now`, showing `preset` in the theme menu.
    pub fn new(now: UnixMs, preset: PresetStyles) -> Self {
        let mut composer = ComposerState::default();
        composer.set_wire(&format!(
            "/review {} before the commit",
            encode(&mention_of(session(3)))
        ));
        Self {
            now,
            entries: entries(now),
            older: Some(LoadState::Available),
            transcript: TranscriptState::default(),
            composer,
            attachments: vec![AttachmentChip {
                name: text("scroll-trace.png"),
                image: None,
                reading: false,
            }],
            detail: DetailLevel::Detailed,
            thinking: true,
            view: View::Chats,
            search: String::new(),
            selected: 1,
            sidebar_wide: true,
            sidebar_narrow: false,
            nav: MessageNav::default(),
            model: 0,
            access: 1,
            auto_review: true,
            title: session(1).title.to_owned(),
            error: Some(
                "Lost the connection to BENCH-07 for 4 s; output may be incomplete.".into(),
            ),
            preset,
            next_key: 0,
            focus_pending: true,
            composer_h: 96.0,
        }
    }

    const COMPOSER: &'static str = "chat_demo.composer";

    /// Updates the clock used for running durations and relative times.
    pub fn set_now(&mut self, now: UnixMs) {
        self.now = now;
    }

    /// Draws the whole window into a central panel.
    pub fn show(&mut self, ui: &mut Ui) {
        let frame = Frame::central_panel(&ui.ctx().global_style()).inner_margin(0.0);
        CentralPanel::default()
            .frame(frame)
            .show(ui, |ui| self.body(ui));
    }

    fn body(&mut self, ui: &mut Ui) {
        let full = ui.max_rect();
        let wide = full.width() >= WIDE;
        let main = if wide && self.sidebar_wide {
            let side = Rect::from_min_size(full.min, vec2(SIDEBAR_W, full.height()));
            ui.scope_builder(
                UiBuilder::new()
                    .max_rect(side.shrink(8.0))
                    .layout(Layout::top_down_justified(Align::Min)),
                |ui| self.sidebar(ui),
            );
            let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
            ui.painter().vline(side.right(), side.y_range(), stroke);
            Rect::from_min_max(pos2(side.right() + 1.0, full.top()), full.max)
        } else {
            full
        };
        if !wide && self.sidebar_narrow {
            ui.scope_builder(
                UiBuilder::new()
                    .max_rect(main.shrink(8.0))
                    .layout(Layout::top_down_justified(Align::Min)),
                |ui| {
                    self.app_bar(ui, wide);
                    self.sidebar(ui);
                },
            );
            return;
        }
        ui.scope_builder(
            UiBuilder::new()
                .max_rect(main.shrink(8.0))
                .layout(Layout::top_down(Align::Min)),
            |ui| {
                self.app_bar(ui, wide);
                match self.view {
                    View::Chats => self.chat(ui),
                    View::Waiting => self.board(ui),
                }
            },
        );
    }

    fn app_bar(&mut self, ui: &mut Ui, wide: bool) {
        let waiting = self.waiting_cards().len() as u32 + 1;
        let views = [
            ViewItem::new(View::Chats, "Chats").icon(icons::LIST),
            ViewItem::new(View::Waiting, "Waiting")
                .icon(icons::QUESTION)
                .badge(waiting, Tone::Warn),
        ];
        let trailing = [BarButton::new(BarId::Settings, icons::GEAR, "Settings")];
        let (mut detail, mut thinking) = (self.detail, self.thinking);
        let mut toggle_sidebar = false;
        let preset = &mut self.preset;
        let show_sidebar = if wide {
            self.sidebar_wide
        } else {
            self.sidebar_narrow
        };
        let action = ChatAppBar::new("chat_demo.app_bar")
            .view(ViewMenu::new(self.view, &views))
            .detail(&mut detail)
            .thinking(&mut thinking)
            .trailing(&trailing)
            .show(ui, |ui| {
                if icon_button(ui, icons::MENU, IconBox::Md, show_sidebar, "Sessions").clicked() {
                    toggle_sidebar = true;
                }
                if wide {
                    theme_menu(ui, preset);
                }
            });
        self.detail = detail;
        self.thinking = thinking;
        if toggle_sidebar {
            let open = if wide {
                &mut self.sidebar_wide
            } else {
                &mut self.sidebar_narrow
            };
            *open = !*open;
        }
        match action {
            Some(AppBarAction::View(view)) => self.view = view,
            Some(AppBarAction::Detail(detail)) => self.detail = detail,
            Some(AppBarAction::Thinking(on)) => self.thinking = on,
            Some(AppBarAction::Button(BarId::Settings)) => {
                self.push_notice("Settings are not part of the demo".into(), Tone::Neutral);
            }
            None => {}
        }
    }

    fn sidebar(&mut self, ui: &mut Ui) {
        let templates = [
            PaletteItem::new("blank", "Blank session")
                .icon(icons::PLUS)
                .section("Start"),
            PaletteItem::new("review", "Code review")
                .icon(icons::FILE_DIFF)
                .detail("Diff against main")
                .section("Templates"),
            PaletteItem::new("triage", "Crash triage")
                .icon(icons::WRENCH)
                .detail("Reads dumps and logs")
                .section("Templates"),
        ];
        if NewSessionButton::new("New session", &templates)
            .show(ui)
            .is_some()
        {
            self.push_notice(
                "New sessions are not part of the demo".into(),
                Tone::Neutral,
            );
        }
        ui.add_space(6.0);
        let row = |s: &'static DemoSession| {
            SessionRow::new(s.key, s.title)
                .subtitle(s.subtitle)
                .status(s.status)
                .updated(UnixMs(self.now.0 - s.minutes_ago * MINUTE))
                .unread(s.unread)
        };
        let pinned: Vec<_> = SESSIONS.iter().filter(|s| s.pinned).map(row).collect();
        let today: Vec<_> = SESSIONS
            .iter()
            .filter(|s| s.today && !s.pinned)
            .map(row)
            .collect();
        let earlier: Vec<_> = SESSIONS.iter().filter(|s| !s.today).map(row).collect();
        let sections = [
            SessionSection {
                title: text("Today"),
                rows: &today,
                collapsed_by_default: false,
            },
            SessionSection {
                title: text("Earlier"),
                rows: &earlier,
                collapsed_by_default: false,
            },
        ];
        let menu = [
            MenuEntry {
                id: RowMenu::Rename,
                label: text("Rename"),
                icon: None,
                danger: false,
            },
            MenuEntry {
                id: RowMenu::Archive,
                label: text("Archive"),
                icon: None,
                danger: true,
            },
        ];
        let action = SessionList::new("chat_demo.sessions", &sections)
            .pinned(&pinned)
            .selected(Some(&self.selected))
            .search(&mut self.search)
            .row_menu(&menu)
            .time_format(
                &egui_theme_kit::chat::ClockFormat::default(),
                Some(self.now),
            )
            .show(ui);
        match action {
            Some(SessionListAction::Open(key)) => self.open(key),
            Some(SessionListAction::MentionInChat(key)) => {
                self.composer.insert_mention(mention_of(session(key)));
            }
            Some(SessionListAction::Menu { key, item }) => {
                let what = match item {
                    RowMenu::Rename => "Rename",
                    RowMenu::Archive => "Archive",
                };
                self.push_notice(format!("{what}: {}", session(key).title), Tone::Neutral);
            }
            None => {}
        }
    }

    fn open(&mut self, key: u32) {
        self.selected = key;
        self.title = session(key).title.to_owned();
        self.sidebar_narrow = false;
    }

    fn chat(&mut self, ui: &mut Ui) {
        let s = session(self.selected);
        let chips = [
            OptionChip::new(Ctl::Model, "Model", MODELS[self.model].clone())
                .icon(icons::BRAIN)
                .kind(ChipKind::Menu {
                    options: &MODELS,
                    selected: Some(self.model),
                }),
            OptionChip::new(Ctl::Access, "Access", ACCESS[self.access].clone())
                .icon(icons::SHIELD)
                .tone(if self.access == 2 {
                    Tone::Warn
                } else {
                    Tone::Neutral
                })
                .kind(ChipKind::Menu {
                    options: &ACCESS,
                    selected: Some(self.access),
                }),
            OptionChip::new(Ctl::Context, "Context", "38%").kind(ChipKind::Meter {
                fraction: 0.38,
                action: Some(text("Compact")),
            }),
            OptionChip::new(Ctl::AutoReview, "Auto review", "").kind(ChipKind::Toggle {
                on: self.auto_review,
            }),
            OptionChip::new(Ctl::Branch, "Branch", "feat/chat").icon(icons::FILE_DIFF),
        ];
        let buttons = [
            BarButton::new(Ctl::Files, icons::FILE_DIFF, "Changed files").badge(2),
            BarButton::new(Ctl::Terminal, icons::TERMINAL, "Terminal"),
        ];
        let mut bar = SessionBar::new("chat_demo.session_bar", self.title.as_str())
            .status(s.status, None)
            .subtitle(s.subtitle)
            .renamable(true)
            .chips(&chips)
            .buttons(&buttons)
            .nav(self.nav);
        if let Some(error) = &self.error {
            bar = bar.error(error.as_str());
        }
        let action = bar.show(ui);
        self.on_bar(action);
        ui.add_space(4.0);

        let rect = ui.available_rect_before_wrap();
        let column = rect.width().min(COLUMN_W);
        let composer_h = self.composer_h.clamp(1.0, rect.height() / 2.0);
        let composer_rect = Rect::from_min_max(
            pos2(rect.center().x - column / 2.0, rect.bottom() - composer_h),
            pos2(rect.center().x + column / 2.0, rect.bottom()),
        );
        let composer = ui.scope_builder(
            UiBuilder::new()
                .max_rect(composer_rect)
                .layout(Layout::top_down_justified(Align::Min)),
            |ui| self.composer(ui),
        );
        let measured = composer.response.rect.height();
        if (measured - self.composer_h).abs() > 0.5 {
            self.composer_h = measured;
            ui.ctx().request_discard("chat demo composer height");
        }
        if let Some(action) = composer.inner {
            self.on_composer(action);
        }
        let transcript_rect =
            Rect::from_min_max(rect.min, pos2(rect.right(), composer_rect.top() - 6.0));
        ui.scope_builder(
            UiBuilder::new()
                .max_rect(transcript_rect)
                .layout(Layout::top_down(Align::Min)),
            |ui| self.transcript(ui),
        );
    }

    fn transcript(&mut self, ui: &mut Ui) {
        let options = TranscriptOptions {
            detail: self.detail,
            show_thinking: self.thinking,
            max_width: Some(COLUMN_W),
            now: Some(self.now),
        };
        let mut transcript = Transcript::new("chat_demo.transcript")
            .options(options)
            .mentions(&Mentions)
            .mention_revision(0);
        if let Some(older) = self.older {
            transcript = transcript.older(older);
        }
        let response = transcript.show(ui, &mut self.transcript, &self.entries, |ui, key| {
            ui.label(format!("custom row {key}"));
        });
        self.nav = response.nav;
        for action in response.actions {
            self.on_chat(ui, action);
        }
    }

    fn composer(&mut self, ui: &mut Ui) -> Option<ComposerAction> {
        let running = self
            .entries
            .iter()
            .any(|e| matches!(e, ChatEntry::Work(w) if w.status == WorkStatus::Running));
        let activity = if running {
            Activity::Busy {
                enter: SendIntent::Steer,
                can_steer: true,
                can_queue: true,
            }
        } else {
            Activity::Idle
        };
        if std::mem::take(&mut self.focus_pending) {
            ui.memory_mut(|m| m.request_focus(Composer::text_id(Self::COMPOSER)));
        }
        let slash = slash_commands;
        Composer::new(Self::COMPOSER)
            .hint("Message the agent, @ to mention, / for commands")
            .mode(ComposerMode {
                activity,
                ..ComposerMode::default()
            })
            .attachments(&self.attachments)
            .enter(EnterBehavior::for_ui(ui))
            .slash(&slash)
            .mentions(&Mentions)
            .mention_revision(0)
            .plus(&PLUS)
            .show(ui, &mut self.composer)
            .action
    }

    fn board(&mut self, ui: &mut Ui) {
        let mine = self.waiting_cards();
        let other = [ChatEntry::Approval(ApprovalCard {
            key: text("ap-other"),
            context: text("LAB-02 \u{b7} docs"),
            title: text("Edit a file"),
            command: None,
            detail: json(json!({ "path": "docs/CHANGELOG.md", "lines_added": 14 })),
            reason: Some(text("Adds the 0.2 entry")),
            choices: CowList::Borrowed(&APPROVE),
            state: CardState::Open,
        })];
        let s = session(self.selected);
        let groups = [
            BoardGroup {
                session: self.selected,
                header: self.title.as_str().into(),
                subtitle: text(s.subtitle),
                status: s.status,
                cards: &mine,
            },
            BoardGroup {
                session: 3,
                header: text(session(3).title),
                subtitle: text(session(3).subtitle),
                status: session(3).status,
                cards: &other,
            },
        ];
        let action = WaitingBoard::new("chat_demo.board", &groups)
            .empty_text("Nothing is waiting on you")
            .show(ui);
        match action {
            Some(BoardAction::OpenSession(key)) => {
                self.open(key);
                self.view = View::Chats;
            }
            Some(BoardAction::Card { key, action, .. }) => self.on_card(&key, action),
            None => {}
        }
    }

    fn waiting_cards(&self) -> Vec<ChatEntry<'static>> {
        self.entries
            .iter()
            .filter(|e| match e {
                ChatEntry::Approval(c) => c.state.actionable(),
                ChatEntry::Question(c) => c.state.actionable(),
                _ => false,
            })
            .cloned()
            .collect()
    }

    fn on_bar(&mut self, action: Option<SessionBarAction<Ctl>>) {
        match action {
            Some(SessionBarAction::Rename(title)) => self.title = title,
            Some(SessionBarAction::Chip { id, choice }) => match id {
                Ctl::Model => self.model = choice,
                Ctl::Access => self.access = choice,
                _ => {}
            },
            Some(SessionBarAction::ChipAction(_)) => {
                self.push_notice("Context compacted \u{b7} 12% used".into(), Tone::Neutral);
            }
            Some(SessionBarAction::ChipToggle { on, .. }) => self.auto_review = on,
            Some(SessionBarAction::Button(id)) => {
                let what = if id == Ctl::Files {
                    "Changed files"
                } else {
                    "Terminal"
                };
                self.push_notice(format!("{what} would open here"), Tone::Neutral);
            }
            Some(SessionBarAction::Nav(step)) => self.transcript.jump(match step {
                NavStep::Prev => Jump::PrevUser,
                NavStep::Next => Jump::NextUser,
                NavStep::Pick(i) => Jump::User(i),
            }),
            Some(SessionBarAction::DismissError) => self.error = None,
            None => {}
        }
    }

    fn on_chat(&mut self, ui: &Ui, action: ChatAction) {
        match action {
            ChatAction::Copied(_) => {}
            ChatAction::OpenMention(target) => {
                if let Some(s) = SESSIONS.iter().find(|s| s.id == target.id) {
                    self.open(s.key);
                }
            }
            ChatAction::Link(url) => ui.ctx().open_url(egui::OpenUrl::new_tab(url)),
            ChatAction::Card { key, action } => self.on_card(&key, action),
            ChatAction::Resend(key) | ChatAction::EditResend(key) => {
                if let Some(ChatEntry::User(m)) = self.entries.iter().find(|e| e.key() == key) {
                    let wire = m.text.to_string();
                    self.composer.set_wire(&wire);
                }
            }
            ChatAction::LoadOlder => {
                let mut all = older_entries(self.now);
                all.append(&mut self.entries);
                self.entries = all;
                self.older = None;
            }
        }
    }

    fn on_card(&mut self, key: &str, action: CardAction) {
        let Some(entry) = self.entries.iter_mut().find(|e| e.key() == key) else {
            return;
        };
        let done = |outcome: &str, tone| CardState::Done {
            outcome: outcome.to_owned().into(),
            tone,
        };
        match (entry, action) {
            (ChatEntry::Approval(card), CardAction::Choose(i)) => {
                let label = card.choices.get(i).map_or("Chosen", |c| c.label.as_ref());
                let tone = if i + 1 == card.choices.len() {
                    Tone::Error
                } else {
                    Tone::Success
                };
                card.state = done(label, tone);
            }
            (ChatEntry::Question(card), CardAction::Answer(answers)) => {
                card.state = done(
                    &format!("Answered {} questions", answers.len()),
                    Tone::Success,
                );
            }
            (ChatEntry::Approval(card), CardAction::Dismiss) => {
                card.state = done("Dismissed", Tone::Neutral);
            }
            (ChatEntry::Question(card), CardAction::Dismiss) => {
                card.state = done("Dismissed", Tone::Neutral);
            }
            _ => {}
        }
    }

    fn on_composer(&mut self, action: ComposerAction) {
        match action {
            ComposerAction::Send { intent, message } => {
                self.next_key += 1;
                let delivery = match intent {
                    SendIntent::Start => Delivery::Sent,
                    SendIntent::Steer => Delivery::Steered,
                    SendIntent::Queue => Delivery::Queued,
                };
                self.entries.push(ChatEntry::User(UserMessage {
                    key: format!("u-sent-{}", self.next_key).into(),
                    text: message.wire.into(),
                    author: None,
                    time: Some(self.now),
                    attachments: CowList::Owned(std::mem::take(&mut self.attachments)),
                    delivery,
                }));
                self.composer.clear();
                self.transcript.stick_for(3);
            }
            ComposerAction::Stop => {
                for entry in &mut self.entries {
                    if let ChatEntry::Work(w) = entry
                        && w.status == WorkStatus::Running
                    {
                        w.status = WorkStatus::Declined;
                    }
                }
                self.push_notice("Stopped the running turn".into(), Tone::Warn);
            }
            ComposerAction::Attach => self.attachments.push(AttachmentChip {
                name: text("notes.txt"),
                image: None,
                reading: false,
            }),
            ComposerAction::RemoveAttachment(i) => {
                if i < self.attachments.len() {
                    self.attachments.remove(i);
                }
            }
            ComposerAction::Command { id, args } => {
                let line = if args.is_empty() {
                    format!("Ran /{id}")
                } else {
                    format!("Ran /{id} {args}")
                };
                self.composer.clear();
                self.push_notice(line, Tone::Info);
            }
            ComposerAction::Plus(id) => {
                self.push_notice(format!("Picked {id} from the + menu"), Tone::Neutral);
            }
        }
    }

    fn push_notice(&mut self, body: String, tone: Tone) {
        self.next_key += 1;
        let key = format!("n-demo-{}", self.next_key);
        self.entries.push(notice(key, body, tone, self.now));
        self.transcript.stick_for(3);
    }
}

/// A theme picker over the built-in presets.
fn theme_menu(ui: &mut Ui, preset: &mut PresetStyles) {
    let before = *preset;
    ComboBox::from_id_salt("chat_demo.theme")
        .selected_text(preset.as_str())
        .width(150.0)
        .show_ui(ui, |ui| {
            for p in egui_theme_kit::presets::MATCHABLE_PRESETS {
                ui.selectable_value(preset, p, p.as_str());
            }
        });
    if *preset != before {
        egui_theme_kit::apply_preset(ui.ctx(), *preset);
    }
}
