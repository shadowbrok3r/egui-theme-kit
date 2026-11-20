//! The message composer: `+` menu, growing text box with mention chips and slash commands, attach and send.

mod edit;
mod plus;
mod popup;
mod slash;
mod state;
mod trigger;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use egui::{
    Align, AsIdSalt, Context, Event, EventFilter, FontId, Frame, Galley, Id, IdSalt, Image,
    InputState, Key, KeyboardShortcut, Layout, Margin, Modifiers, OutputCommand, Rect, Response,
    ScrollArea, Sense, Stroke, StrokeKind, TextBuffer, TextEdit, TextStyle, Ui, UiBuilder,
    load::SizedTexture,
    pos2,
    text::{CCursor, CCursorRange, LayoutJob, TextFormat},
    vec2,
};

pub use slash::{PlusItem, SlashCommand, SlashSource};
pub use state::{ComposerState, MentionSpan, OutgoingText};

use super::icons::ChatIcons;
use super::mention::{MentionSource, clean_label};
use super::palette::ChatPalette;
use super::surface::{SurfaceKind, surface_frame};
use super::transcript::AttachmentChip;
use super::types::CowStr;
use crate::controls::{IconBox, IconButton, Metrics, elided};
use popup::Suggestion;
use trigger::Trigger;

/// Narrowest touch target.
const TOUCH_MIN: f32 = 40.0;
/// Below this many control heights of width the controls sit above the text.
const NARROW_CONTROLS: f32 = 16.0;
/// Fewest text rows in the narrow layout.
const NARROW_ROWS: usize = 3;
/// Widest an attachment name is drawn.
const ATTACHMENT_NAME_W: f32 = 160.0;
/// Width of the drop-target outline.
const DROP_STROKE: f32 = 2.0;
/// Width of the line under an unresolved mention.
const UNRESOLVED_STROKE: f32 = 1.0;
/// Width change, in points, that relays the row out.
const RELAYOUT_EPS: f32 = 0.5;

/// What a send does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SendIntent {
    /// Starts a turn.
    #[default]
    Start,
    /// Steers the running turn.
    Steer,
    /// Queues for after the running turn.
    Queue,
}

/// Whether the agent is working, and what Enter does meanwhile.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Activity {
    #[default]
    Idle,
    Busy {
        enter: SendIntent,
        can_steer: bool,
        can_queue: bool,
    },
}

impl Activity {
    /// What a send does now: `enter` when allowed, else the other allowed intent; `None` when nothing is.
    pub fn send_intent(self) -> Option<SendIntent> {
        match self {
            Self::Idle => Some(SendIntent::Start),
            Self::Busy {
                enter,
                can_steer,
                can_queue,
            } => [enter, SendIntent::Steer, SendIntent::Queue]
                .into_iter()
                .find(|i| match i {
                    SendIntent::Steer => can_steer,
                    SendIntent::Queue => can_queue,
                    SendIntent::Start => false,
                }),
        }
    }

    pub fn is_busy(self) -> bool {
        matches!(self, Self::Busy { .. })
    }
}

/// What the composer allows this frame; typing and sending are separate.
#[derive(Clone, Debug)]
pub struct ComposerMode<'a> {
    pub activity: Activity,
    pub editable: bool,
    /// Why sending is blocked, shown as the send tooltip.
    pub send_blocked: Option<CowStr<'a>>,
}

impl Default for ComposerMode<'_> {
    fn default() -> Self {
        Self {
            activity: Activity::Idle,
            editable: true,
            send_blocked: None,
        }
    }
}

/// What Enter does without Shift.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EnterBehavior {
    Send,
    Newline,
}

impl EnterBehavior {
    /// Newline on touch screens, send otherwise.
    pub fn for_ui(ui: &Ui) -> Self {
        if ui.input(|i| i.has_touch_screen()) {
            Self::Newline
        } else {
            Self::Send
        }
    }
}

/// What the user asked the composer to do; the app carries it out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComposerAction {
    Send {
        intent: SendIntent,
        message: OutgoingText,
    },
    Stop,
    Attach,
    RemoveAttachment(usize),
    Command {
        id: String,
        args: String,
    },
    /// Id of the picked [`PlusItem`].
    Plus(String),
}

/// What a composer frame produced.
#[derive(Clone, Debug)]
pub struct ComposerResponse {
    pub action: Option<ComposerAction>,
    /// The text edit's response.
    pub text: Response,
    /// A suggestion was accepted this frame.
    pub accepted_suggestion: bool,
    /// Files are hovering over the composer; they are left in the input for the app.
    pub file_hover: bool,
    /// Files were dropped over the composer this frame; they are left in the input for the app.
    pub files_dropped: bool,
}

type Slot<'a> = Box<dyn FnOnce(&mut Ui) + 'a>;

fn text_id_of(id_salt: IdSalt) -> Id {
    Id::unique("egui_theme_kit.chat.composer").with_salt(id_salt)
}

/// The message composer.
pub struct Composer<'a> {
    id_salt: IdSalt,
    hint: CowStr<'a>,
    mode: ComposerMode<'a>,
    attachments: &'a [AttachmentChip<'a>],
    enter: Option<EnterBehavior>,
    min_rows: usize,
    max_height: Option<f32>,
    show_attach: bool,
    slash: Option<&'a dyn SlashSource>,
    mentions: Option<&'a dyn MentionSource>,
    mention_revision: u64,
    plus: &'a [PlusItem<'a>],
    narrow_below: f32,
    leading: Option<Slot<'a>>,
    trailing: Option<Slot<'a>>,
    above: Option<Slot<'a>>,
}

impl<'a> Composer<'a> {
    pub fn new(id_salt: impl AsIdSalt) -> Self {
        Self {
            id_salt: IdSalt::new(id_salt),
            hint: CowStr::Borrowed(""),
            mode: ComposerMode::default(),
            attachments: &[],
            enter: None,
            min_rows: 1,
            max_height: None,
            show_attach: true,
            slash: None,
            mentions: None,
            mention_revision: 0,
            plus: &[],
            narrow_below: NARROW_CONTROLS,
            leading: None,
            trailing: None,
            above: None,
        }
    }

    /// The fixed id of the text edit for `id_salt`, independent of where the composer sits.
    pub fn text_id(id_salt: impl AsIdSalt) -> Id {
        text_id_of(IdSalt::new(id_salt))
    }

    pub fn hint(mut self, hint: impl Into<CowStr<'a>>) -> Self {
        self.hint = hint.into();
        self
    }

    pub fn mode(mut self, mode: ComposerMode<'a>) -> Self {
        self.mode = mode;
        self
    }

    pub fn attachments(mut self, attachments: &'a [AttachmentChip<'a>]) -> Self {
        self.attachments = attachments;
        self
    }

    /// What Enter does; [`EnterBehavior::for_ui`] by default.
    pub fn enter(mut self, enter: EnterBehavior) -> Self {
        self.enter = Some(enter);
        self
    }

    pub fn min_rows(mut self, rows: usize) -> Self {
        self.min_rows = rows;
        self
    }

    /// Height the text grows to before scrolling; a third of the screen by default.
    pub fn max_height(mut self, height: f32) -> Self {
        self.max_height = Some(height);
        self
    }

    pub fn show_attach(mut self, show: bool) -> Self {
        self.show_attach = show;
        self
    }

    pub fn slash(mut self, source: &'a dyn SlashSource) -> Self {
        self.slash = Some(source);
        self
    }

    pub fn mentions(mut self, source: &'a dyn MentionSource) -> Self {
        self.mentions = Some(source);
        self
    }

    /// Version of the labels [`MentionSource::resolve`] returns; resolved labels are cached until it changes.
    pub fn mention_revision(mut self, revision: u64) -> Self {
        self.mention_revision = revision;
        self
    }

    /// App entries of the `+` menu.
    pub fn plus(mut self, items: &'a [PlusItem<'a>]) -> Self {
        self.plus = items;
        self
    }

    /// Width, in control heights, below which the controls sit in a row above the text.
    pub fn narrow_below(mut self, control_heights: f32) -> Self {
        self.narrow_below = control_heights;
        self
    }

    /// Controls before the `+` button.
    pub fn leading(mut self, add: impl FnOnce(&mut Ui) + 'a) -> Self {
        self.leading = Some(Box::new(add));
        self
    }

    /// Controls before the send button.
    pub fn trailing(mut self, add: impl FnOnce(&mut Ui) + 'a) -> Self {
        self.trailing = Some(Box::new(add));
        self
    }

    /// Content above the text row, such as a queue strip or a plan card.
    pub fn above(mut self, add: impl FnOnce(&mut Ui) + 'a) -> Self {
        self.above = Some(Box::new(add));
        self
    }

    pub fn show(self, ui: &mut Ui, state: &mut ComposerState) -> ComposerResponse {
        let text_id = text_id_of(self.id_salt);
        let parts = text_id.with("egui_theme_kit.chat.composer.parts");
        let ctx = ui.ctx().clone();
        let touch = ui.input(|i| i.has_touch_screen());
        let m = Metrics::get(&ctx);
        let look = Look {
            p: ChatPalette::get(&ctx),
            icons: ChatIcons::get(&ctx),
            icon_box: icon_box_for(
                &m,
                if touch {
                    m.control_h.max(TOUCH_MIN)
                } else {
                    m.button_h
                },
            ),
            row_h: if touch {
                m.control_h.max(TOUCH_MIN)
            } else {
                m.control_h
            },
        };
        let Self {
            hint,
            mode,
            attachments,
            enter,
            min_rows,
            max_height,
            show_attach,
            slash,
            mentions,
            mention_revision,
            plus,
            narrow_below,
            leading,
            trailing,
            above,
            ..
        } = self;
        let enter = enter.unwrap_or_else(|| EnterBehavior::for_ui(ui));
        let narrow = ui.available_width() < narrow_below * m.control_h;
        let editable = mode.editable;
        let mut out = Outcome::default();

        if let Some(cursor) = state.pending_cursor.take() {
            store_cursor(&ctx, text_id, cursor, cursor, Some(state.text()));
        }
        let unresolved = resolve_labels(&ctx, text_id, state, mentions, mention_revision);

        if let Some(above) = above {
            ui.scope_builder(UiBuilder::new().scope_id(parts.with("above")), above);
        }

        let focused = editable && ctx.memory(|mem| mem.has_focus(text_id));
        if let Some(popup) = state.popup.as_mut().filter(|_| focused)
            && let Some(i) = popup::show_scoped(ui, parts.with("popup"), popup, look.row_h)
        {
            popup.selected = i;
            accept(&ctx, text_id, state, &mut out);
            state.refocus = true;
        }
        if focused {
            handle_keys(&ctx, text_id, state, enter, &mut out);
        }
        let undo = focused && ctx.input(has_undo);
        let copying = focused
            && ctx.input(|i| {
                i.events
                    .iter()
                    .any(|e| matches!(e, Event::Copy | Event::Cut))
            });

        let max_h = max_height.unwrap_or_else(|| ctx.content_rect().height() / 3.0);
        let rows = if narrow {
            min_rows.max(NARROW_ROWS)
        } else {
            min_rows
        };
        let mut send_clicked = false;
        let frame = ui.scope_builder(UiBuilder::new().scope_id(parts.with("frame")), |ui| {
            surface_frame(
                ui,
                parts.with("surface"),
                SurfaceKind::Composer,
                Margin::symmetric(m.pad.y as i8, m.pad.y as i8),
                |ui| {
                    ui.set_width(ui.available_width());
                    attachment_row(ui, &look, attachments, &mut out);
                    let controls = Controls {
                        look: &look,
                        mode: &mode,
                        show_attach,
                        can_send: !state.is_empty() || !attachments.is_empty(),
                        leading,
                        trailing,
                    };
                    let old_text = state.text().to_owned();
                    let old_spans = state.spans().to_vec();
                    let old_sel = cursor_of(&ctx, text_id);
                    let text_area = TextArea {
                        id: text_id,
                        scroll: parts.with("scroll"),
                        hint: &hint,
                        rows,
                        max_h,
                        editable,
                        enter,
                        unresolved: &unresolved,
                        popup: state.popup.is_some(),
                    };
                    let (text_response, clicked) =
                        controls.lay_out(ui, state, text_area, narrow, &mut out);
                    send_clicked = clicked;
                    after_edit(
                        &ctx,
                        text_id,
                        state,
                        EditFrame {
                            old_text: &old_text,
                            old_spans: &old_spans,
                            old_sel,
                            undo,
                            copying,
                        },
                    );
                    text_response
                },
            )
        });
        let surface = frame.inner;
        let rect = surface.response.rect;
        let text = surface.inner;

        if send_clicked || out.send {
            out.action = out
                .action
                .take()
                .or_else(|| send(state, &mode, attachments, slash));
        }

        let (file_hover, files_dropped) = ctx.input(|i| {
            let over = i.pointer.latest_pos().is_none_or(|pos| rect.contains(pos));
            (
                over && !i.raw.hovered_files.is_empty(),
                over && !i.raw.dropped_files.is_empty(),
            )
        });
        if file_hover {
            ui.painter().rect_stroke(
                rect,
                look.p.radius_card,
                Stroke::new(DROP_STROKE, look.p.accent),
                StrokeKind::Inside,
            );
        }

        update_popup(&ctx, text_id, state, editable, slash, mentions);
        plus::show(
            &ctx,
            text_id,
            state,
            plus::Menu {
                id: parts.with("plus"),
                show_attach,
                editable,
                mentions: mentions.is_some(),
                items: plus,
                slash,
            },
            &mut out,
        );
        if state.refocus && !state.plus.is_open() {
            state.refocus = false;
            ctx.memory_mut(|mem| mem.request_focus(text_id));
        }

        ComposerResponse {
            action: out.action,
            text,
            accepted_suggestion: out.accepted,
            file_hover,
            files_dropped,
        }
    }
}

/// Colours, icons and sizes for one frame.
struct Look {
    p: ChatPalette,
    icons: ChatIcons,
    icon_box: IconBox,
    row_h: f32,
}

/// What a frame collected for the response.
#[derive(Default)]
struct Outcome {
    action: Option<ComposerAction>,
    accepted: bool,
    /// A send was asked for by key.
    send: bool,
}

/// Smallest icon box at least `target` tall, else the largest.
fn icon_box_for(m: &Metrics, target: f32) -> IconBox {
    [IconBox::Sm, IconBox::Md, IconBox::Lg, IconBox::Tool]
        .into_iter()
        .find(|b| b.side(m) >= target)
        .unwrap_or(IconBox::Tool)
}

/// `(secondary, primary)` char positions of the stored cursor.
fn cursor_of(ctx: &Context, id: Id) -> Option<(usize, usize)> {
    let range = TextEdit::load_state(ctx, id)?.cursor.char_range()?;
    Some((range.secondary.index.0, range.primary.index.0))
}

/// Stores a selection from `secondary` to `primary`, recording `text` as an undo point when given.
fn store_cursor(ctx: &Context, id: Id, secondary: usize, primary: usize, text: Option<&str>) {
    let mut st = TextEdit::load_state(ctx, id).unwrap_or_default();
    let range = CCursorRange::two(CCursor::new(secondary), CCursor::new(primary));
    if let Some(text) = text {
        let mut undoer = st.undoer();
        undoer.add_undo(&(range, text.to_owned()));
        st.set_undoer(undoer);
    }
    st.cursor.set_char_range(Some(range));
    st.store(ctx, id);
    ctx.request_repaint();
}

/// Rewrites chips whose target was renamed; returns which chips do not resolve.
fn resolve_labels(
    ctx: &Context,
    id: Id,
    state: &mut ComposerState,
    source: Option<&dyn MentionSource>,
    revision: u64,
) -> Vec<bool> {
    let Some(source) = source else {
        return vec![false; state.spans().len()];
    };
    let mut unresolved = Vec::with_capacity(state.spans().len());
    for i in 0..state.spans().len() {
        let target = state.spans()[i].target.clone();
        match state.labels.resolve(source, revision, &target) {
            Some(label) => {
                let label = clean_label(&label);
                if !label.is_empty() && label != target.label {
                    let (text, spans) = state.parts_mut();
                    let (old, n) = edit::relabel(text, spans, i, &label);
                    if let Some((a, b)) = cursor_of(ctx, id) {
                        let (a, b) = (edit::map_cursor(a, &old, n), edit::map_cursor(b, &old, n));
                        store_cursor(ctx, id, a, b, None);
                    }
                }
                unresolved.push(false);
            }
            None => unresolved.push(!state.was_picked(&target)),
        }
    }
    unresolved
}

/// Removes key presses of `key` whose modifiers pass `test`; returns how many.
fn take_key(i: &mut InputState, key: Key, test: impl Fn(Modifiers) -> bool) -> usize {
    let mut n = 0;
    i.events.retain(|e| {
        let hit = matches!(e, Event::Key { key: k, pressed: true, modifiers, .. } if *k == key && test(*modifiers));
        n += usize::from(hit);
        !hit
    });
    n
}

/// Whether this frame carries an undo or redo shortcut.
fn has_undo(i: &InputState) -> bool {
    i.events.iter().any(|e| {
        matches!(e, Event::Key { key: Key::Z | Key::Y, pressed: true, modifiers, .. } if modifiers.command)
    })
}

/// Popup navigation, or Enter to send, consumed before the text edit sees them.
fn handle_keys(
    ctx: &Context,
    id: Id,
    state: &mut ComposerState,
    enter: EnterBehavior,
    out: &mut Outcome,
) {
    let plain = |m: Modifiers| m.is_none();
    if let Some(popup) = state.popup.as_mut() {
        let (down, up, pick, close) = ctx.input_mut(|i| {
            (
                take_key(i, Key::ArrowDown, plain),
                take_key(i, Key::ArrowUp, plain),
                take_key(i, Key::Enter, plain) + take_key(i, Key::Tab, plain),
                take_key(i, Key::Escape, plain),
            )
        });
        popup.step(down as isize - up as isize);
        if close > 0 {
            state.dismissed = Some(popup.trigger.at());
            state.popup = None;
        } else if pick > 0 {
            accept(ctx, id, state, out);
        }
        return;
    }
    let sent = ctx.input_mut(|i| {
        let command = take_key(i, Key::Enter, |m| m.command && !m.shift && !m.alt);
        let bare = match enter {
            EnterBehavior::Send => take_key(i, Key::Enter, plain),
            EnterBehavior::Newline => 0,
        };
        command + bare > 0
    });
    out.send |= sent;
}

/// Applies the selected suggestion of the open popup.
fn accept(ctx: &Context, id: Id, state: &mut ComposerState, out: &mut Outcome) {
    let Some(popup) = state.popup.take() else {
        return;
    };
    let Some(item) = popup.current().cloned() else {
        state.popup = Some(popup);
        return;
    };
    state.dismissed = None;
    out.accepted = true;
    let query_end = popup.trigger.at() + 1 + popup.trigger.query().chars().count();
    match item {
        Suggestion::Mention { target, .. } => {
            let cursor = state.accept_mention(popup.trigger.at()..query_end, target);
            store_cursor(ctx, id, cursor, cursor, Some(state.text()));
        }
        Suggestion::Slash { pick, .. } if pick.runs_on_pick() => {
            let wire = state.message().wire;
            let args = wire
                .split_once(char::is_whitespace)
                .map_or("", |(_, rest)| rest.trim());
            out.action = Some(ComposerAction::Command {
                id: pick.id,
                args: args.to_owned(),
            });
            state.clear();
        }
        Suggestion::Slash { pick, .. } => {
            let cursor = insert_command(state, &pick.name);
            store_cursor(ctx, id, cursor, cursor, Some(state.text()));
        }
    }
}

/// Replaces a leading `/token` with `/name ` or puts it in front; returns the cursor after it.
fn insert_command(state: &mut ComposerState, name: &str) -> usize {
    let text = state.text();
    let token = if text.starts_with('/') {
        let chars: Vec<char> = text.chars().collect();
        let end = chars
            .iter()
            .position(|c| c.is_whitespace())
            .unwrap_or(chars.len());
        end + usize::from(chars.get(end).is_some_and(|c| *c == ' '))
    } else {
        0
    };
    state.replace(0..token, &format!("/{name} "))
}

/// The send or command a submit produces, clearing the buffer; `None` when nothing may be sent.
fn send(
    state: &mut ComposerState,
    mode: &ComposerMode<'_>,
    attachments: &[AttachmentChip<'_>],
    slash: Option<&dyn SlashSource>,
) -> Option<ComposerAction> {
    let intent = mode.activity.send_intent()?;
    if mode.send_blocked.is_some() || (state.is_empty() && attachments.is_empty()) {
        return None;
    }
    let message = state.message();
    let action = match slash.and_then(|s| slash::parse_command(s, message.wire.trim_start())) {
        Some((id, args)) => ComposerAction::Command { id, args },
        None => ComposerAction::Send { intent, message },
    };
    state.clear();
    Some(action)
}

/// Snapshot taken before the text edit ran.
struct EditFrame<'s> {
    old_text: &'s str,
    old_spans: &'s [state::MentionSpan],
    old_sel: Option<(usize, usize)>,
    undo: bool,
    copying: bool,
}

/// Reconciles chips with the text edit's changes, rewrites copies to wire form and snaps the cursor out of chips.
fn after_edit(ctx: &Context, id: Id, state: &mut ComposerState, f: EditFrame<'_>) {
    if f.copying
        && let Some((a, b)) = f.old_sel.filter(|(a, b)| a != b)
    {
        let range = a.min(b)..a.max(b);
        if let Some(wire) = edit::wire_slice(f.old_text, f.old_spans, range.clone()) {
            let display: String = f
                .old_text
                .chars()
                .skip(range.start)
                .take(range.len())
                .collect();
            ctx.output_mut(|o| {
                for cmd in &mut o.commands {
                    if let OutputCommand::CopyText(t) = cmd
                        && *t == display
                    {
                        t.clone_from(&wire);
                    }
                }
            });
        }
    }
    if state.text() != f.old_text {
        let before = f.old_sel.map(|(a, b)| a.min(b)..a.max(b));
        let after = cursor_of(ctx, id).map(|(_, primary)| primary);
        let mut graveyard = std::mem::take(&mut state.graveyard);
        let outcome = edit::reconcile(
            f.old_text,
            state.text(),
            state.spans(),
            &mut graveyard,
            before,
            after,
            f.undo,
        );
        state.graveyard = graveyard;
        state.set_parts(outcome.text, outcome.spans);
        if let Some(cursor) = outcome.cursor {
            store_cursor(ctx, id, cursor, cursor, Some(state.text()));
        }
    }
    if let Some((secondary, primary)) = cursor_of(ctx, id) {
        let head = edit::snap(state.spans(), f.old_sel.map(|s| s.1), primary);
        let anchor = if secondary == primary {
            head
        } else {
            edit::snap_outward(state.spans(), secondary, head)
        };
        if (anchor, head) != (secondary, primary) {
            store_cursor(ctx, id, anchor, head, None);
        }
    }
}

/// Opens, refreshes or closes the suggestion popup for the trigger at the cursor.
fn update_popup(
    ctx: &Context,
    id: Id,
    state: &mut ComposerState,
    editable: bool,
    slash: Option<&dyn SlashSource>,
    mentions: Option<&dyn MentionSource>,
) {
    let focused = editable && (ctx.memory(|mem| mem.has_focus(id)) || state.refocus);
    let cursor = cursor_of(ctx, id)
        .filter(|(a, b)| a == b)
        .map(|(_, b)| b)
        .or(state.pending_cursor);
    let found = cursor.filter(|_| focused).and_then(|c| {
        trigger::detect(
            state.text(),
            c,
            state.spans(),
            slash.is_some(),
            mentions.is_some(),
        )
    });
    if state.dismissed.is_some() && found.as_ref().map(Trigger::at) != state.dismissed {
        state.dismissed = None;
    }
    let found = found.filter(|t| state.dismissed != Some(t.at()));
    let Some(found) = found else {
        if state.popup.take().is_some() {
            ctx.request_repaint();
        }
        return;
    };
    if state.popup.as_ref().is_some_and(|p| p.trigger == found) {
        return;
    }
    let items = match (&found, mentions, slash) {
        (Trigger::Mention { query, .. }, Some(source), _) => popup::mention_items(source, query),
        (Trigger::Slash { query }, _, Some(source)) => popup::slash_items(source, query),
        _ => Vec::new(),
    };
    let had = state.popup.is_some();
    state.popup = (!items.is_empty()).then_some(popup::Popup {
        trigger: found,
        items,
        selected: 0,
    });
    if had || state.popup.is_some() {
        ctx.request_repaint();
    }
}

/// The text edit with chip highlighting inside a scroll area that grows to `max_h`.
struct TextArea<'s> {
    id: Id,
    scroll: Id,
    hint: &'s str,
    rows: usize,
    max_h: f32,
    editable: bool,
    enter: EnterBehavior,
    unresolved: &'s [bool],
    /// The popup is open, so Tab and Escape reach the composer instead of moving focus.
    popup: bool,
}

impl TextArea<'_> {
    /// Draws the text in a child of `ui` over `rect`; returns its response and the height it used.
    fn show_in(self, ui: &mut Ui, state: &mut ComposerState, rect: Rect) -> (Response, f32) {
        let p = ChatPalette::of(ui);
        let font = TextStyle::Body.resolve(ui.style());
        let spans = state.spans().to_vec();
        let unresolved = self.unresolved.to_vec();
        let mut layouter = move |ui: &Ui, buf: &dyn TextBuffer, wrap: f32| -> Arc<Galley> {
            let mut job = chip_job(buf.as_str(), &spans, &unresolved, &font, &p);
            job.wrap.max_width = wrap;
            ui.painter().layout_job(job)
        };
        let return_key = match self.enter {
            EnterBehavior::Send => KeyboardShortcut::new(Modifiers::SHIFT, Key::Enter),
            EnterBehavior::Newline => KeyboardShortcut::new(Modifiers::NONE, Key::Enter),
        };
        let mut child = ui.new_child(
            UiBuilder::new()
                .id_salt("egui_theme_kit.chat.composer.text")
                .max_rect(rect)
                .layout(Layout::top_down(Align::Min)),
        );
        let response = ScrollArea::vertical()
            .id_salt(self.scroll)
            .max_height(self.max_h)
            .auto_shrink([false, true])
            .show(&mut child, |ui| {
                let edit = TextEdit::multiline(state.buffer_mut());
                let filter = edit.get_event_filter();
                edit.event_filter(EventFilter {
                    tab: filter.tab || self.popup,
                    escape: filter.escape || self.popup,
                    ..filter
                })
                .id(self.id)
                .hint_text(self.hint)
                .desired_rows(self.rows)
                .desired_width(f32::INFINITY)
                .frame(Frame::NONE)
                .interactive(self.editable)
                .return_key(return_key)
                .layouter(&mut layouter)
                .show(ui)
                .response
                .response
            })
            .inner;
        let height = child.min_rect().height();
        if (height - state.text_h).abs() > RELAYOUT_EPS {
            state.text_h = height;
            ui.ctx().request_repaint();
        }
        (response, height)
    }
}

/// `text` laid out with chips over `spans`; spans whose text no longer matches stay plain.
fn chip_job(
    text: &str,
    spans: &[state::MentionSpan],
    unresolved: &[bool],
    font: &FontId,
    p: &ChatPalette,
) -> LayoutJob {
    let plain = TextFormat::simple(font.clone(), p.text);
    let chip = TextFormat {
        color: p.mention_text,
        background: p.mention_fill,
        ..plain.clone()
    };
    let dead = TextFormat {
        color: p.text_dim,
        background: p.chip_fill,
        underline: Stroke::new(UNRESOLVED_STROKE, p.warn),
        ..plain.clone()
    };
    let bytes: Vec<usize> = text
        .char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(text.len()))
        .collect();
    let mut job = LayoutJob::default();
    let mut at = 0;
    for (i, span) in spans.iter().enumerate() {
        let (Some(&a), Some(&b)) = (bytes.get(span.chars.start), bytes.get(span.chars.end)) else {
            continue;
        };
        if a < at || text[a..b] != edit::chip_text(&span.target) {
            continue;
        }
        job.append(&text[at..a], 0.0, plain.clone());
        let format = if unresolved.get(i).copied().unwrap_or(false) {
            dead.clone()
        } else {
            chip.clone()
        };
        job.append(&text[a..b], 0.0, format);
        at = b;
    }
    job.append(&text[at..], 0.0, plain);
    job
}

/// Chips of the attachments with remove buttons.
fn attachment_row(ui: &mut Ui, look: &Look, attachments: &[AttachmentChip<'_>], out: &mut Outcome) {
    if attachments.is_empty() {
        return;
    }
    let m = Metrics::get(ui.ctx());
    ui.horizontal_wrapped(|ui| {
        for (i, chip) in attachments.iter().enumerate() {
            surface_frame(
                ui,
                ("egui_theme_kit.chat.attachment", i),
                SurfaceKind::Chip,
                Margin::symmetric(m.pad.y as i8, 0),
                |ui| {
                    ui.horizontal(|ui| {
                        if let Some(texture) = chip.image {
                            ui.add(Image::new(SizedTexture::new(
                                texture,
                                vec2(m.icon_lg, m.icon_lg),
                            )));
                        }
                        let font = TextStyle::Small.resolve(ui.style());
                        let galley = elided(ui, &chip.name, font, ATTACHMENT_NAME_W);
                        let (rect, _) = ui.allocate_exact_size(galley.size(), Sense::hover());
                        let ink = if chip.reading {
                            look.p.text_dim
                        } else {
                            look.p.chip_text
                        };
                        ui.painter().galley(rect.min, galley, ink);
                        if chip.reading {
                            ui.add(egui::Spinner::new().size(m.icon_sm).color(look.p.text_dim));
                        }
                        if IconButton::new(look.icons.close, IconBox::Sm)
                            .tip("Remove attachment")
                            .show(ui)
                            .clicked()
                        {
                            out.action = Some(ComposerAction::RemoveAttachment(i));
                        }
                    });
                },
            );
        }
    });
}

/// The buttons around the text.
struct Controls<'s, 'a> {
    look: &'s Look,
    mode: &'s ComposerMode<'a>,
    show_attach: bool,
    can_send: bool,
    leading: Option<Slot<'a>>,
    trailing: Option<Slot<'a>>,
}

impl Controls<'_, '_> {
    /// Lays out the controls and `text`: one row when wide, a control row over the text when narrow.
    fn lay_out(
        self,
        ui: &mut Ui,
        state: &mut ComposerState,
        text: TextArea<'_>,
        narrow: bool,
        out: &mut Outcome,
    ) -> (Response, bool) {
        let side = self.look.icon_box.side(&Metrics::get(ui.ctx()));
        let gap = ui.spacing().item_spacing;
        let origin = ui.available_rect_before_wrap().min;
        let width = ui.available_width();
        let max_h = text.max_h;
        if narrow {
            let bar = Rect::from_min_size(origin, vec2(width, side));
            let clicked = self.sides(ui, state, bar, Align::Center, out);
            let top = bar.bottom() + gap.y;
            let area = Rect::from_min_size(pos2(origin.x, top), vec2(width, max_h));
            let (response, h) = text.show_in(ui, state, area);
            ui.advance_cursor_after_rect(Rect::from_min_max(
                origin,
                pos2(origin.x + width, top + h),
            ));
            return (response, clicked);
        }
        let right_w = if state.right_w > 0.0 {
            state.right_w
        } else {
            3.0 * (side + gap.x)
        };
        let left_w = if state.left_w > 0.0 {
            state.left_w
        } else {
            side + gap.x
        };
        let text_w = (width - left_w - right_w - gap.x).max(side);
        let lift = (side - state.text_h).max(0.0) / 2.0;
        let area = Rect::from_min_size(
            pos2(origin.x + left_w, origin.y + lift),
            vec2(text_w, max_h),
        );
        let (response, h) = text.show_in(ui, state, area);
        let row = Rect::from_min_size(origin, vec2(width, (lift + h).max(side)));
        let clicked = self.sides(ui, state, row, Align::Max, out);
        ui.advance_cursor_after_rect(row);
        (response, clicked)
    }

    /// Leading and `+` at the left of `row`, the rest at its right; returns whether send was clicked.
    fn sides(
        self,
        ui: &mut Ui,
        state: &mut ComposerState,
        row: Rect,
        align: Align,
        out: &mut Outcome,
    ) -> bool {
        let Self {
            look,
            mode,
            show_attach,
            can_send,
            leading,
            trailing,
        } = self;
        let gap = ui.spacing().item_spacing.x;
        let mut left = ui.new_child(
            UiBuilder::new()
                .id_salt("egui_theme_kit.chat.composer.left")
                .max_rect(row)
                .layout(Layout::left_to_right(align)),
        );
        if let Some(leading) = leading {
            leading(&mut left);
        }
        if IconButton::new(look.icons.plus, look.icon_box)
            .tip("Commands and more")
            .show(&mut left)
            .clicked()
        {
            state.plus.open();
            state.refocus = true;
        }
        remember(ui, &mut state.left_w, left.min_rect().width() + gap);

        let right_w = if state.right_w > 0.0 {
            state.right_w
        } else {
            row.width() / 2.0
        };
        let mut right = ui.new_child(
            UiBuilder::new()
                .id_salt("egui_theme_kit.chat.composer.right")
                .max_rect(Rect::from_min_max(
                    pos2(row.right() - right_w, row.top()),
                    row.max,
                ))
                .layout(Layout::left_to_right(align)),
        );
        if let Some(trailing) = trailing {
            trailing(&mut right);
        }
        if show_attach
            && IconButton::new(look.icons.attach, look.icon_box)
                .tip("Attach file or image")
                .show(&mut right)
                .clicked()
        {
            out.action = Some(ComposerAction::Attach);
        }
        if mode.activity.is_busy()
            && IconButton::new(look.icons.stop, look.icon_box)
                .tip("Stop")
                .show(&mut right)
                .clicked()
        {
            out.action = Some(ComposerAction::Stop);
        }
        let clicked = send_button(&mut right, look, mode, can_send);
        remember(ui, &mut state.right_w, right.min_rect().width());
        clicked
    }
}

/// Stores a measured width and repaints when it moved.
fn remember(ui: &Ui, slot: &mut f32, width: f32) {
    if (width - *slot).abs() > RELAYOUT_EPS {
        *slot = width;
        ui.ctx().request_repaint();
    }
}

/// The send, steer or queue button; returns whether it was clicked.
fn send_button(ui: &mut Ui, look: &Look, mode: &ComposerMode<'_>, has_content: bool) -> bool {
    let intent = mode.activity.send_intent();
    let (icon, tip) = match intent {
        Some(SendIntent::Steer) => (look.icons.steer, "Steer"),
        Some(SendIntent::Queue) => (look.icons.queue, "Queue"),
        _ => (look.icons.send, "Send"),
    };
    let enabled = intent.is_some() && mode.send_blocked.is_none() && has_content;
    let response = IconButton::new(icon, look.icon_box)
        .tip(tip)
        .tint(look.p.accent)
        .enabled(enabled)
        .show(ui);
    let response = match &mode.send_blocked {
        Some(reason) => response.on_disabled_hover_text(reason.as_ref()),
        None => response,
    };
    response.clicked()
}
