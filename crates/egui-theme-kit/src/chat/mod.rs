//! Shared AI chat widgets: a transcript, a session title card and app bar, a session list and a composer.
//!
//! Widgets take borrowed views of app data and return actions; none of them does I/O, networking or
//! permission checks. Colours come from [`ChatPalette`], derived from the kit [`crate::Tokens`];
//! surfaces are painted by the installed [`ChatSurface`]; icons come from [`ChatIcons`].

pub mod composer;
mod icons;
pub mod mention;
mod palette;
pub mod session_bar;
mod surface;
pub mod transcript;
mod types;

#[cfg(test)]
mod tests;

pub use composer::{
    Activity, Composer, ComposerAction, ComposerMode, ComposerResponse, ComposerState,
    EnterBehavior, MentionSpan, OutgoingText, PlusItem, SendIntent, SlashCommand, SlashSource,
};
pub use icons::ChatIcons;
pub use mention::{
    MentionCandidate, MentionRef, MentionSource, WireSegment, clean_label, encode, segments,
};
pub use palette::{ChatPalette, JsonColors, PaletteBuilder, ShellColors, legible};
pub use session_bar::{
    AppBarAction, BarButton, ChatAppBar, ChipKind, MenuEntry, NewSession, NewSessionButton,
    OptionChip, RowKind, SessionBar, SessionBarAction, SessionList, SessionListAction, SessionRow,
    SessionSection, ViewItem, ViewMenu,
};
pub use surface::{
    ChatSurface, FlatSurface, KitGlassSurface, SurfaceKind, paint_surface, set_surface, surface,
    surface_frame,
};
pub use transcript::{
    AgentMessage, ApprovalCard, AttachmentChip, BoardAction, BoardGroup, CONFIRM_NO, CONFIRM_YES,
    CardChoice, CardState, ChatEntry, Delivery, DetailLevel, EntryKind, Jump, LoadState, Notice,
    Payload, PlanCard, PlanStatus, PlanStep, Question, QuestionCard, QuestionForm, QuestionItem,
    Run, Transcript, TranscriptOptions, TranscriptResponse, TranscriptSource, TranscriptState,
    UserMessage, WaitingBoard, WorkItem, WorkKind, WorkStats, WorkStatus, floating_cards,
    is_command_key, plan_progress, runs, work_summary,
};
#[cfg(feature = "chat-json")]
pub use transcript::{
    PayloadRole, PayloadTree, PayloadTreeResponse, TreeBudget, count_nodes, json_visuals,
};
pub use types::{
    Answer, CardAction, ChatAction, ClockFormat, CowList, CowStr, MessageNav, NavStep,
    SessionStatus, TimeFormat, Tone, UnixMs, format_duration, item_id,
};
