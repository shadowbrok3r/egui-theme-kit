//! Headless frames through the composer: typing, sending, popups, chips, copy, drop hover and layout.

use egui::{
    Context, DroppedFile, Event, FullOutput, HoveredFile, Key, Modifiers, OutputCommand,
    PointerButton, Pos2, RawInput, Rect, TouchDeviceId, TouchId, TouchPhase, ViewportId, vec2,
};

use super::*;
use crate::chat::mention::{MentionCandidate, MentionRef, Resolution, encode};

const SALT: &str = "test-composer";

#[derive(Debug)]
struct FakeFile(std::path::PathBuf);

impl DroppedFile for FakeFile {
    fn path(&self) -> &std::path::Path {
        &self.0
    }

    fn bytes(&self) -> Result<Vec<u8>, String> {
        Ok(Vec::new())
    }
}

fn key(key: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn text(t: &str) -> Event {
    Event::Text(t.to_owned())
}

fn candidates(_: &str, _: usize) -> Vec<MentionCandidate> {
    vec![
        MentionCandidate {
            target: MentionRef::new("session", "desk/1", "Fix scroll"),
            detail: "desk".into(),
            status: None,
            section: None,
        },
        MentionCandidate {
            target: MentionRef::new("session", "desk/2", "Other"),
            detail: String::new(),
            status: None,
            section: None,
        },
    ]
}

fn commands(_: &str) -> Vec<SlashCommand<'static>> {
    let compact = SlashCommand {
        id: "compact".into(),
        name: "/compact".into(),
        args: "".into(),
        about: "Shrink the context".into(),
        section: None,
        icon: None,
        enabled: true,
        run_on_pick: false,
    };
    vec![
        compact.clone(),
        SlashCommand {
            id: "permissions".into(),
            name: "/permissions".into(),
            args: "<mode>".into(),
            enabled: false,
            ..compact.clone()
        },
        SlashCommand {
            id: "steer".into(),
            name: "/steer".into(),
            args: "<message>".into(),
            ..compact
        },
    ]
}

/// Painted text sections whose background is the mention fill.
fn chip_sections(ctx: &Context, out: &FullOutput) -> usize {
    let fill = ChatPalette::get(ctx).mention_fill;
    out.shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Text(t) => Some(t),
            _ => None,
        })
        .map(|t| {
            t.galley
                .job
                .sections
                .iter()
                .filter(|s| s.format.background == fill)
                .count()
        })
        .sum()
}

/// A composer harness with fixed sources and options.
struct Harness {
    ctx: Context,
    state: ComposerState,
    width: f32,
    enter: EnterBehavior,
    editable: bool,
    activity: Activity,
    blocked: Option<&'static str>,
    stop_blocked: Option<&'static str>,
    raw: Option<RawInput>,
    height: f32,
    /// Clock of the next frame; `None` lets egui advance it.
    time: Option<f64>,
    clipboard: Option<String>,
}

impl Harness {
    fn new() -> Self {
        let mut h = Self {
            ctx: Context::default(),
            state: ComposerState::default(),
            width: 800.0,
            enter: EnterBehavior::Send,
            editable: true,
            activity: Activity::Idle,
            blocked: None,
            stop_blocked: None,
            raw: None,
            height: 600.0,
            time: None,
            clipboard: None,
        };
        h.frame(Vec::new());
        h.ctx
            .memory_mut(|m| m.request_focus(Composer::text_id(SALT)));
        h.frame(Vec::new());
        h
    }

    fn frame_full(&mut self, events: Vec<Event>) -> (ComposerResponse, FullOutput) {
        let mut raw = self.raw.take().unwrap_or_default();
        raw.screen_rect = Some(Rect::from_min_size(
            Pos2::ZERO,
            vec2(self.width, self.height),
        ));
        raw.events = events;
        if let Some(t) = self.time.as_mut() {
            raw.time = Some(*t);
            *t += 1.0 / 60.0;
        }
        let mut response = None;
        let (enter, editable, activity, blocked, stop_blocked) = (
            self.enter,
            self.editable,
            self.activity,
            self.blocked,
            self.stop_blocked,
        );
        let state = &mut self.state;
        let mentions = candidates;
        let slash = commands;
        let clip = self.clipboard.clone();
        let read = move || clip.clone();
        let has_clip = self.clipboard.is_some();
        let mut out = self.ctx.run_ui(raw, |ui| {
            let mut composer = Composer::new(SALT)
                .enter(enter)
                .mentions(&mentions)
                .slash(&slash)
                .mode(ComposerMode {
                    editable,
                    activity,
                    send_blocked: blocked.map(Into::into),
                    stop_blocked: stop_blocked.map(Into::into),
                });
            if has_clip {
                composer = composer.clipboard(&read);
            }
            response = Some(composer.show(ui, state));
        });
        out.textures_delta.clear();
        (response.expect("composer drawn"), out)
    }

    fn frame(&mut self, events: Vec<Event>) -> ComposerResponse {
        self.frame_full(events).0
    }

    /// Runs `events`, then one idle frame; returns the first action.
    fn act(&mut self, events: Vec<Event>) -> Option<ComposerAction> {
        let first = self.frame(events).action;
        let second = self.frame(Vec::new()).action;
        first.or(second)
    }
}

#[test]
fn the_text_edit_uses_the_fixed_id() {
    let mut h = Harness::new();
    let r = h.frame(Vec::new());
    assert_eq!(r.text.id, Composer::text_id(SALT));
    assert!(r.text.has_focus());
}

#[test]
fn enter_sends_and_shift_enter_breaks_the_line() {
    let mut h = Harness::new();
    h.frame(vec![text("hi")]);
    h.frame(vec![key(Key::Enter, Modifiers::SHIFT), text("there")]);
    assert_eq!(h.state.text(), "hi\nthere");
    let action = h.act(vec![key(Key::Enter, Modifiers::NONE)]);
    let Some(ComposerAction::Send { intent, message }) = action else {
        panic!("send expected, got {action:?}");
    };
    assert_eq!(intent, SendIntent::Start);
    assert_eq!(message.wire, "hi\nthere");
    assert!(h.state.is_empty());
}

#[test]
fn newline_mode_breaks_on_enter_and_sends_on_command_enter() {
    let mut h = Harness::new();
    h.enter = EnterBehavior::Newline;
    h.frame(vec![text("a"), key(Key::Enter, Modifiers::NONE), text("b")]);
    assert_eq!(h.state.text(), "a\nb");
    let action = h.act(vec![key(Key::Enter, Modifiers::COMMAND)]);
    assert!(
        matches!(action, Some(ComposerAction::Send { .. })),
        "{action:?}"
    );
}

#[test]
fn empty_text_does_not_send() {
    let mut h = Harness::new();
    assert_eq!(h.act(vec![key(Key::Enter, Modifiers::NONE)]), None);
}

#[test]
fn mentions_insert_atomic_chips_that_send_as_wire() {
    let mut h = Harness::new();
    h.frame(vec![text("ask @fi")]);
    h.frame(Vec::new());
    assert!(h.state.popup.is_some(), "popup should open");
    let r = h.frame(vec![key(Key::Enter, Modifiers::NONE)]);
    assert!(r.accepted_suggestion);
    h.frame(Vec::new());
    assert_eq!(h.state.text(), "ask @Fix scroll ");
    assert_eq!(h.state.spans().len(), 1);
    assert!(h.state.popup.is_none());

    let target = MentionRef::new("session", "desk/1", "Fix scroll");
    assert_eq!(h.state.message().wire, format!("ask {} ", encode(&target)));

    h.frame(vec![key(Key::Backspace, Modifiers::NONE)]);
    assert_eq!(h.state.text(), "ask @Fix scroll");
    h.frame(vec![key(Key::Backspace, Modifiers::NONE)]);
    assert_eq!(h.state.text(), "ask ");
    assert!(h.state.spans().is_empty());
}

#[test]
fn escape_dismisses_the_popup_until_the_trigger_changes() {
    let mut h = Harness::new();
    h.frame(vec![text("@f")]);
    h.frame(Vec::new());
    assert!(h.state.popup.is_some());
    h.frame(vec![key(Key::Escape, Modifiers::NONE)]);
    h.frame(vec![text("i")]);
    h.frame(Vec::new());
    assert!(h.state.popup.is_none());
    h.frame(vec![text(" @o")]);
    h.frame(Vec::new());
    assert!(h.state.popup.is_some());
}

#[test]
fn arrows_move_the_selection() {
    let mut h = Harness::new();
    h.frame(vec![text("@")]);
    h.frame(Vec::new());
    h.frame(vec![key(Key::ArrowDown, Modifiers::NONE)]);
    h.frame(vec![key(Key::Tab, Modifiers::NONE)]);
    h.frame(Vec::new());
    assert_eq!(h.state.text(), "@Other ");
}

#[test]
fn slash_commands_run_from_the_popup_and_unknown_ones_send_as_text() {
    let mut h = Harness::new();
    h.frame(vec![text("/comp")]);
    h.frame(Vec::new());
    let action = h.act(vec![key(Key::Enter, Modifiers::NONE)]);
    assert_eq!(
        action,
        Some(ComposerAction::Command {
            id: "compact".into(),
            args: String::new(),
        })
    );
    assert!(h.state.is_empty());

    h.frame(vec![text("/foo bar")]);
    let action = h.act(vec![key(Key::Enter, Modifiers::NONE)]);
    let Some(ComposerAction::Send { message, .. }) = action else {
        panic!("send expected, got {action:?}");
    };
    assert_eq!(message.wire, "/foo bar");
}

#[test]
fn pasted_wire_text_becomes_a_chip_and_copies_back_as_wire() {
    let mut h = Harness::new();
    let target = MentionRef::new("session", "desk/9", "Pasted");
    let wire = format!("see {}", encode(&target));
    h.frame(vec![Event::Paste(wire.clone())]);
    h.frame(Vec::new());
    assert_eq!(h.state.text(), "see @Pasted");
    assert_eq!(h.state.spans()[0].target, target);

    h.frame(vec![key(Key::A, Modifiers::COMMAND)]);
    let (_, out) = h.frame_full(vec![Event::Copy]);
    let copied: Vec<&String> = out
        .platform_output
        .commands
        .iter()
        .filter_map(|c| match c {
            OutputCommand::CopyText(t) => Some(t),
            _ => None,
        })
        .collect();
    assert_eq!(copied, vec![&wire]);
}

#[test]
fn undo_brings_a_deleted_chip_back() {
    let mut h = Harness::new();
    h.state.set_wire(&format!(
        "{} x",
        encode(&MentionRef::new("session", "1", "Chip"))
    ));
    h.frame(Vec::new());
    h.frame(vec![key(Key::ArrowLeft, Modifiers::NONE)]);
    h.frame(vec![key(Key::ArrowLeft, Modifiers::NONE)]);
    h.frame(vec![key(Key::Backspace, Modifiers::NONE)]);
    assert_eq!(h.state.text(), " x");
    assert!(h.state.spans().is_empty());
    h.frame(vec![key(Key::Z, Modifiers::COMMAND)]);
    h.frame(Vec::new());
    assert_eq!(h.state.text(), "@Chip x");
    assert_eq!(h.state.spans().len(), 1);
}

#[test]
fn read_only_text_ignores_typing() {
    let mut h = Harness::new();
    h.editable = false;
    h.frame(vec![text("nope")]);
    assert!(h.state.is_empty());
}

#[test]
fn dropped_files_are_reported_and_left_in_the_input() {
    let mut h = Harness::new();
    h.raw = Some(RawInput {
        hovered_files: vec![HoveredFile::default()],
        ..RawInput::default()
    });
    assert!(h.frame(Vec::new()).file_hover);
    h.raw = Some(RawInput {
        dropped_files: vec![std::sync::Arc::new(FakeFile(std::path::PathBuf::from(
            "a.txt",
        )))],
        ..RawInput::default()
    });
    let mut left = 0;
    let state = &mut h.state;
    let raw = h.raw.take().unwrap_or_default();
    let mut dropped = false;
    let mut out = h.ctx.run_ui(raw, |ui| {
        dropped = Composer::new(SALT).show(ui, state).files_dropped;
        left = ui.input(|i| i.raw.dropped_files.len());
    });
    out.textures_delta.clear();
    assert!(dropped);
    assert_eq!(left, 1);
}

#[test]
fn narrow_layout_puts_controls_above_a_taller_text_box() {
    let mut wide = Harness::new();
    let w = wide.frame(Vec::new());
    let mut narrow = Harness::new();
    narrow.width = 300.0;
    narrow.frame(Vec::new());
    let n = narrow.frame(Vec::new());
    let row = TextStyleRow::of(&narrow.ctx);
    assert!(
        n.text.rect.height() >= 2.5 * row,
        "{}",
        n.text.rect.height()
    );
    assert!(w.text.rect.height() < 2.0 * row, "{}", w.text.rect.height());
    assert!(n.text.rect.top() > w.text.rect.top());
}

#[test]
fn renamed_targets_relabel_and_unknown_ones_are_marked() {
    struct Renaming;
    impl MentionSource for Renaming {
        fn suggest(&self, _: &str, _: usize) -> Vec<MentionCandidate> {
            Vec::new()
        }
        fn resolve(&self, t: &MentionRef) -> Resolution {
            Resolution::found((t.id == "1").then(|| "Renamed".to_owned()))
        }
    }
    let ctx = Context::default();
    let mut state = ComposerState::default();
    let (a, b) = (
        MentionRef::new("session", "1", "Old"),
        MentionRef::new("session", "2", "Gone"),
    );
    state.set_wire(&format!("{} {}", encode(&a), encode(&b)));
    let mut out = ctx.run_ui(RawInput::default(), |ui| {
        let _ = Composer::new(SALT).mentions(&Renaming).show(ui, &mut state);
    });
    out.textures_delta.clear();
    assert_eq!(state.text(), "@Renamed @Gone");
    assert_eq!(state.spans()[1].chars, 9..14);
    let unresolved = resolve_labels(
        &ctx,
        Composer::text_id(SALT),
        &mut state,
        Some(&Renaming),
        0,
    );
    assert_eq!(unresolved, vec![false, true]);
}

#[test]
fn chip_layout_skips_spans_that_no_longer_match() {
    let p = ChatPalette::get(&Context::default());
    let font = egui::FontId::proportional(14.0);
    let target = MentionRef::new("session", "1", "ab");
    let spans = vec![MentionSpan {
        chars: 0..3,
        target,
    }];
    let job = chip_job("@ab x", &spans, &[false], &font, &p);
    assert_eq!(job.sections.len(), 3);
    assert_eq!(job.sections[1].format.background, p.mention_fill);
    let stale = chip_job("@ax x", &spans, &[false], &font, &p);
    assert!(
        stale
            .sections
            .iter()
            .all(|s| s.format.background != p.mention_fill)
    );
}

#[test]
fn busy_sends_steer_or_fall_back_to_queue() {
    let busy = |enter, can_steer, can_queue| Activity::Busy {
        enter,
        can_steer,
        can_queue,
    };
    assert_eq!(Activity::Idle.send_intent(), Some(SendIntent::Start));
    assert_eq!(
        busy(SendIntent::Steer, true, true).send_intent(),
        Some(SendIntent::Steer)
    );
    assert_eq!(
        busy(SendIntent::Steer, false, true).send_intent(),
        Some(SendIntent::Queue)
    );
    assert_eq!(busy(SendIntent::Queue, false, false).send_intent(), None);
}

/// Height of one body text row.
struct TextStyleRow;

impl TextStyleRow {
    fn of(ctx: &Context) -> f32 {
        let font = egui::TextStyle::Body.resolve(&ctx.global_style());
        ctx.fonts_mut(|f| f.row_height(&font))
    }
}

#[test]
fn a_pick_in_the_same_frame_as_typing_uses_the_new_query() {
    let mut h = Harness::new();
    h.frame(vec![text("@F")]);
    h.frame(Vec::new());
    assert!(h.state.popup.is_some());
    let r = h.frame(vec![text("ix"), key(Key::Enter, Modifiers::NONE)]);
    assert!(r.accepted_suggestion);
    h.frame(Vec::new());
    assert_eq!(h.state.text(), "@Fix scroll ");
    assert_eq!(h.state.spans()[0].chars, 0..11);
}

#[test]
fn modified_tabs_never_reach_the_text_while_the_popup_is_open() {
    let mut h = Harness::new();
    h.frame(vec![text("    @F")]);
    h.frame(Vec::new());
    assert!(h.state.popup.is_some());
    h.frame(vec![key(Key::Tab, Modifiers::SHIFT)]);
    assert_eq!(h.state.text(), "    @F");
    h.frame(vec![key(Key::Tab, Modifiers::CTRL)]);
    assert_eq!(h.state.text(), "    @F");
    assert!(h.state.popup.is_some());
}

#[test]
fn typed_and_picked_commands_run_alike_while_sending_is_blocked() {
    let mut h = Harness::new();
    h.blocked = Some("Reconnecting");
    h.frame(vec![text("/compact now")]);
    let action = h.act(vec![key(Key::Enter, Modifiers::NONE)]);
    assert_eq!(
        action,
        Some(ComposerAction::Command {
            id: "compact".into(),
            args: "now".into(),
        })
    );
    h.frame(vec![text("/comp")]);
    h.frame(Vec::new());
    let action = h.act(vec![key(Key::Enter, Modifiers::NONE)]);
    assert!(
        matches!(action, Some(ComposerAction::Command { .. })),
        "{action:?}"
    );
    h.frame(vec![text("hello")]);
    assert_eq!(h.act(vec![key(Key::Enter, Modifiers::NONE)]), None);
    assert_eq!(h.state.text(), "hello");
}

#[test]
fn a_disabled_command_is_neither_run_nor_sent() {
    let mut h = Harness::new();
    h.frame(vec![text("/permissions full")]);
    assert_eq!(h.act(vec![key(Key::Enter, Modifiers::NONE)]), None);
    assert_eq!(h.state.text(), "/permissions full");
}

#[test]
fn alt_enter_sends_with_the_other_busy_intent() {
    let mut h = Harness::new();
    h.activity = Activity::Busy {
        enter: SendIntent::Steer,
        can_steer: true,
        can_queue: true,
    };
    h.frame(vec![text("later")]);
    let action = h.act(vec![key(Key::Enter, Modifiers::ALT)]);
    let Some(ComposerAction::Send { intent, .. }) = action else {
        panic!("send expected, got {action:?}");
    };
    assert_eq!(intent, SendIntent::Queue);
    h.frame(vec![text("now")]);
    let action = h.act(vec![key(Key::Enter, Modifiers::NONE)]);
    assert!(
        matches!(
            action,
            Some(ComposerAction::Send {
                intent: SendIntent::Steer,
                ..
            })
        ),
        "{action:?}"
    );
    assert_eq!(h.activity.alternate_intent(), Some(SendIntent::Queue));
    let steer_only = Activity::Busy {
        enter: SendIntent::Steer,
        can_steer: true,
        can_queue: false,
    };
    assert_eq!(steer_only.alternate_intent(), None);
    assert_eq!(Activity::Idle.alternate_intent(), None);
}

#[test]
fn chips_after_the_cursor_keep_their_style_on_the_typing_frame() {
    let mut h = Harness::new();
    h.state
        .insert_mention(MentionRef::new("session", "desk/1", "Fix scroll"));
    h.frame(Vec::new());
    let (_, idle) = h.frame_full(Vec::new());
    assert_eq!(chip_sections(&h.ctx, &idle), 1);
    h.frame(vec![key(Key::Home, Modifiers::NONE)]);
    let (_, typing) = h.frame_full(vec![text("x")]);
    assert_eq!(h.state.text(), "x@Fix scroll ");
    assert_eq!(chip_sections(&h.ctx, &typing), 1);
}

#[test]
fn app_inserted_and_restored_chips_stay_live_with_a_suggest_only_source() {
    let ctx = Context::default();
    let mut state = ComposerState::default();
    let source = candidates;
    state.set_wire(&encode(&MentionRef::new("session", "x", "Restored")));
    state.insert_mention(MentionRef::new("session", "y", "Inserted"));
    let unresolved = resolve_labels(&ctx, Composer::text_id(SALT), &mut state, Some(&source), 0);
    assert_eq!(unresolved, vec![false, false]);
    assert!(state.was_picked(&MentionRef::new("session", "y", "Inserted")));
}

#[test]
fn files_without_a_pointer_go_to_one_composer() {
    let ctx = Context::default();
    let (mut a, mut b) = (ComposerState::default(), ComposerState::default());
    let mut frame = |focus: Option<&str>| {
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
            hovered_files: vec![HoveredFile::default()],
            ..RawInput::default()
        };
        if let Some(salt) = focus {
            ctx.memory_mut(|m| m.request_focus(Composer::text_id(salt)));
        }
        let mut hover = (false, false);
        let mut out = ctx.run_ui(raw, |ui| {
            hover.0 = Composer::new("drop-a").show(ui, &mut a).file_hover;
            hover.1 = Composer::new("drop-b").show(ui, &mut b).file_hover;
        });
        out.textures_delta.clear();
        hover
    };
    assert_eq!(frame(None), (true, false));
    frame(Some("drop-b"));
    assert_eq!(frame(None), (false, true));
}

fn finger(phase: TouchPhase, pos: Pos2) -> Event {
    Event::Touch {
        device_id: TouchDeviceId(1),
        id: TouchId(1),
        phase,
        pos,
        force: None,
    }
}

fn button(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

/// Forty numbered lines, taller than the text box.
fn long_text() -> String {
    (0..40)
        .map(|n| format!("line {n:02}"))
        .collect::<Vec<_>>()
        .join("\n")
}

impl Harness {
    /// A touch screen harness holding `text`, unfocused and without a stored cursor.
    fn touch(text: &str) -> Self {
        let mut h = Self {
            ctx: Context::default(),
            state: ComposerState::default(),
            width: 800.0,
            enter: EnterBehavior::Newline,
            editable: true,
            activity: Activity::Idle,
            blocked: None,
            stop_blocked: None,
            raw: None,
            height: 600.0,
            time: Some(0.0),
            clipboard: None,
        };
        h.state.buffer_mut().push_str(text);
        h.frame(vec![finger(TouchPhase::Cancel, Pos2::ZERO)]);
        h.idle(3);
        h
    }

    fn idle(&mut self, frames: usize) -> ComposerResponse {
        let mut last = self.frame(Vec::new());
        for _ in 1..frames {
            last = self.frame(Vec::new());
        }
        last
    }

    fn press(&mut self, pos: Pos2) {
        self.frame(vec![
            finger(TouchPhase::Start, pos),
            Event::PointerMoved(pos),
            button(pos, true),
        ]);
    }

    fn drag_to(&mut self, pos: Pos2) {
        self.frame(vec![
            finger(TouchPhase::Move, pos),
            Event::PointerMoved(pos),
        ]);
    }

    fn release(&mut self, pos: Pos2) -> FullOutput {
        self.frame_full(vec![
            finger(TouchPhase::End, pos),
            button(pos, false),
            Event::PointerGone,
        ])
        .1
    }

    fn tap(&mut self, pos: Pos2) -> FullOutput {
        self.press(pos);
        self.release(pos)
    }

    fn cursor(&self) -> Option<(usize, usize)> {
        cursor_of(&self.ctx, Composer::text_id(SALT))
    }

    fn focused(&self) -> bool {
        self.ctx.memory(|m| m.has_focus(Composer::text_id(SALT)))
    }

    /// Centre of the text action button for `action`.
    fn action(&self, action: TextAction) -> Pos2 {
        self.state
            .touch
            .buttons
            .iter()
            .find(|(a, _)| *a == action)
            .map(|(_, r)| r.center())
            .expect("action row drawn")
    }

    fn cursor_in_view(&self) -> bool {
        let t = &self.state.touch;
        t.view.expand(2.0).contains_rect(t.cursor)
    }
}

fn copied(out: &FullOutput) -> Vec<String> {
    out.platform_output
        .commands
        .iter()
        .filter_map(|c| match c {
            OutputCommand::CopyText(t) => Some(t.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_first_tap_puts_the_cursor_at_the_end_and_later_taps_where_they_land() {
    let text = long_text();
    let len = text.chars().count();
    let mut h = Harness::touch(&text);
    assert!(h.state.touch.scroll_max > 0.0);
    assert_eq!(h.state.touch.scroll_y, 0.0);
    let view = h.state.touch.view;
    h.tap(view.center());
    h.idle(3);
    assert!(h.focused());
    assert_eq!(h.cursor(), Some((len, len)));
    assert!(
        (h.state.touch.scroll_y - h.state.touch.scroll_max).abs() < 1.0,
        "{} of {}",
        h.state.touch.scroll_y,
        h.state.touch.scroll_max
    );
    assert!(h.cursor_in_view());

    h.tap(h.state.touch.view.center());
    h.idle(2);
    let (a, b) = h.cursor().expect("cursor");
    assert_eq!(a, b);
    assert!(b < len, "{b}");
}

#[test]
fn a_drag_on_the_focused_text_scrolls_it_and_keeps_the_cursor() {
    let mut h = Harness::touch(&long_text());
    h.tap(h.state.touch.view.center());
    h.idle(3);
    let before = h.cursor();
    let top = h.state.touch.scroll_y;
    let p = h.state.touch.view.center();
    h.press(p);
    for dy in [20.0, 60.0, 100.0] {
        h.drag_to(p + vec2(0.0, dy));
    }
    assert!(
        (top - h.state.touch.scroll_y - 100.0).abs() < 1.0,
        "{top} -> {}",
        h.state.touch.scroll_y
    );
    h.release(p + vec2(0.0, 100.0));
    h.idle(2);
    assert_eq!(h.cursor(), before);
    assert!(h.focused());
}

#[test]
fn a_held_press_selects_instead_of_scrolling() {
    let mut h = Harness::touch(&long_text());
    h.tap(h.state.touch.view.center());
    h.idle(3);
    let top = h.state.touch.scroll_y;
    let p = h.state.touch.view.center();
    h.press(p);
    h.idle(25);
    assert!(matches!(h.state.touch.gesture, Gesture::Holding { .. }));
    h.drag_to(p - vec2(0.0, 40.0));
    h.release(p - vec2(0.0, 40.0));
    h.idle(2);
    let (a, b) = h.cursor().expect("cursor");
    assert_ne!(a, b);
    assert!((h.state.touch.scroll_y - top).abs() < 40.0);
}

#[test]
fn a_press_right_after_a_tap_drag_selects() {
    let mut h = Harness::touch(&long_text());
    h.tap(h.state.touch.view.center());
    h.idle(3);
    let top = h.state.touch.scroll_y;
    let p = h.state.touch.view.center();
    h.tap(p);
    h.press(p);
    assert_eq!(h.state.touch.gesture, Gesture::Selecting);
    h.drag_to(p + vec2(30.0, 0.0));
    h.release(p + vec2(30.0, 0.0));
    h.idle(1);
    let (a, b) = h.cursor().expect("cursor");
    assert_ne!(a, b);
    assert_eq!(h.state.touch.scroll_y, top);
}

#[test]
fn drags_outside_the_text_still_reach_other_scroll_areas() {
    let ctx = Context::default();
    let mut state = ComposerState::default();
    state.buffer_mut().push_str(&long_text());
    let mut offset = 0.0;
    let mut time = 0.0;
    let mut frame = |events: Vec<Event>| {
        time += 1.0 / 60.0;
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
            time: Some(time),
            events,
            ..RawInput::default()
        };
        let mut out = ctx.run_ui(raw, |ui| {
            offset = egui::ScrollArea::vertical()
                .id_salt("above")
                .auto_shrink(false)
                .max_height(200.0)
                .show(ui, |ui| {
                    for n in 0..60 {
                        ui.label(format!("row {n}"));
                    }
                })
                .state
                .clamped_offset()
                .y;
            let _ = Composer::new(SALT).show(ui, &mut state);
        });
        out.textures_delta.clear();
    };
    frame(vec![finger(TouchPhase::Cancel, Pos2::ZERO)]);
    ctx.memory_mut(|m| m.request_focus(Composer::text_id(SALT)));
    frame(Vec::new());
    let p = Pos2::new(100.0, 150.0);
    frame(vec![
        finger(TouchPhase::Start, p),
        Event::PointerMoved(p),
        button(p, true),
    ]);
    for dy in [20.0, 50.0, 80.0] {
        let q = p - vec2(0.0, dy);
        frame(vec![finger(TouchPhase::Move, q), Event::PointerMoved(q)]);
    }
    assert!(offset > 50.0, "{offset}");
    assert_eq!(state.touch.gesture, Gesture::Idle);
}

#[test]
fn the_cursor_stays_in_view_as_text_grows_and_the_box_shrinks() {
    let mut h = Harness::touch("start");
    h.tap(h.state.touch.view.center());
    h.idle(2);
    for n in 0..30 {
        h.frame(vec![text(&format!("\nrow {n}"))]);
        h.idle(1);
        assert!(h.cursor_in_view(), "row {n}");
    }
    assert!(h.state.touch.scroll_max > 0.0);

    let p = h.state.touch.view.center();
    h.press(p);
    for dy in [30.0, 80.0, 150.0] {
        h.drag_to(p + vec2(0.0, dy));
    }
    h.release(p + vec2(0.0, 150.0));
    h.idle(1);
    assert!(!h.cursor_in_view());

    h.height = 360.0;
    h.idle(3);
    assert!(h.cursor_in_view());
    h.height = 600.0;
    h.idle(3);
    assert!(h.cursor_in_view());
}

#[test]
fn touch_shows_the_action_row_while_focused() {
    let mut h = Harness::touch("hello");
    assert!(!h.frame(Vec::new()).provides_text_actions);
    h.tap(h.state.touch.view.center());
    let r = h.idle(2);
    assert!(r.provides_text_actions);
    assert_eq!(h.state.touch.buttons.len(), 4);
    let view = h.state.touch.view;
    assert!(
        h.state
            .touch
            .buttons
            .iter()
            .all(|(_, b)| !b.intersects(view))
    );

    let mut desk = Harness::new();
    assert!(!desk.frame(Vec::new()).provides_text_actions);
    assert!(desk.state.touch.buttons.is_empty());
}

#[test]
fn a_narrow_touch_composer_gives_the_actions_their_own_row() {
    let mut h = Harness::touch("hello");
    h.width = 300.0;
    h.idle(2);
    h.tap(h.state.touch.view.center());
    h.idle(3);
    let t = &h.state.touch;
    assert_eq!(t.buttons.len(), 4);
    for (_, b) in &t.buttons {
        assert!(!b.intersects(t.view), "{b:?} over {:?}", t.view);
        assert!(b.right() <= 300.0, "{b:?}");
    }
}

#[test]
fn action_buttons_select_copy_cut_and_paste_with_chips_as_wire() {
    let target = MentionRef::new("session", "desk/9", "Pasted");
    let wire = format!("see {}", encode(&target));
    let mut h = Harness::touch("");
    h.state.set_wire(&wire);
    h.idle(2);
    h.tap(h.state.touch.view.center());
    h.idle(2);
    assert_eq!(h.state.text(), "see @Pasted");

    h.tap(h.action(TextAction::SelectAll));
    h.idle(1);
    assert!(h.focused(), "a button tap keeps focus");
    assert_eq!(h.cursor(), Some((0, 11)));

    let out = h.tap(h.action(TextAction::Copy));
    assert_eq!(copied(&out), vec![wire.clone()]);
    assert_eq!(h.state.text(), "see @Pasted");

    h.idle(1);
    let out = h.tap(h.action(TextAction::Cut));
    assert_eq!(copied(&out), vec![wire.clone()]);
    h.idle(1);
    assert!(h.state.text().is_empty());
    assert!(h.state.spans().is_empty());
    assert!(h.focused());

    h.clipboard = Some(wire.clone());
    h.tap(h.action(TextAction::Paste));
    h.idle(2);
    assert_eq!(h.state.text(), "see @Pasted");
    assert_eq!(h.state.spans()[0].target, target);
    assert_eq!(h.state.message().wire, wire);
}

#[test]
fn paste_without_a_clipboard_reader_asks_the_integration() {
    let mut h = Harness::touch("x");
    h.tap(h.state.touch.view.center());
    h.idle(2);
    let out = h.tap(h.action(TextAction::Paste));
    let asked = out.viewport_output[&ViewportId::ROOT]
        .commands
        .iter()
        .any(|c| matches!(c, egui::ViewportCommand::RequestPaste));
    assert!(asked);
}

/// Clickable square widgets `side` wide drawn last frame, left to right, with whether each is enabled.
fn squares(ctx: &Context, side: f32) -> Vec<(Rect, bool)> {
    let mut out: Vec<(Rect, bool)> = ctx.viewport(|v| {
        v.prev_pass
            .widgets
            .layers()
            .flat_map(|(_, w)| w.iter())
            .filter(|w| w.sense.senses_click())
            .filter(|w| (w.rect.width() - side).abs() < 0.5 && (w.rect.height() - side).abs() < 0.5)
            .map(|w| (w.rect, w.enabled))
            .collect()
    });
    out.sort_by(|a, b| a.0.left().total_cmp(&b.0.left()));
    out
}

fn painted_text(out: &FullOutput) -> Vec<String> {
    out.shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_blocked_stop_draws_disabled_with_its_reason_and_reports_nothing() {
    let busy = Activity::Busy {
        enter: SendIntent::Steer,
        can_steer: true,
        can_queue: false,
    };
    let stop_of = |h: &mut Harness| {
        h.frame(Vec::new());
        h.frame(Vec::new());
        let side = Composer::control_box(&h.ctx, false).side(&Metrics::get(&h.ctx));
        let found = squares(&h.ctx, side);
        assert!(found.len() >= 2, "{found:?}");
        found[found.len() - 2]
    };

    let mut free = Harness::new();
    free.activity = busy;
    let (rect, enabled) = stop_of(&mut free);
    assert!(enabled);
    let at = rect.center();
    let action = free.act(vec![
        Event::PointerMoved(at),
        button(at, true),
        button(at, false),
    ]);
    assert_eq!(action, Some(ComposerAction::Stop));

    let mut held = Harness::new();
    held.activity = busy;
    held.stop_blocked = Some("Waiting for the relay");
    held.ctx.all_styles_mut(|s| {
        s.interaction.tooltip_delay = 0.0;
        s.interaction.show_tooltips_only_when_still = false;
    });
    let (rect, enabled) = stop_of(&mut held);
    assert!(!enabled, "Stop draws disabled");
    let at = rect.center();
    let mut shown = Vec::new();
    for _ in 0..3 {
        shown = painted_text(&held.frame_full(vec![Event::PointerMoved(at)]).1);
    }
    assert!(
        shown.iter().any(|t| t == "Waiting for the relay"),
        "{shown:?}"
    );
    let action = held.act(vec![button(at, true), button(at, false)]);
    assert_eq!(action, None);
}

#[test]
fn attachment_thumbnails_crop_to_a_centred_square() {
    let ctx = Context::default();
    let image = |w, h| egui::ColorImage::filled([w, h], egui::Color32::RED);
    let wide = ctx.load_texture("wide", image(40, 20), egui::TextureOptions::default());
    let tall = ctx.load_texture("tall", image(20, 40), egui::TextureOptions::default());
    let unknown = egui::TextureId::User(7);
    let chips = [
        AttachmentChip {
            image: Some(wide.id()),
            ..AttachmentChip::new("wide.png")
        },
        AttachmentChip {
            image: Some(tall.id()),
            ..AttachmentChip::new("tall.png")
        },
        AttachmentChip {
            image: Some(unknown),
            ..AttachmentChip::new("user.png")
        },
    ];
    let mut state = ComposerState::default();
    let raw = || RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
        ..RawInput::default()
    };
    let mut out = ctx.run_ui(raw(), |ui| {
        Composer::new(SALT).attachments(&chips).show(ui, &mut state);
    });
    out.textures_delta.clear();
    let uv_of = |id: egui::TextureId| {
        out.shapes.iter().find_map(|c| match &c.shape {
            egui::Shape::Rect(r) => r
                .brush
                .as_ref()
                .filter(|b| b.fill_texture_id == id)
                .map(|b| b.uv),
            _ => None,
        })
    };
    let rect = |x0, y0, x1, y1| Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1));
    assert_eq!(uv_of(wide.id()), Some(rect(0.25, 0.0, 0.75, 1.0)));
    assert_eq!(uv_of(tall.id()), Some(rect(0.0, 0.25, 1.0, 0.75)));
    assert_eq!(uv_of(unknown), Some(rect(0.0, 0.0, 1.0, 1.0)));
}

#[test]
fn control_box_sizes_app_buttons_like_the_composer_buttons() {
    for touch in [false, true] {
        let ctx = Context::default();
        let side = Composer::control_box(&ctx, touch).side(&Metrics::get(&ctx));
        assert!(side >= Composer::control_box(&ctx, false).side(&Metrics::get(&ctx)));
        let mut state = ComposerState::default();
        let mut placed = None;
        for _ in 0..2 {
            let raw = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
                ..RawInput::default()
            };
            let mut out = ctx.run_ui(raw, |ui| {
                Composer::new(SALT)
                    .touch(touch)
                    .trailing(|ui| {
                        let size = Composer::control_box(ui.ctx(), touch);
                        placed = Some(
                            IconButton::new(crate::controls::icons::X, size)
                                .show(ui)
                                .rect,
                        );
                    })
                    .show(ui, &mut state);
            });
            out.textures_delta.clear();
        }
        let placed = placed.expect("trailing drawn");
        let found = squares(&ctx, side);
        let send = found.last().expect("send drawn").0;
        assert_eq!(placed.size(), send.size(), "touch {touch}");
        assert!(
            (placed.center().y - send.center().y).abs() < 0.5,
            "touch {touch}"
        );
    }
}

impl Harness {
    /// Opens the `+` menu as its button does.
    fn open_plus(&mut self) {
        self.state.plus.open();
        self.state.refocus = true;
    }

    /// Opens the `+` menu, searches it for `query` and picks the best match.
    fn pick_plus(&mut self, query: &str) {
        self.open_plus();
        self.frame(Vec::new());
        self.frame(vec![text(query)]);
        self.frame(vec![key(Key::Enter, Modifiers::NONE)]);
        assert!(!self.state.plus.is_open(), "the pick closes the menu");
    }
}

#[test]
fn a_plus_menu_pick_keeps_the_text_focused_with_the_cursor_after_the_command() {
    let mut h = Harness::new();
    h.frame(vec![text("fix this")]);
    h.pick_plus("steer");
    assert_eq!(h.state.text(), "/steer fix this");
    for frame in 0..4 {
        h.frame(Vec::new());
        assert!(h.focused(), "frame {frame} after the pick");
        assert_eq!(h.cursor(), Some((7, 7)), "frame {frame} after the pick");
    }
}

#[test]
fn mention_from_the_plus_menu_keeps_focus_and_opens_the_popup() {
    let mut h = Harness::new();
    h.pick_plus("mention");
    assert_eq!(h.state.text(), "@");
    h.idle(3);
    assert!(h.focused());
    assert!(h.state.popup.is_some(), "the mention popup opens");
}

#[test]
fn a_plus_menu_pick_on_touch_keeps_the_cursor_where_the_pick_put_it() {
    for host_repins in [false, true] {
        let mut h = Harness::touch("fix this");
        h.tap(h.state.touch.view.center());
        h.idle(3);
        assert_eq!(h.cursor(), Some((8, 8)));
        h.pick_plus("steer");
        assert_eq!(h.state.text(), "/steer fix this");
        for frame in 0..4 {
            if host_repins && !h.focused() {
                h.ctx
                    .memory_mut(|m| m.request_focus(Composer::text_id(SALT)));
            }
            h.frame(Vec::new());
            assert!(h.focused(), "host {host_repins}, frame {frame}");
            assert_eq!(
                h.cursor(),
                Some((7, 7)),
                "host {host_repins}, frame {frame}"
            );
        }
    }
}

/// Prose that wraps into many rows with no line breaks.
fn long_prose() -> String {
    "the quick brown fox jumps over the lazy dog ".repeat(60)
}

fn mouse_press(h: &mut Harness, pos: Pos2) {
    h.frame(vec![Event::PointerMoved(pos), button(pos, true)]);
}

fn mouse_release(h: &mut Harness, pos: Pos2) {
    h.frame(vec![button(pos, false)]);
}

#[test]
fn a_hold_past_the_long_touch_time_still_drag_selects() {
    let mut h = Harness::touch(&long_text());
    h.tap(h.state.touch.view.center());
    h.idle(3);
    let top = h.state.touch.scroll_y;
    let p = h.state.touch.view.center();
    h.press(p);
    h.idle(70);
    let Gesture::Holding {
        word: Some((ws, we)),
        ..
    } = h.state.touch.gesture
    else {
        panic!("{:?}", h.state.touch.gesture);
    };
    assert!(we > ws, "the held word is selected");
    assert_eq!(h.cursor(), Some((ws, we)));
    for dy in [20.0, 40.0, 60.0] {
        h.drag_to(p - vec2(0.0, dy));
    }
    h.release(p - vec2(0.0, 60.0));
    h.idle(2);
    let (a, b) = h.cursor().expect("cursor");
    assert_eq!(a, we);
    assert!(b < ws, "{a} {b}");
    assert!((h.state.touch.scroll_y - top).abs() < 1.0);
}

#[test]
fn a_press_keeps_the_cursor_until_it_lifts_as_a_tap() {
    let text = long_text();
    let len = text.chars().count();
    let mut h = Harness::touch(&text);
    h.tap(h.state.touch.view.center());
    h.idle(3);
    assert_eq!(h.cursor(), Some((len, len)));
    let p = h.state.touch.view.center();
    h.press(p);
    assert_eq!(h.cursor(), Some((len, len)));
    h.drag_to(p);
    assert_eq!(h.cursor(), Some((len, len)));
    h.release(p);
    let (a, b) = h.cursor().expect("cursor");
    assert_eq!(a, b);
    assert!(b < len, "{b}");
}

#[test]
fn the_tap_that_focuses_reports_the_actions_on_the_same_frame() {
    let mut h = Harness::touch("hello");
    let p = h.state.touch.view.center();
    let r = h.frame(vec![
        finger(TouchPhase::Start, p),
        Event::PointerMoved(p),
        button(p, true),
    ]);
    assert_eq!(r.provides_text_actions, h.focused());
    let r = h
        .frame_full(vec![
            finger(TouchPhase::End, p),
            button(p, false),
            Event::PointerGone,
        ])
        .0;
    assert!(h.focused());
    assert!(r.provides_text_actions);
    h.ctx
        .memory_mut(|m| m.surrender_focus(Composer::text_id(SALT)));
    let r = h.frame(Vec::new());
    assert!(!h.focused());
    assert!(!r.provides_text_actions);
}

#[test]
fn focus_from_the_app_keeps_the_cursor() {
    let text = long_text();
    let len = text.chars().count();
    let mut h = Harness::touch(&text);
    h.tap(h.state.touch.view.center());
    h.idle(3);
    h.tap(h.state.touch.view.center());
    h.idle(2);
    let placed = h.cursor().expect("cursor");
    assert!(placed.1 < len);
    let id = Composer::text_id(SALT);
    h.ctx.memory_mut(|m| m.surrender_focus(id));
    h.idle(2);
    assert!(!h.focused());
    h.ctx.memory_mut(|m| m.request_focus(id));
    h.idle(2);
    assert!(h.focused());
    assert_eq!(h.cursor(), Some(placed));
}

#[test]
fn a_first_tap_on_wrapped_prose_keeps_the_end_in_view_as_the_box_narrows() {
    let text = long_prose();
    let len = text.chars().count();
    let mut h = Harness::touch(&text);
    let wide = h.state.touch.view.width();
    h.tap(h.state.touch.view.center());
    h.idle(5);
    assert!(
        h.state.touch.view.width() < wide - 50.0,
        "the action row narrows the text"
    );
    assert_eq!(h.cursor(), Some((len, len)));
    assert!(
        h.cursor_in_view(),
        "{:?} in {:?}",
        h.state.touch.cursor,
        h.state.touch.view
    );
}

#[test]
fn a_mouse_after_touch_selects_and_places_the_cursor_like_a_desktop() {
    let text = long_text();
    let len = text.chars().count();
    let mut h = Harness::touch(&text);
    store_cursor(&h.ctx, Composer::text_id(SALT), 5, 5, None);
    let p = h.state.touch.view.center();
    mouse_press(&mut h, p);
    mouse_release(&mut h, p);
    let r = h.idle(2);
    assert!(h.focused());
    assert_ne!(h.cursor(), Some((len, len)));
    assert!(!r.provides_text_actions);

    let top = h.state.touch.scroll_y;
    mouse_press(&mut h, p);
    for dy in [20.0, 40.0, 60.0] {
        h.frame(vec![Event::PointerMoved(p - vec2(0.0, dy))]);
    }
    mouse_release(&mut h, p - vec2(0.0, 60.0));
    h.idle(1);
    let (a, b) = h.cursor().expect("cursor");
    assert_ne!(a, b);
    assert_eq!(h.state.touch.gesture, Gesture::Idle);
    assert!((h.state.touch.scroll_y - top).abs() < 1.0);
}

#[test]
fn the_narrow_action_row_stays_put_when_the_activity_changes() {
    let mut h = Harness::touch("hello");
    h.width = 380.0;
    h.idle(2);
    h.tap(h.state.touch.view.center());
    h.idle(3);
    let view = h.state.touch.view;
    let above = |h: &Harness| {
        h.state
            .touch
            .buttons
            .iter()
            .all(|(_, b)| b.bottom() <= h.state.touch.view.top())
    };
    assert_eq!(h.state.touch.buttons.len(), 4);
    assert!(above(&h));
    for activity in [
        Activity::Busy {
            enter: SendIntent::Steer,
            can_steer: true,
            can_queue: true,
        },
        Activity::Idle,
    ] {
        h.activity = activity;
        for _ in 0..3 {
            h.frame(Vec::new());
            assert!(
                (h.state.touch.view.top() - view.top()).abs() < 0.5,
                "{activity:?}"
            );
            assert_eq!(h.state.touch.buttons.len(), 4);
        }
    }
}

#[test]
fn back_closes_the_plus_menu_and_a_surrendered_query_stays_unfocused() {
    let mut h = Harness::touch("fix this");
    h.tap(h.state.touch.view.center());
    h.idle(3);
    h.open_plus();
    h.idle(2);
    let query = h.ctx.memory(|m| m.focused()).expect("the query is focused");
    assert_ne!(query, Composer::text_id(SALT));
    h.ctx.memory_mut(|m| m.surrender_focus(query));
    h.idle(3);
    assert_eq!(
        h.ctx.memory(|m| m.focused()),
        None,
        "focus stays surrendered"
    );
    assert!(h.state.plus.is_open());
    h.frame(vec![key(Key::BrowserBack, Modifiers::NONE)]);
    assert!(!h.state.plus.is_open(), "back closes the menu");
    h.idle(3);
    assert_eq!(
        h.ctx.memory(|m| m.focused()),
        None,
        "the text is not refocused"
    );
    assert_eq!(h.state.text(), "fix this");
}

#[test]
fn a_flick_on_the_focused_text_coasts_after_it_lifts() {
    let mut h = Harness::touch(&long_text());
    h.tap(h.state.touch.view.center());
    h.idle(3);
    let p = h.state.touch.view.center() - vec2(0.0, 40.0);
    h.press(p);
    for dy in [30.0, 60.0, 90.0] {
        h.drag_to(p + vec2(0.0, dy));
    }
    h.release(p + vec2(0.0, 90.0));
    let lifted = h.state.touch.scroll_y;
    assert!(h.state.touch.coasting);
    h.idle(5);
    assert!(
        h.state.touch.scroll_y < lifted - 5.0,
        "{lifted} -> {}",
        h.state.touch.scroll_y
    );
    h.idle(120);
    assert!(!h.state.touch.coasting);
    h.press(h.state.touch.view.center());
    h.release(h.state.touch.view.center());
    assert!(!h.state.touch.coasting);
}

impl Harness {
    /// Focuses the touch text, flicks it down and lifts; returns the cursor before the flick.
    fn flick(&mut self) -> Option<(usize, usize)> {
        self.tap(self.state.touch.view.center());
        self.idle(3);
        let before = self.cursor();
        let p = self.state.touch.view.center() - vec2(0.0, 40.0);
        self.press(p);
        for dy in [30.0, 60.0, 90.0] {
            self.drag_to(p + vec2(0.0, dy));
        }
        self.release(p + vec2(0.0, 90.0));
        assert!(self.state.touch.coasting);
        before
    }
}

#[test]
fn a_flick_that_rests_before_lifting_does_not_coast() {
    let mut h = Harness::touch(&long_text());
    h.tap(h.state.touch.view.center());
    h.idle(3);
    let p = h.state.touch.view.center() - vec2(0.0, 40.0);
    h.press(p);
    for dy in [30.0, 60.0, 90.0] {
        h.drag_to(p + vec2(0.0, dy));
    }
    h.time = h.time.map(|t| t + 1.0);
    h.release(p + vec2(0.0, 90.0));
    assert!(!h.state.touch.coasting);
}

#[test]
fn a_slow_drift_past_the_slop_barely_coasts() {
    let mut h = Harness::touch(&long_text());
    h.tap(h.state.touch.view.center());
    h.idle(3);
    let p = h.state.touch.view.center();
    h.press(p);
    h.idle(15);
    h.drag_to(p + vec2(0.0, 8.0));
    assert_eq!(h.state.touch.gesture, Gesture::Scrolling);
    let lifted = h.state.touch.scroll_y;
    h.release(p + vec2(0.0, 8.0));
    h.idle(60);
    assert!(
        (lifted - h.state.touch.scroll_y).abs() < 15.0,
        "{lifted} -> {}",
        h.state.touch.scroll_y
    );
}

#[test]
fn a_tap_during_a_coast_stops_it_and_keeps_the_cursor() {
    let mut h = Harness::touch(&long_text());
    let before = h.flick();
    h.idle(2);
    let p = h.state.touch.view.center();
    h.tap(p);
    assert!(!h.state.touch.coasting);
    h.idle(2);
    assert_eq!(h.cursor(), before);
    assert!(h.state.touch.last_tap.is_none());
}

#[test]
fn a_cancelled_press_is_not_a_tap() {
    let text = long_text();
    let len = text.chars().count();
    let mut h = Harness::touch(&text);
    h.tap(h.state.touch.view.center());
    h.idle(3);
    assert_eq!(h.cursor(), Some((len, len)));
    let p = h.state.touch.view.center();
    h.press(p);
    h.frame(vec![finger(TouchPhase::Cancel, p), Event::PointerGone]);
    h.idle(2);
    assert_eq!(h.cursor(), Some((len, len)));
    assert!(h.state.touch.last_tap.is_none());
    assert_eq!(h.state.touch.gesture, Gesture::Idle);
}
