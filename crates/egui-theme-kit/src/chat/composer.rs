//! The message composer: `+` menu, growing text box with mention chips and slash commands, attach and send.

mod slash;
mod state;

use egui::{AsIdSalt, Id, IdSalt, Response, TextEdit, Ui};

pub use slash::{PlusItem, SlashCommand, SlashSource};
pub use state::{ComposerState, MentionSpan, OutgoingText};

use super::mention::MentionSource;
use super::transcript::AttachmentChip;
use super::types::CowStr;

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
    plus: &'a [PlusItem<'a>],
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
            plus: &[],
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

    /// App entries of the `+` menu.
    pub fn plus(mut self, items: &'a [PlusItem<'a>]) -> Self {
        self.plus = items;
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
        let Self {
            id_salt,
            hint,
            mode,
            attachments,
            enter,
            min_rows,
            max_height,
            show_attach,
            slash,
            mentions,
            plus,
            leading,
            trailing,
            above,
        } = self;
        let _ = (
            &mode,
            attachments,
            enter,
            max_height,
            show_attach,
            slash.is_some(),
            mentions.is_some(),
            plus,
            leading.is_some(),
            trailing.is_some(),
            above.is_some(),
        );
        let text = ui.add(
            TextEdit::multiline(state.buffer_mut())
                .id(text_id_of(id_salt))
                .hint_text(hint.as_ref())
                .desired_rows(min_rows),
        );
        ComposerResponse {
            action: None,
            text,
            accepted_suggestion: false,
            file_hover: false,
            files_dropped: false,
        }
    }
}
