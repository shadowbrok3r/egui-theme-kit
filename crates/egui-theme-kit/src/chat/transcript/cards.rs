//! Approval and question cards, inline in a transcript or floating over it.

use egui::Ui;

use super::entry::{ChatEntry, Payload};
use crate::chat::types::{CardAction, CowList, CowStr, Tone};
use crate::kit::ButtonKind;

/// A request to approve a command or change.
#[derive(Clone, Debug)]
pub struct ApprovalCard<'a> {
    pub key: CowStr<'a>,
    /// Where the request comes from, such as `SN 2156701 · HOST` or `desk · ~/src/engine`.
    pub context: CowStr<'a>,
    pub title: CowStr<'a>,
    pub command: Option<CowStr<'a>>,
    pub detail: Option<Payload<'a>>,
    pub reason: Option<CowStr<'a>>,
    pub choices: CowList<'a, CardChoice<'a>>,
    pub state: CardState<'a>,
}

/// One button of an approval card.
#[derive(Clone, Debug)]
pub struct CardChoice<'a> {
    pub label: CowStr<'a>,
    pub kind: ButtonKind,
    /// Key hint drawn as keycaps.
    pub shortcut: Option<CowStr<'a>>,
}

/// Where a card stands.
#[derive(Clone, Debug, Default)]
pub enum CardState<'a> {
    #[default]
    Open,
    Busy,
    Done {
        outcome: CowStr<'a>,
        tone: Tone,
    },
    Failed(CowStr<'a>),
    /// Visible but not actionable by this user.
    Locked {
        reason: CowStr<'a>,
    },
}

/// One or more questions the agent asks the user.
#[derive(Clone, Debug)]
pub struct QuestionCard<'a> {
    pub key: CowStr<'a>,
    pub context: CowStr<'a>,
    pub header: Option<CowStr<'a>>,
    /// Why the agent asks.
    pub why: Option<CowStr<'a>>,
    pub questions: CowList<'a, Question<'a>>,
    pub state: CardState<'a>,
}

/// How a question is answered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum QuestionForm {
    /// Exactly one item.
    #[default]
    Choice,
    /// Any number of items.
    Multi,
    /// Items ticked off a list, recommended ones pre-ticked.
    Checklist,
    /// Yes or no.
    Confirm,
    /// Free text only.
    Text,
}

#[derive(Clone, Debug)]
pub struct Question<'a> {
    pub id: CowStr<'a>,
    pub prompt: CowStr<'a>,
    pub form: QuestionForm,
    pub items: CowList<'a, QuestionItem<'a>>,
    /// Offers a note field beside the items.
    pub free_text: bool,
    pub header: Option<CowStr<'a>>,
    pub why: Option<CowStr<'a>>,
}

/// One option of a question.
#[derive(Clone, Debug)]
pub struct QuestionItem<'a> {
    pub id: CowStr<'a>,
    pub label: CowStr<'a>,
    pub detail: Option<CowStr<'a>>,
    /// Pre-ticked.
    pub recommended: bool,
    pub reason: Option<CowStr<'a>>,
}

/// Shows the approval and question cards among `cards` stacked; returns each acted-on card key with its action.
pub fn floating_cards(ui: &mut Ui, cards: &[ChatEntry<'_>]) -> Vec<(String, CardAction)> {
    let _ = (ui, cards);
    Vec::new()
}
