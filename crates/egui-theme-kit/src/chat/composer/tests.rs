//! Headless frames through the composer: typing, sending, popups, chips, copy, drop hover and layout.

use egui::{
    Context, DroppedFile, Event, FullOutput, HoveredFile, Key, Modifiers, OutputCommand, Pos2,
    RawInput, Rect, vec2,
};

use super::*;
use crate::chat::mention::{MentionCandidate, MentionRef, encode};

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
    vec![SlashCommand {
        id: "compact".into(),
        name: "/compact".into(),
        args: "".into(),
        about: "Shrink the context".into(),
        section: None,
        icon: None,
        enabled: true,
        run_on_pick: false,
    }]
}

/// A composer harness with fixed sources and options.
struct Harness {
    ctx: Context,
    state: ComposerState,
    width: f32,
    enter: EnterBehavior,
    editable: bool,
    raw: Option<RawInput>,
}

impl Harness {
    fn new() -> Self {
        let mut h = Self {
            ctx: Context::default(),
            state: ComposerState::default(),
            width: 800.0,
            enter: EnterBehavior::Send,
            editable: true,
            raw: None,
        };
        h.frame(Vec::new());
        h.ctx
            .memory_mut(|m| m.request_focus(Composer::text_id(SALT)));
        h.frame(Vec::new());
        h
    }

    fn frame_full(&mut self, events: Vec<Event>) -> (ComposerResponse, FullOutput) {
        let mut raw = self.raw.take().unwrap_or_default();
        raw.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, vec2(self.width, 600.0)));
        raw.events = events;
        let mut response = None;
        let (enter, editable) = (self.enter, self.editable);
        let state = &mut self.state;
        let mentions = candidates;
        let slash = commands;
        let mut out = self.ctx.run_ui(raw, |ui| {
            response = Some(
                Composer::new(SALT)
                    .enter(enter)
                    .mentions(&mentions)
                    .slash(&slash)
                    .mode(ComposerMode {
                        editable,
                        ..ComposerMode::default()
                    })
                    .show(ui, state),
            );
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
        fn resolve(&self, t: &MentionRef) -> Option<String> {
            (t.id == "1").then(|| "Renamed".to_owned())
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
