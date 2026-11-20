//! The chat transcript: user bubbles, agent markdown, notices, folded work groups and cards.

mod board;
mod cards;
mod entry;
mod group;
#[cfg(feature = "chat-json")]
mod json;

use std::collections::HashMap;

use egui::{AsIdSalt, IdSalt, Ui};

pub use board::{BoardAction, BoardGroup, WaitingBoard};
pub use cards::{
    ApprovalCard, CardChoice, CardState, Question, QuestionCard, QuestionForm, QuestionItem,
    floating_cards,
};
pub use entry::{
    AgentMessage, AttachmentChip, ChatEntry, Delivery, EntryKind, Notice, Payload, UserMessage,
    WorkItem, WorkKind, WorkStatus,
};
pub use group::{Run, WorkStats, runs, work_summary};
#[cfg(feature = "chat-json")]
pub use json::{
    PayloadRole, PayloadTree, PayloadTreeResponse, TreeBudget, count_nodes, json_visuals,
};

use super::mention::MentionSource;
use super::types::{ChatAction, CowStr, MessageNav, TimeFormat, UnixMs};

/// Entries the transcript reads by index, so the app builds only the rows in view.
pub trait TranscriptSource {
    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The variant of entry `i`; cheap, used for folding.
    fn kind(&self, i: usize) -> EntryKind;

    fn key(&self, i: usize) -> CowStr<'_>;

    fn entry(&self, i: usize) -> ChatEntry<'_>;

    /// Changes whenever entry `i` changes; `None` disables height caching for it.
    fn revision(&self, i: usize) -> Option<u64>;
}

impl TranscriptSource for [ChatEntry<'_>] {
    fn len(&self) -> usize {
        <[ChatEntry<'_>]>::len(self)
    }

    fn kind(&self, i: usize) -> EntryKind {
        self[i].kind()
    }

    fn key(&self, i: usize) -> CowStr<'_> {
        CowStr::Borrowed(self[i].key())
    }

    fn entry(&self, i: usize) -> ChatEntry<'_> {
        self[i].clone()
    }

    fn revision(&self, _i: usize) -> Option<u64> {
        None
    }
}

/// How much of the agent's work the transcript shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DetailLevel {
    /// Work groups stay closed unless a call failed.
    #[default]
    Simple,
    /// Work groups open while a call runs or after one failed.
    Detailed,
}

/// State of the history before the first entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LoadState {
    /// More history can be requested.
    Available,
    Loading,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TranscriptOptions {
    pub detail: DetailLevel,
    /// Draws reasoning lines inside work groups.
    pub show_thinking: bool,
    /// Width cap of the content column.
    pub max_width: Option<f32>,
    /// Current time, for running durations and today's labels.
    pub now: Option<UnixMs>,
}

impl Default for TranscriptOptions {
    fn default() -> Self {
        Self {
            detail: DetailLevel::Simple,
            show_thinking: true,
            max_width: None,
            now: None,
        }
    }
}

/// A scroll target.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Jump {
    Key(String),
    Bottom,
    PrevUser,
    NextUser,
    /// The user's message at this zero-based index.
    User(usize),
}

/// Scroll and fold state the app keeps per transcript.
#[derive(Clone, Debug, Default)]
pub struct TranscriptState {
    following: bool,
    pending: Option<Jump>,
    stick_frames: u32,
    group_open: HashMap<String, bool>,
    heights: HashMap<String, (u64, f32)>,
}

impl TranscriptState {
    /// Scrolls to `to` on the next frame, opening the work group that holds it.
    pub fn jump(&mut self, to: Jump) {
        self.pending = Some(to);
    }

    /// Whether the view follows new entries at the bottom.
    pub fn following(&self) -> bool {
        self.following
    }

    /// Keeps the view pinned to the bottom for the next `frames` frames.
    pub fn stick_for(&mut self, frames: u32) {
        self.stick_frames = self.stick_frames.max(frames);
        self.following = true;
    }
}

/// What a transcript frame produced.
#[derive(Clone, Debug, Default)]
pub struct TranscriptResponse {
    pub actions: Vec<ChatAction>,
    pub nav: MessageNav,
    pub at_bottom: bool,
}

/// A virtualised, scrolling chat transcript.
pub struct Transcript<'a> {
    id_salt: IdSalt,
    options: TranscriptOptions,
    mentions: Option<&'a dyn MentionSource>,
    time_format: Option<&'a dyn TimeFormat>,
    older: Option<LoadState>,
}

impl<'a> Transcript<'a> {
    pub fn new(id_salt: impl AsIdSalt) -> Self {
        Self {
            id_salt: IdSalt::new(id_salt),
            options: TranscriptOptions::default(),
            mentions: None,
            time_format: None,
            older: None,
        }
    }

    pub fn options(mut self, options: TranscriptOptions) -> Self {
        self.options = options;
        self
    }

    /// Resolves mention chips to current labels.
    pub fn mentions(mut self, mentions: &'a dyn MentionSource) -> Self {
        self.mentions = Some(mentions);
        self
    }

    /// Formats entry times; [`super::types::ClockFormat`] by default.
    pub fn time_format(mut self, format: &'a dyn TimeFormat) -> Self {
        self.time_format = Some(format);
        self
    }

    /// Shows a load-earlier row above the first entry.
    pub fn older(mut self, state: LoadState) -> Self {
        self.older = Some(state);
        self
    }

    /// Draws the entries of `src`; `custom` draws [`ChatEntry::Custom`] rows by key.
    pub fn show(
        self,
        ui: &mut Ui,
        state: &mut TranscriptState,
        src: &dyn TranscriptSource,
        custom: impl FnMut(&mut Ui, &str),
    ) -> TranscriptResponse {
        let Self {
            id_salt,
            options,
            mentions,
            time_format,
            older,
        } = self;
        state.pending = None;
        state.stick_frames = state.stick_frames.saturating_sub(1);
        let _ = (state.group_open.len(), state.heights.len());
        let _ = (
            ui,
            src,
            custom,
            id_salt,
            options,
            mentions.is_some(),
            time_format.is_some(),
            older,
        );
        TranscriptResponse::default()
    }
}
