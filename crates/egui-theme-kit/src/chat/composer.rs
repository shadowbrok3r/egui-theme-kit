//! The message composer: `+` menu, growing text box with mention chips and slash commands, attach and send.

mod edit;
mod plus;
mod popup;
mod slash;
mod state;
mod touch;
mod trigger;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use egui::{
    Align, AsIdSalt, Context, Event, EventFilter, FontId, Frame, Galley, Id, IdSalt, Image,
    InputState, Key, KeyboardShortcut, Layout, Margin, Modifiers, OutputCommand, Rect, Response,
    ScrollArea, Sense, Stroke, StrokeKind, SurrenderFocusOn, TextBuffer, TextEdit, TextStyle, Ui,
    UiBuilder, ViewportCommand,
    load::SizedTexture,
    pos2,
    text::{CCursor, CCursorRange, LayoutJob, TextFormat},
    vec2,
};

pub use slash::{PlusItem, SlashCommand, SlashSource};
pub use state::{ComposerState, MentionSpan, OutgoingText};
pub use touch::TextAction;

use super::icons::ChatIcons;
use super::mention::{MentionSource, Resolution, clean_label};
use super::palette::ChatPalette;
use super::surface::{SurfaceKind, surface_frame};
use super::transcript::{AttachmentChip, cover_uv};
use super::types::CowStr;
use crate::controls::{IconBox, IconButton, Metrics, elided};
use popup::Suggestion;
use touch::Gesture;
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

    /// Whether a send with `intent` is allowed now.
    pub fn allows(self, intent: SendIntent) -> bool {
        match self {
            Self::Idle => intent == SendIntent::Start,
            Self::Busy {
                can_steer,
                can_queue,
                ..
            } => match intent {
                SendIntent::Start => false,
                SendIntent::Steer => can_steer,
                SendIntent::Queue => can_queue,
            },
        }
    }

    /// While busy, the allowed intent other than [`Self::send_intent`]; Alt+Enter and a second button send with it.
    pub fn alternate_intent(self) -> Option<SendIntent> {
        let first = self.send_intent()?;
        [SendIntent::Steer, SendIntent::Queue]
            .into_iter()
            .find(|i| *i != first && self.allows(*i))
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
    /// Why stopping is blocked, shown as the tooltip of a disabled Stop that reports nothing.
    pub stop_blocked: Option<CowStr<'a>>,
}

impl Default for ComposerMode<'_> {
    fn default() -> Self {
        Self {
            activity: Activity::Idle,
            editable: true,
            send_blocked: None,
            stop_blocked: None,
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
    /// A slash command; emitted whatever the activity or send block, the app decides whether it runs.
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
    /// Without a pointer position only the last focused composer still drawn, else the first drawn, claims them.
    pub file_hover: bool,
    /// Files were dropped over the composer this frame; they are left in the input for the app.
    pub files_dropped: bool,
    /// The composer drew its cut, copy, paste and select-all buttons this frame.
    pub provides_text_actions: bool,
}

type Slot<'a> = Box<dyn FnOnce(&mut Ui) + 'a>;

/// Reads the system clipboard on a paste tap.
pub type ClipboardRead<'a> = &'a dyn Fn() -> Option<String>;

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
    touch: Option<bool>,
    text_actions: bool,
    clipboard: Option<ClipboardRead<'a>>,
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
            touch: None,
            text_actions: true,
            clipboard: None,
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

    /// Forces touch handling on or off; defaults to whether the screen is touched.
    pub fn touch(mut self, touch: bool) -> Self {
        self.touch = Some(touch);
        self
    }

    /// Whether touch screens get the built-in cut, copy, paste and select-all row; on by default.
    pub fn text_actions(mut self, show: bool) -> Self {
        self.text_actions = show;
        self
    }

    /// Clipboard reader for the paste button; without one it asks the integration with [`ViewportCommand::RequestPaste`].
    pub fn clipboard(mut self, read: ClipboardRead<'a>) -> Self {
        self.clipboard = Some(read);
        self
    }

    /// Box size of the composer's own buttons; `touch` matches [`Composer::touch`], which defaults to whether the screen is touched.
    pub fn control_box(ctx: &Context, touch: bool) -> IconBox {
        let m = Metrics::get(ctx);
        let target = if touch {
            m.control_h.max(TOUCH_MIN)
        } else {
            m.button_h
        };
        icon_box_for(&m, target)
    }

    pub fn show(self, ui: &mut Ui, state: &mut ComposerState) -> ComposerResponse {
        let text_id = text_id_of(self.id_salt);
        let parts = text_id.with("egui_theme_kit.chat.composer.parts");
        let ctx = ui.ctx().clone();
        let touch = self
            .touch
            .unwrap_or_else(|| ui.input(|i| i.has_touch_screen()));
        let m = Metrics::get(&ctx);
        let look = Look {
            p: ChatPalette::get(&ctx),
            icons: ChatIcons::get(&ctx),
            icon_box: Self::control_box(&ctx, touch),
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
            text_actions,
            clipboard,
            ..
        } = self;
        let enter = enter.unwrap_or_else(|| EnterBehavior::for_ui(ui));
        let narrow = ui.available_width() < narrow_below * m.control_h;
        let editable = mode.editable;
        let mut out = Outcome::default();

        if let Some(cursor) = state.pending_cursor.take() {
            store_cursor(&ctx, text_id, cursor, cursor, Some(state.text()));
            state.touch.track = true;
        }
        let unresolved = resolve_labels(&ctx, text_id, state, mentions, mention_revision);

        if let Some(above) = above {
            ui.scope_builder(UiBuilder::new().scope_id(parts.with("above")), above);
        }

        let focused = editable && ctx.memory(|mem| mem.has_focus(text_id));
        if let Some(text) = state.touch.pending_paste.take().filter(|_| focused) {
            ctx.input_mut(|i| i.events.push(Event::Paste(text)));
        }
        if touch::text_hash(state.text()) != state.touch.drawn {
            state.touch.track = true;
        }
        let step = if touch && editable {
            let cursor = cursor_of(&ctx, text_id);
            touch::advance(&ctx, &mut state.touch, focused, cursor)
        } else {
            state.touch.gesture = Gesture::Idle;
            touch::Step::default()
        };
        if let Some((a, b)) = step.restore {
            store_cursor(&ctx, text_id, a, b, None);
        }
        let actions = (touch && editable && text_actions && focused).then(|| {
            let selected = cursor_of(&ctx, text_id).is_some_and(|(a, b)| a != b);
            Actions {
                copy: selected,
                select: !state.text().is_empty(),
            }
        });
        let guard = actions.is_some()
            && ctx.input(|i| {
                i.pointer
                    .interact_pos()
                    .is_some_and(|p| state.touch.on_button(p))
            });
        state.touch.buttons.clear();
        if let Some(popup) = state.popup.as_mut().filter(|_| focused)
            && let Some(i) = popup::show_scoped(ui, parts.with("popup"), popup, look.row_h)
        {
            popup.selected = i;
            accept(&ctx, text_id, state, &mut out);
            state.refocus = true;
        }
        if focused {
            handle_keys(&ctx, state, enter, mode.activity, &mut out);
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
        let mut send_clicked = None;
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
                        command: is_command(state.text(), slash),
                        leading,
                        trailing,
                        actions,
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
                        scroll_by: step.scroll_by,
                        track: state.touch.track && state.touch.gesture != Gesture::Scrolling,
                    };
                    let surrender = guard.then(|| {
                        ctx.options_mut(|o| {
                            std::mem::replace(
                                &mut o.input_options.surrender_focus_on,
                                SurrenderFocusOn::Never,
                            )
                        })
                    });
                    let (text_response, clicked) =
                        controls.lay_out(ui, state, text_area, narrow, &mut out);
                    if let Some(on) = surrender {
                        ctx.options_mut(|o| o.input_options.surrender_focus_on = on);
                    }
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

        if let Some(by_enter) = out.pick.take() {
            update_popup(&ctx, text_id, state, editable, slash, mentions);
            if state.popup.is_some() {
                accept(&ctx, text_id, state, &mut out);
            } else if by_enter && enter == EnterBehavior::Send {
                out.send = true;
            }
        }
        if let Some(intent) = send_clicked {
            out.send = true;
            out.intent = Some(intent);
        }
        if out.send {
            out.action = out
                .action
                .take()
                .or_else(|| send(state, &mode, out.intent, attachments, slash));
        }

        if let Some(action) = out.text_action.take() {
            apply_text_action(&ctx, text_id, state, action, clipboard);
        }
        let has_focus = ctx.memory(|mem| mem.has_focus(text_id));
        let regain = std::mem::take(&mut state.touch.regain);
        if touch && editable && has_focus && !state.touch.was_focused && !regain {
            let end = state.char_len();
            store_cursor(&ctx, text_id, end, end, None);
            state.touch.track = true;
        }
        state.touch.was_focused = has_focus;
        let unplaced = claims_unplaced_drop(&ctx, text_id, has_focus);
        let (file_hover, files_dropped) = ctx.input(|i| {
            let over = i
                .pointer
                .latest_pos()
                .map_or(unplaced, |pos| rect.contains(pos));
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
        let menu_open = state.plus.is_open();
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
            state.refocus_again = menu_open;
            state.touch.regain = true;
            ctx.memory_mut(|mem| mem.request_focus(text_id));
        } else if std::mem::take(&mut state.refocus_again) {
            state.touch.regain = true;
            ctx.memory_mut(|mem| mem.request_focus(text_id));
        }

        ComposerResponse {
            action: out.action,
            text,
            accepted_suggestion: out.accepted,
            file_hover,
            files_dropped,
            provides_text_actions: actions.is_some(),
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
    /// A send was asked for.
    send: bool,
    /// Intent the send asked for; `None` uses [`Activity::send_intent`].
    intent: Option<SendIntent>,
    /// A popup pick key arrived; `true` when it was Enter. Applied after the text edit.
    pick: Option<bool>,
    /// A text action button was tapped; applied after the text edit.
    text_action: Option<TextAction>,
}

/// The composer that takes files dropped without a pointer position, and the pass it was drawn in.
#[derive(Clone, Copy)]
struct DropClaim {
    id: Id,
    pass: u64,
}

/// Whether the composer `id` takes files dropped without a pointer position: the last focused one still drawn, else the first drawn this pass.
fn claims_unplaced_drop(ctx: &Context, id: Id, focused: bool) -> bool {
    let target_key = Id::unique("egui_theme_kit.chat.composer.drop_target");
    let first_key = Id::unique("egui_theme_kit.chat.composer.first_drawn");
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| {
        if focused {
            d.insert_temp(target_key, DropClaim { id, pass });
        }
        let first = match d.get_temp::<DropClaim>(first_key) {
            Some(c) if c.pass == pass => c.id,
            _ => {
                d.insert_temp(first_key, DropClaim { id, pass });
                id
            }
        };
        match d.get_temp::<DropClaim>(target_key) {
            Some(c) if c.id == id => {
                d.insert_temp(target_key, DropClaim { id, pass });
                true
            }
            Some(c) if pass.saturating_sub(c.pass) <= 1 => false,
            _ => first == id,
        }
    })
}

/// Whether `text` invokes a known slash command.
fn is_command(text: &str, slash: Option<&dyn SlashSource>) -> bool {
    text.starts_with('/')
        && slash.is_some_and(|s| slash::parse_command(s, text.trim_start()).is_some())
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
            Resolution::Label(label) => {
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
            Resolution::Missing => unresolved.push(!state.was_picked(&target)),
            Resolution::Unknown => unresolved.push(false),
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
    state: &mut ComposerState,
    enter: EnterBehavior,
    activity: Activity,
    out: &mut Outcome,
) {
    let plain = |m: Modifiers| m.is_none();
    let shift_only = |m: Modifiers| m.shift && !m.alt && !m.ctrl && !m.command && !m.mac_cmd;
    if let Some(popup) = state.popup.as_mut() {
        let (down, up, by_enter, by_tab, back, close) = ctx.input_mut(|i| {
            let keys = (
                take_key(i, Key::ArrowDown, plain),
                take_key(i, Key::ArrowUp, plain),
                take_key(i, Key::Enter, plain),
                take_key(i, Key::Tab, plain),
                take_key(i, Key::Tab, shift_only),
                take_key(i, Key::Escape, plain),
            );
            take_key(i, Key::Tab, |_| true);
            keys
        });
        popup.step(down as isize - up as isize - back as isize);
        if close > 0 {
            state.dismissed = Some(popup.trigger.at());
            state.popup = None;
        } else if by_enter + by_tab > 0 {
            out.pick = Some(by_enter > 0);
        }
        return;
    }
    let (sent, alternate) = ctx.input_mut(|i| {
        let alternate = take_key(i, Key::Enter, |m| m.alt && !m.shift && !m.command);
        let command = take_key(i, Key::Enter, |m| m.command && !m.shift && !m.alt);
        let bare = match enter {
            EnterBehavior::Send => take_key(i, Key::Enter, plain),
            EnterBehavior::Newline => 0,
        };
        (command + bare > 0, alternate > 0)
    });
    if alternate && let Some(intent) = activity.alternate_intent() {
        out.send = true;
        out.intent = Some(intent);
    }
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
    intent: Option<SendIntent>,
    attachments: &[AttachmentChip<'_>],
    slash: Option<&dyn SlashSource>,
) -> Option<ComposerAction> {
    if state.is_empty() && attachments.is_empty() {
        return None;
    }
    let message = state.message();
    if let Some(parsed) = slash.and_then(|s| slash::parse_command(s, message.wire.trim_start())) {
        let slash::Parsed::Run { id, args } = parsed else {
            return None;
        };
        state.clear();
        return Some(ComposerAction::Command { id, args });
    }
    let intent = intent
        .filter(|i| mode.activity.allows(*i))
        .or_else(|| mode.activity.send_intent())?;
    if mode.send_blocked.is_some() {
        return None;
    }
    state.clear();
    Some(ComposerAction::Send { intent, message })
}

/// Carries out a tap on the text action row.
fn apply_text_action(
    ctx: &Context,
    id: Id,
    state: &mut ComposerState,
    action: TextAction,
    clipboard: Option<ClipboardRead<'_>>,
) {
    let selection = cursor_of(ctx, id)
        .map(|(a, b)| a.min(b)..a.max(b))
        .filter(|r| !r.is_empty());
    match action {
        TextAction::SelectAll => store_cursor(ctx, id, 0, state.char_len(), None),
        TextAction::Copy | TextAction::Cut => {
            let Some(range) = selection else {
                return;
            };
            ctx.copy_text(selected_wire(state, range.clone()));
            if action == TextAction::Cut {
                let cursor = state.replace(range, "");
                store_cursor(ctx, id, cursor, cursor, Some(state.text()));
            }
        }
        TextAction::Paste => match clipboard {
            Some(read) => state.touch.pending_paste = read().filter(|t| !t.is_empty()),
            None => ctx.send_viewport_cmd(ViewportCommand::RequestPaste),
        },
    }
    state.touch.track = true;
    ctx.request_repaint();
}

/// Chars `range` of the buffer with whole chips in wire form.
fn selected_wire(state: &ComposerState, range: std::ops::Range<usize>) -> String {
    edit::wire_slice(state.text(), state.spans(), range.clone()).unwrap_or_else(|| {
        state
            .text()
            .chars()
            .skip(range.start)
            .take(range.len())
            .collect()
    })
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
    let focused =
        editable && (ctx.memory(|mem| mem.has_focus(id)) || state.refocus || state.refocus_again);
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
    /// Points a touch drag moved the text down by.
    scroll_by: f32,
    /// Scroll the cursor into view.
    track: bool,
}

impl TextArea<'_> {
    /// Draws the text in a child of `ui` over `rect`; returns its response and the height it used.
    fn show_in(self, ui: &mut Ui, state: &mut ComposerState, rect: Rect) -> (Response, f32) {
        let p = ChatPalette::of(ui);
        let font = TextStyle::Body.resolve(ui.style());
        let row_h = ui.fonts_mut(|f| f.row_height(&font));
        let spans = state.spans().to_vec();
        let unresolved = self.unresolved.to_vec();
        let before = state.text().to_owned();
        let mut layouter = move |ui: &Ui, buf: &dyn TextBuffer, wrap: f32| -> Arc<Galley> {
            let text = buf.as_str();
            let mut job = if text == before {
                chip_job(text, &spans, &unresolved, &font, &p)
            } else {
                let (moved, flags): (Vec<_>, Vec<_>) = edit::follow_edit(&before, text, &spans)
                    .into_iter()
                    .zip(spans.iter().zip(&unresolved))
                    .filter_map(|(chars, (span, dead))| {
                        chars.map(|chars| {
                            let target = span.target.clone();
                            (state::MentionSpan { chars, target }, *dead)
                        })
                    })
                    .unzip();
                chip_job(text, &moved, &flags, &font, &p)
            };
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
        let mut area = ScrollArea::vertical()
            .id_salt(self.scroll)
            .max_height(self.max_h)
            .auto_shrink([false, true])
            .animated(false);
        if self.scroll_by != 0.0 {
            area = area.vertical_scroll_offset(state.touch.scroll_y - self.scroll_by);
        }
        let output = area.show(&mut child, |ui| {
            let edit = TextEdit::multiline(state.buffer_mut());
            let filter = edit.get_event_filter();
            let out = edit
                .event_filter(EventFilter {
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
                .show(ui);
            let focused = ui.memory(|mem| mem.has_focus(self.id));
            let cursor = out.cursor_range.map(|r| {
                let mut rect = out.galley.pos_from_cursor(r.primary);
                if rect.height() < 1.0 {
                    rect.max.y = rect.min.y + row_h;
                }
                rect.translate(out.galley_pos.to_vec2())
            });
            if let Some(rect) = cursor
                && (self.track || (focused && out.response.response.changed()))
            {
                ui.scroll_to_rect(rect, None);
            }
            (
                out.response.response,
                cursor,
                focused,
                touch::text_hash(state.text()),
            )
        });
        let (response, cursor, focused, drawn) = output.inner;
        let height = child.min_rect().height();
        let view = Rect::from_min_size(
            output.inner_rect.min,
            vec2(
                output.inner_rect.width(),
                output.inner_rect.height().min(height),
            ),
        );
        let t = &mut state.touch;
        let resized = (view.height() - t.view.height()).abs() > RELAYOUT_EPS
            || (self.max_h - t.max_h).abs() > RELAYOUT_EPS;
        t.view = view;
        t.scroll_y = output.state.clamped_offset().y;
        t.scroll_max = (output.content_size.y - view.height()).max(0.0);
        t.max_h = self.max_h;
        t.cursor = cursor.unwrap_or(Rect::NOTHING);
        t.drawn = drawn;
        t.track = focused && resized && t.gesture != Gesture::Scrolling;
        if t.track {
            ui.ctx().request_repaint();
        }
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
                            ui.add(
                                Image::new(SizedTexture::new(texture, vec2(m.icon_lg, m.icon_lg)))
                                    .uv(cover_uv(ui.ctx(), texture)),
                            );
                        }
                        let font = TextStyle::Small.resolve(ui.style());
                        let galley = elided(ui, &chip.name, font, ATTACHMENT_NAME_W);
                        let (rect, _) = ui.allocate_exact_size(galley.size(), Sense::hover());
                        let ink = if chip.reading {
                            look.p.text_dim
                        } else if chip.failed {
                            look.p.error
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
    /// The text is a known slash command, which runs whatever the activity.
    command: bool,
    leading: Option<Slot<'a>>,
    trailing: Option<Slot<'a>>,
    /// The text action row, shown on touch while the text has focus.
    actions: Option<Actions>,
}

/// Which text actions apply now.
#[derive(Clone, Copy)]
struct Actions {
    /// Some text is selected.
    copy: bool,
    /// There is text to select.
    select: bool,
}

/// Width of the text action row with buttons of side `side`.
fn action_row_w(side: f32, gap: f32) -> f32 {
    TextAction::ALL.len() as f32 * (side + gap)
}

/// Cut, copy, paste and select-all buttons; records their rects and the tapped one.
fn action_buttons(
    ui: &mut Ui,
    look: &Look,
    actions: Actions,
    state: &mut ComposerState,
    out: &mut Outcome,
) {
    for action in TextAction::ALL {
        let (icon, enabled) = match action {
            TextAction::Cut => (look.icons.cut, actions.copy),
            TextAction::Copy => (look.icons.copy, actions.copy),
            TextAction::Paste => (look.icons.paste, true),
            TextAction::SelectAll => (look.icons.select_all, actions.select),
        };
        let response = IconButton::new(icon, look.icon_box)
            .tip(action.label())
            .enabled(enabled)
            .show(ui);
        state.touch.buttons.push((action, response.rect));
        if response.clicked() {
            out.text_action = Some(action);
        }
    }
}

impl Controls<'_, '_> {
    /// Lays out the controls and `text`: one row when wide, a control row over the text when narrow.
    fn lay_out(
        mut self,
        ui: &mut Ui,
        state: &mut ComposerState,
        text: TextArea<'_>,
        narrow: bool,
        out: &mut Outcome,
    ) -> (Response, Option<SendIntent>) {
        let side = self.look.icon_box.side(&Metrics::get(ui.ctx()));
        let gap = ui.spacing().item_spacing;
        let origin = ui.available_rect_before_wrap().min;
        let width = ui.available_width();
        let max_h = text.max_h;
        if narrow {
            let bar = Rect::from_min_size(origin, vec2(width, side));
            let base_left = if state.touch.base_left_w > 0.0 {
                state.touch.base_left_w
            } else {
                side + gap.x
            };
            let apart = self
                .actions
                .filter(|_| base_left + action_row_w(side, gap.x) + state.right_w > width);
            if apart.is_some() {
                self.actions = None;
            }
            let look = self.look;
            let clicked = self.sides(ui, state, bar, Align::Center, out);
            let mut top = bar.bottom() + gap.y;
            if let Some(actions) = apart {
                let row = Rect::from_min_size(pos2(origin.x, top), vec2(width, side));
                let mut strip = ui.new_child(
                    UiBuilder::new()
                        .id_salt("egui_theme_kit.chat.composer.actions")
                        .max_rect(row)
                        .layout(Layout::left_to_right(Align::Center)),
                );
                action_buttons(&mut strip, look, actions, state, out);
                top = row.bottom() + gap.y;
            }
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

    /// Leading and `+` at the left of `row`, the rest at its right; returns the intent of a clicked send button.
    fn sides(
        self,
        ui: &mut Ui,
        state: &mut ComposerState,
        row: Rect,
        align: Align,
        out: &mut Outcome,
    ) -> Option<SendIntent> {
        let Self {
            look,
            mode,
            show_attach,
            can_send,
            command,
            leading,
            trailing,
            actions,
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
        state.touch.base_left_w = left.min_rect().width() + gap;
        if let Some(actions) = actions {
            action_buttons(&mut left, look, actions, state, out);
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
        if mode.activity.is_busy() {
            let stop = IconButton::new(look.icons.stop, look.icon_box)
                .tip("Stop")
                .enabled(mode.stop_blocked.is_none())
                .show(&mut right);
            let stop = match &mode.stop_blocked {
                Some(reason) => stop.on_disabled_hover_text(reason.as_ref()),
                None => stop,
            };
            if stop.clicked() {
                out.action = Some(ComposerAction::Stop);
            }
        }
        let alternate = mode
            .activity
            .alternate_intent()
            .filter(|_| !command)
            .and_then(|intent| send_button(&mut right, look, mode, can_send, Some(intent), false));
        let primary = send_button(&mut right, look, mode, can_send, None, command);
        remember(ui, &mut state.right_w, right.min_rect().width());
        alternate.or(primary)
    }
}

/// Stores a measured width and repaints when it moved.
fn remember(ui: &Ui, slot: &mut f32, width: f32) {
    if (width - *slot).abs() > RELAYOUT_EPS {
        *slot = width;
        ui.ctx().request_repaint();
    }
}

/// A send, steer or queue button for `intent`, else for the activity's intent; returns the intent when clicked.
fn send_button(
    ui: &mut Ui,
    look: &Look,
    mode: &ComposerMode<'_>,
    has_content: bool,
    intent: Option<SendIntent>,
    command: bool,
) -> Option<SendIntent> {
    let alternate = intent.is_some();
    let intent = intent.or_else(|| mode.activity.send_intent());
    let (icon, tip) = match (command, intent) {
        (true, _) => (look.icons.send, "Run command"),
        (false, Some(SendIntent::Steer)) => (look.icons.steer, "Steer (send now)"),
        (false, Some(SendIntent::Queue)) => (look.icons.queue, "Queue for after this turn"),
        (false, _) => (look.icons.send, "Send"),
    };
    let tip = if alternate {
        format!("{tip} \u{b7} Alt+Enter")
    } else {
        tip.to_owned()
    };
    let enabled = has_content && (command || (intent.is_some() && mode.send_blocked.is_none()));
    let tint = if alternate {
        look.p.text_dim
    } else {
        look.p.accent
    };
    let response = IconButton::new(icon, look.icon_box)
        .tip(&tip)
        .tint(tint)
        .enabled(enabled)
        .show(ui);
    let response = match &mode.send_blocked {
        Some(reason) if !command => response.on_disabled_hover_text(reason.as_ref()),
        _ => response,
    };
    response
        .clicked()
        .then_some(intent.unwrap_or(SendIntent::Start))
}
