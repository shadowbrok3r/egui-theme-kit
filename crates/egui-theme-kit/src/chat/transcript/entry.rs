//! Transcript entries the app hands the kit, as borrowed or owned views.

use std::borrow::Cow;

use egui::TextureId;

use super::cards::{ApprovalCard, QuestionCard};
use crate::chat::types::{CowList, CowStr, Tone, UnixMs};

/// One row of a transcript.
#[derive(Clone, Debug)]
pub enum ChatEntry<'a> {
    User(UserMessage<'a>),
    Agent(AgentMessage<'a>),
    Notice(Notice<'a>),
    Divider {
        key: CowStr<'a>,
        caption: CowStr<'a>,
    },
    /// Folded with its neighbours into one work group.
    Work(WorkItem<'a>),
    Approval(ApprovalCard<'a>),
    Question(QuestionCard<'a>),
    /// Drawn by the app's callback.
    Custom {
        key: CowStr<'a>,
    },
}

impl ChatEntry<'_> {
    pub fn key(&self) -> &str {
        match self {
            Self::User(m) => &m.key,
            Self::Agent(m) => &m.key,
            Self::Notice(n) => &n.key,
            Self::Divider { key, .. } | Self::Custom { key } => key,
            Self::Work(w) => &w.key,
            Self::Approval(c) => &c.key,
            Self::Question(c) => &c.key,
        }
    }

    pub fn kind(&self) -> EntryKind {
        match self {
            Self::User(_) => EntryKind::User,
            Self::Agent(_) => EntryKind::Agent,
            Self::Notice(_) => EntryKind::Notice,
            Self::Divider { .. } => EntryKind::Divider,
            Self::Work(w) => EntryKind::Work(w.kind),
            Self::Approval(_) => EntryKind::Approval,
            Self::Question(_) => EntryKind::Question,
            Self::Custom { .. } => EntryKind::Custom,
        }
    }
}

/// The variant of an entry without its data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EntryKind {
    User,
    Agent,
    Notice,
    Divider,
    Work(WorkKind),
    Approval,
    Question,
    Custom,
}

/// A message the user sent.
#[derive(Clone, Debug)]
pub struct UserMessage<'a> {
    pub key: CowStr<'a>,
    /// Wire form, mentions encoded.
    pub text: CowStr<'a>,
    pub author: Option<CowStr<'a>>,
    pub time: Option<UnixMs>,
    pub attachments: CowList<'a, AttachmentChip<'a>>,
    pub delivery: Delivery,
}

/// How far a user message got.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Delivery {
    #[default]
    Sent,
    Queued,
    Steered,
    Failed,
}

/// A markdown reply from the agent.
#[derive(Clone, Debug)]
pub struct AgentMessage<'a> {
    pub key: CowStr<'a>,
    pub text: CowStr<'a>,
    pub author: Option<CowStr<'a>>,
    pub time: Option<UnixMs>,
    /// Text is still arriving.
    pub streaming: bool,
}

/// A faint centred line, such as a reconnect or compaction note.
#[derive(Clone, Debug)]
pub struct Notice<'a> {
    pub key: CowStr<'a>,
    pub text: CowStr<'a>,
    pub time: Option<UnixMs>,
    pub tone: Tone,
}

/// A reasoning step, tool call, command or file change.
#[derive(Clone, Debug)]
pub struct WorkItem<'a> {
    pub key: CowStr<'a>,
    pub kind: WorkKind,
    /// Short label, such as the tool name.
    pub name: CowStr<'a>,
    /// One-line summary after the label.
    pub summary: CowStr<'a>,
    pub status: WorkStatus,
    pub started: Option<UnixMs>,
    pub duration_ms: Option<u64>,
    pub arguments: Option<Payload<'a>>,
    pub result: Option<Payload<'a>>,
    /// Reasoning text or command output.
    pub body: Option<CowStr<'a>>,
    /// Paths a file change touched.
    pub files: CowList<'a, CowStr<'a>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WorkKind {
    Reasoning,
    Tool,
    Command,
    FileChange,
    Other,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WorkStatus {
    Running,
    #[default]
    Ok,
    Failed,
    Declined,
    Pending,
}

/// Tool arguments or results; text is parsed as JSON when it is JSON, cached by content.
#[derive(Clone, Debug)]
pub enum Payload<'a> {
    Json(Cow<'a, serde_json::Value>),
    Text(CowStr<'a>),
}

/// A file or image attached to a user message.
#[derive(Clone, Debug)]
pub struct AttachmentChip<'a> {
    pub name: CowStr<'a>,
    pub image: Option<TextureId>,
    /// The app is still reading the file.
    pub reading: bool,
}
