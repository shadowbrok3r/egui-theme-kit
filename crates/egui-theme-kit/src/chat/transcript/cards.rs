//! Approval, question and plan cards, inline in a transcript or floating over it.

use egui::{
    AsIdSalt, CornerRadius, CursorIcon, Id, InnerResponse, Key, KeyboardShortcut, Margin,
    Modifiers, Rect, Response, ScrollArea, Sense, Shape, Ui, Vec2, vec2,
};

use super::entry::{ChatEntry, Payload};
use super::markdown;
use super::payload::{self, Part};
use super::rich::{one_row, plain};
use super::row::{Fonts, RowCx};
use crate::chat::surface::{SurfaceKind, surface_frame};
use crate::chat::types::{Answer, CardAction, CowList, CowStr, Tone};
use crate::controls::{
    Icon, IconBox, IconButton, Kbd, SpinnerSize, TextArea, TextButton, TextField, icons,
    tinted_spinner,
};
use crate::kit::ButtonKind;

/// Picked id of a confirm question answered yes when it has no items.
pub const CONFIRM_YES: &str = "yes";
/// Picked id of a confirm question answered no when it has no items.
pub const CONFIRM_NO: &str = "no";
/// Share of the screen height floating cards may cover.
const FLOATING_SHARE: f32 = 0.45;
/// Label of the toggle that shows a cut command whole.
pub const SHOW_COMMAND: &str = "Show the whole command";
/// Label of the toggle that cuts a whole command back to [`ApprovalCard::command_max_lines`].
pub const SHORT_COMMAND: &str = "Show less";

/// A request to approve a command or change.
#[derive(Clone, Debug, Default)]
pub struct ApprovalCard<'a> {
    pub key: CowStr<'a>,
    /// Where the request comes from, such as `SN 2156701 · HOST` or `desk · ~/src/engine`.
    pub context: CowStr<'a>,
    pub title: CowStr<'a>,
    pub command: Option<CowStr<'a>>,
    /// Text the command's copy button copies in place of `command`, such as the whole command behind a cut preview.
    pub copy_text: Option<CowStr<'a>>,
    /// Most command lines shown; a longer command is cut with a toggle that shows it whole.
    pub command_max_lines: Option<usize>,
    pub detail: Option<Payload<'a>>,
    pub reason: Option<CowStr<'a>>,
    pub choices: CowList<'a, CardChoice<'a>>,
    pub state: CardState<'a>,
}

/// One button of an approval card.
#[derive(Clone, Debug, Default)]
pub struct CardChoice<'a> {
    pub label: CowStr<'a>,
    pub kind: ButtonKind,
    /// Keys written as `Y` or `Ctrl+Enter`, drawn as keycaps; pressing them chooses while the pointer is over the card and no text field has focus.
    pub shortcut: Option<CowStr<'a>>,
    /// Stays usable while the card is [`CardState::Locked`], such as a dismiss choice.
    pub while_locked: bool,
}

/// The shortcut written as `Y` or `Ctrl+Enter`; `None` when a part is unknown.
fn parse_shortcut(text: &str) -> Option<KeyboardShortcut> {
    let mut modifiers = Modifiers::NONE;
    let mut key = None;
    for part in text.split('+').map(str::trim) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "cmd" | "command" => modifiers |= Modifiers::COMMAND,
            "shift" => modifiers |= Modifiers::SHIFT,
            "alt" | "option" => modifiers |= Modifiers::ALT,
            _ if key.is_none() => {
                key = Key::from_name(part).or_else(|| Key::from_name(&part.to_ascii_uppercase()));
                key?;
            }
            _ => return None,
        }
    }
    Some(KeyboardShortcut::new(modifiers, key?))
}

/// The usable choice whose shortcut was pressed while the pointer is over `rect` and no text field has focus.
fn pressed_choice(
    ui: &Ui,
    rect: Rect,
    choices: &[CardChoice<'_>],
    usable: impl Fn(&CardChoice<'_>) -> bool,
) -> Option<usize> {
    if ui.ctx().egui_wants_keyboard_input() || !ui.rect_contains_pointer(rect) {
        return None;
    }
    choices.iter().position(|c| {
        usable(c)
            && c.shortcut
                .as_deref()
                .and_then(parse_shortcut)
                .is_some_and(|s| ui.input_mut(|i| i.consume_shortcut(&s)))
    })
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
    /// Visible but not actionable by this user; choices draw disabled with `reason` as their tooltip, except [`CardChoice::while_locked`] ones.
    Locked {
        reason: CowStr<'a>,
    },
}

impl CardState<'_> {
    /// Whether the user can still act on the card.
    pub fn actionable(&self) -> bool {
        matches!(self, Self::Open | Self::Failed(_))
    }

    /// The reason of a locked card.
    fn lock_reason(&self) -> Option<&str> {
        match self {
            Self::Locked { reason } => Some(reason),
            _ => None,
        }
    }
}

/// Shows the lock reason as the tooltip of a disabled button.
fn lock_tip(response: Response, locked: Option<&str>) -> Response {
    match locked {
        Some(reason) => response.on_disabled_hover_text(reason),
        None => response,
    }
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
    /// Any number of items, at least one.
    Multi,
    /// Items ticked off a list, recommended ones pre-ticked; none is a valid answer.
    Checklist,
    /// Yes or no; two items replace the default pair.
    Confirm,
    /// Free text only, returned as the note.
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
    /// Pre-ticked on a checklist and tagged on other forms.
    pub recommended: bool,
    pub reason: Option<CowStr<'a>>,
}

/// The agent's step list.
#[derive(Clone, Debug, Default)]
pub struct PlanCard<'a> {
    pub key: CowStr<'a>,
    pub title: Option<CowStr<'a>>,
    pub explanation: Option<CowStr<'a>>,
    pub steps: CowList<'a, PlanStep<'a>>,
    /// Tooltip of a close button in the header; tapping it reports [`CardAction::Dismiss`] for `key`.
    pub dismiss: Option<CowStr<'a>>,
    /// Fold state the card starts in, `None` for open; a new `key` or a changed value resets the user's fold.
    pub open: Option<bool>,
}

/// One step of a plan.
#[derive(Clone, Debug)]
pub struct PlanStep<'a> {
    pub text: CowStr<'a>,
    pub status: PlanStatus,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PlanStatus {
    #[default]
    Pending,
    InProgress,
    Completed,
}

impl PlanCard<'_> {
    /// Steps completed.
    pub fn done(&self) -> usize {
        self.steps
            .iter()
            .filter(|s| s.status == PlanStatus::Completed)
            .count()
    }

    /// The step in progress, else the first pending one.
    pub fn current(&self) -> Option<&PlanStep<'_>> {
        self.steps
            .iter()
            .find(|s| s.status == PlanStatus::InProgress)
            .or_else(|| self.steps.iter().find(|s| s.status == PlanStatus::Pending))
    }
}

/// `2 of 5 done`.
pub fn plan_progress(plan: &PlanCard<'_>) -> String {
    format!("{} of {} done", plan.done(), plan.steps.len())
}

/// Unsent choices for one question.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Draft {
    picked: Vec<String>,
    text: String,
    note: String,
}

impl Draft {
    fn new(q: &Question<'_>) -> Self {
        let picked = match q.form {
            QuestionForm::Checklist => q
                .items
                .iter()
                .filter(|i| i.recommended)
                .map(|i| i.id.to_string())
                .collect(),
            _ => Vec::new(),
        };
        Self {
            picked,
            ..Self::default()
        }
    }

    fn has(&self, id: &str) -> bool {
        self.picked.iter().any(|p| p == id)
    }

    /// Selects `id` alone when `single`, else flips it.
    fn pick(&mut self, id: &str, single: bool) {
        if single {
            self.picked = vec![id.to_owned()];
        } else if self.has(id) {
            self.picked.retain(|p| p != id);
        } else {
            self.picked.push(id.to_owned());
        }
    }

    /// The answer this draft holds, picks in item order; `None` while it is incomplete.
    fn answer(&self, q: &Question<'_>) -> Option<Answer> {
        let note = Some(self.note.trim())
            .filter(|n| !n.is_empty())
            .map(str::to_owned);
        let in_order = |ids: &mut dyn Iterator<Item = String>| -> Vec<String> {
            ids.filter(|id| self.has(id)).collect()
        };
        let picked = match q.form {
            QuestionForm::Text => {
                let text = self.text.trim();
                return (!text.is_empty()).then(|| Answer {
                    question: q.id.to_string(),
                    picked: Vec::new(),
                    note: Some(text.to_owned()),
                });
            }
            QuestionForm::Checklist => in_order(&mut q.items.iter().map(|i| i.id.to_string())),
            QuestionForm::Choice | QuestionForm::Multi => {
                let picked = in_order(&mut q.items.iter().map(|i| i.id.to_string()));
                if picked.is_empty() && !(q.free_text && note.is_some()) {
                    return None;
                }
                picked
            }
            QuestionForm::Confirm => {
                let picked = in_order(&mut confirm_items(q).into_iter().map(|(id, _)| id));
                if picked.is_empty() {
                    return None;
                }
                picked
            }
        };
        Some(Answer {
            question: q.id.to_string(),
            picked,
            note,
        })
    }
}

/// The two choices of a confirm question as (id, label).
fn confirm_items(q: &Question<'_>) -> Vec<(String, String)> {
    if q.items.len() >= 2 {
        q.items
            .iter()
            .take(2)
            .map(|i| (i.id.to_string(), i.label.to_string()))
            .collect()
    } else {
        vec![
            (CONFIRM_YES.to_owned(), "Yes".to_owned()),
            (CONFIRM_NO.to_owned(), "No".to_owned()),
        ]
    }
}

/// Drafts of every question of a card, reset when its question ids change.
#[derive(Clone, Debug, Default)]
struct CardDraft {
    ids: Vec<String>,
    drafts: Vec<Draft>,
}

/// Id of the drafts of the card keyed `key` drawn in `view`.
fn draft_id(view: Id, key: &str) -> Id {
    view.with(("egui_theme_kit.chat.card_draft", key))
}

fn load_drafts(ui: &Ui, view: Id, card: &QuestionCard<'_>) -> CardDraft {
    let ids: Vec<String> = card.questions.iter().map(|q| q.id.to_string()).collect();
    ui.data(|d| d.get_temp::<CardDraft>(draft_id(view, &card.key)))
        .filter(|d| d.ids == ids)
        .unwrap_or_else(|| CardDraft {
            drafts: card.questions.iter().map(Draft::new).collect(),
            ids,
        })
}

/// Answers of every question, or `None` while one is incomplete.
fn answers(card: &QuestionCard<'_>, drafts: &CardDraft) -> Option<Vec<Answer>> {
    card.questions
        .iter()
        .zip(&drafts.drafts)
        .map(|(q, d)| d.answer(q))
        .collect()
}

/// A card's surface with its kind label and context on the first line.
fn frame<R>(
    ui: &mut Ui,
    rc: &mut RowCx<'_>,
    salt: impl AsIdSalt,
    head: Head<'_>,
    add: impl FnOnce(&mut Ui, &mut RowCx<'_>) -> R,
) -> InnerResponse<R> {
    let margin = Margin::symmetric(rc.m.pad.x as i8, (rc.m.gap.y + 2.0) as i8);
    let shown = surface_frame(ui, salt, SurfaceKind::Card, margin, |ui| {
        ui.set_min_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = rc.m.gap.y / 2.0;
        header(ui, rc, &head);
        add(ui, rc)
    });
    InnerResponse::new(shown.inner, shown.response)
}

/// The first line of a card: an icon, the card kind and its context.
struct Head<'h> {
    icon: Icon,
    tone: Tone,
    kind: &'h str,
    context: &'h str,
    /// Width kept clear at the right end for header buttons.
    trail: f32,
}

fn header(ui: &mut Ui, rc: &RowCx<'_>, head: &Head<'_>) {
    let Head {
        icon,
        tone,
        kind,
        context,
        trail,
    } = *head;
    let h = RowCx::row_h(ui, &rc.f.small).max(rc.m.icon_sm);
    let (full, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
    let rect = full.with_max_x((full.right() - trail).max(full.left()));
    let painter = ui.painter();
    let slot = Rect::from_min_size(rect.left_top(), Vec2::new(rc.m.icon_sm, h));
    icon.paint(ui, slot, rc.m.icon_sm, tone.color(&rc.p));
    let small_strong = crate::controls::medium_font_sized(ui, rc.f.small.size);
    let mut x = slot.right() + rc.m.gap.x / 2.0;
    let kind_g = one_row(ui, kind, &small_strong, rc.p.card_text, rect.right() - x);
    let y = |g: &egui::Galley| (rect.center().y - g.size().y / 2.0).round();
    painter.galley(egui::pos2(x, y(&kind_g)), kind_g.clone(), rc.p.card_text);
    x += kind_g.size().x;
    if !context.is_empty() {
        let text = format!("  {context}");
        let ctx_g = one_row(ui, &text, &rc.f.small, rc.p.text_dim, rect.right() - x);
        painter.galley(egui::pos2(x, y(&ctx_g)), ctx_g, rc.p.text_dim);
    }
}

/// Outcome line of a card that is done, failed, busy or locked; returns whether buttons stay usable.
fn state_line(ui: &mut Ui, rc: &RowCx<'_>, state: &CardState<'_>) -> bool {
    match state {
        CardState::Open => true,
        CardState::Busy => false,
        CardState::Done { outcome, tone } => {
            status(ui, rc, icons::CHECK_CIRCLE, tone.color(&rc.p), outcome);
            false
        }
        CardState::Failed(reason) => {
            status(ui, rc, icons::ERROR, rc.p.error, reason);
            true
        }
        CardState::Locked { reason } => {
            status(ui, rc, rc.icons.approval, rc.p.text_dim, reason);
            false
        }
    }
}

fn status(ui: &mut Ui, rc: &RowCx<'_>, icon: Icon, color: egui::Color32, text: &str) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = rc.m.gap.x / 2.0;
        let h = RowCx::row_h(ui, &rc.f.body);
        let (slot, _) = ui.allocate_exact_size(vec2(rc.m.icon_sm, h), Sense::hover());
        icon.paint(ui, slot, rc.m.icon_sm, color);
        plain(ui, rc, text, &rc.f.body, color);
    });
}

/// `command` cut to `max` lines, the last saying how many were left out; `None` when it fits.
pub(crate) fn cut_command(command: &str, max: usize) -> Option<String> {
    let total = command.lines().count();
    if total <= max {
        return None;
    }
    let keep = max.saturating_sub(1).max(1);
    let mut out: Vec<&str> = command.lines().take(keep).collect();
    let note = match total - keep {
        1 => "… 1 more line".to_owned(),
        n => format!("… {n} more lines"),
    };
    out.push(&note);
    Some(out.join(
        "
",
    ))
}

/// The command of `card` with its copy text, cut to its line cap unless shown whole, and its toggle.
fn approval_command(ui: &mut Ui, rc: &mut RowCx<'_>, card: &ApprovalCard<'_>, view: Id) {
    let Some(command) = &card.command else {
        return;
    };
    let key = card.key.as_ref();
    let whole_id = view.with((key, "command_whole"));
    let whole = ui.data(|d| d.get_temp::<bool>(whole_id)).unwrap_or(false);
    let cut = card
        .command_max_lines
        .and_then(|max| cut_command(command, max));
    let shown = match (&cut, whole) {
        (Some(cut), false) => cut.as_str(),
        _ => command.as_ref(),
    };
    let copy = card.copy_text.as_deref().unwrap_or(command);
    markdown::code_block_copying(ui, rc, "shell", shown, copy, view.with((key, "command")));
    if cut.is_some() {
        let label = if whole { SHORT_COMMAND } else { SHOW_COMMAND };
        let toggle = TextButton::new(label).kind(ButtonKind::Ghost).show(ui);
        if rc.tapped(&toggle) {
            ui.data_mut(|d| d.insert_temp(whole_id, !whole));
        }
    }
}

/// Draws an approval card; `view` keys its payload trees.
pub(crate) fn approval(
    ui: &mut Ui,
    rc: &mut RowCx<'_>,
    card: &ApprovalCard<'_>,
    view: Id,
) -> Option<CardAction> {
    let key = card.key.as_ref();
    let mut action = None;
    let icon = rc.icons.approval;
    let shown = frame(
        ui,
        rc,
        ("approval", key),
        Head {
            icon,
            tone: Tone::Warn,
            kind: "Approval",
            context: &card.context,
            trail: 0.0,
        },
        |ui, rc| {
            let title_font = rc.f.strong.clone();
            plain(ui, rc, &card.title, &title_font, rc.p.card_text);
            approval_command(ui, rc, card, view);
            if let Some(detail) = &card.detail {
                payload::show(
                    ui,
                    rc,
                    detail,
                    view.with((key, "detail")),
                    Part::Detail,
                    None,
                );
            }
            if let Some(reason) = &card.reason {
                let font = rc.f.small.clone();
                plain(ui, rc, reason, &font, rc.p.text_dim);
            }
            let usable = state_line(ui, rc, &card.state);
            let busy = matches!(card.state, CardState::Busy);
            let locked = card.state.lock_reason();
            if usable || busy || locked.is_some() {
                ui.horizontal_wrapped(|ui| {
                    for (n, choice) in card.choices.iter().enumerate() {
                        let enabled = usable || (locked.is_some() && choice.while_locked);
                        let button = TextButton::new(&choice.label)
                            .kind(choice.kind)
                            .enabled(enabled)
                            .show(ui);
                        let button = lock_tip(button, locked);
                        if let Some(keys) = &choice.shortcut {
                            Kbd::new(keys).show(ui);
                        }
                        if rc.tapped(&button) {
                            action = Some(CardAction::Choose(n));
                        }
                    }
                    if busy {
                        tinted_spinner(ui, SpinnerSize::Sm, rc.p.running);
                    }
                });
            }
        },
    );
    let open = card.state.actionable();
    let locked = card.state.lock_reason().is_some();
    if action.is_none()
        && (open || locked)
        && let Some(n) = pressed_choice(ui, shown.response.rect, &card.choices, |c| {
            open || c.while_locked
        })
    {
        action = Some(CardAction::Choose(n));
    }
    action
}

/// Draws a question card; drafts live in temp memory keyed by the card key.
pub(crate) fn question(
    ui: &mut Ui,
    rc: &mut RowCx<'_>,
    card: &QuestionCard<'_>,
    view: Id,
) -> Option<CardAction> {
    let key = card.key.as_ref();
    let mut drafts = load_drafts(ui, view, card);
    let before = drafts.drafts.clone();
    let mut action = None;
    let icon = rc.icons.question;
    let _ = frame(
        ui,
        rc,
        ("question", key),
        Head {
            icon,
            tone: Tone::Info,
            kind: "Question",
            context: &card.context,
            trail: 0.0,
        },
        |ui, rc| {
            if let Some(header) = &card.header {
                let font = rc.f.strong.clone();
                plain(ui, rc, header, &font, rc.p.card_text);
            }
            if let Some(why) = &card.why {
                let font = rc.f.small.clone();
                plain(ui, rc, why, &font, rc.p.text_dim);
            }
            let usable = matches!(card.state, CardState::Open | CardState::Failed(_));
            let single_confirm =
                card.questions.len() == 1 && card.questions[0].form == QuestionForm::Confirm;
            for (n, (q, draft)) in card.questions.iter().zip(&mut drafts.drafts).enumerate() {
                if n > 0 {
                    crate::controls::hairline(ui);
                }
                ui.add_enabled_ui(usable, |ui| {
                    question_body(ui, rc, q, draft, view.with((key, n)), single_confirm);
                });
            }
            let usable = state_line(ui, rc, &card.state) && usable;
            let busy = matches!(card.state, CardState::Busy);
            let locked = card.state.lock_reason();
            if !(usable || busy || locked.is_some()) {
                return;
            }
            ui.horizontal_wrapped(|ui| {
                if single_confirm {
                    let q = &card.questions[0];
                    for (n, (id, label)) in confirm_items(q).into_iter().enumerate() {
                        let kind = if n == 0 {
                            ButtonKind::Primary
                        } else {
                            ButtonKind::Neutral
                        };
                        let button = TextButton::new(&label).kind(kind).enabled(usable).show(ui);
                        let button = lock_tip(button, locked);
                        if rc.tapped(&button) {
                            let note = drafts.drafts[0].note.trim();
                            action = Some(CardAction::Answer(vec![Answer {
                                question: q.id.to_string(),
                                picked: vec![id],
                                note: (!note.is_empty()).then(|| note.to_owned()),
                            }]));
                        }
                    }
                } else {
                    let ready = answers(card, &drafts);
                    let send = TextButton::new("Send answer")
                        .kind(ButtonKind::Primary)
                        .icon(rc.icons.send)
                        .enabled(usable && ready.is_some())
                        .show(ui);
                    let send = lock_tip(send, locked);
                    if rc.tapped(&send)
                        && let Some(ready) = ready
                    {
                        action = Some(CardAction::Answer(ready));
                    }
                }
                let skip = TextButton::new("Skip")
                    .kind(ButtonKind::Ghost)
                    .enabled(usable)
                    .show(ui)
                    .on_hover_text("Carry on without an answer");
                let skip = lock_tip(skip, locked);
                if rc.tapped(&skip) {
                    action = Some(CardAction::Dismiss);
                }
                if busy {
                    tinted_spinner(ui, SpinnerSize::Sm, rc.p.running);
                }
            });
        },
    );
    let id = draft_id(view, key);
    if drafts.drafts != before || !ui.data(|d| d.get_temp::<CardDraft>(id).is_some()) {
        ui.data_mut(|d| d.insert_temp(id, drafts));
    }
    action
}

fn question_body(
    ui: &mut Ui,
    rc: &mut RowCx<'_>,
    q: &Question<'_>,
    draft: &mut Draft,
    id: Id,
    single_confirm: bool,
) {
    if let Some(header) = &q.header {
        let font = rc.f.small.clone();
        plain(ui, rc, header, &font, rc.p.text_dim);
    }
    let font = rc.f.strong.clone();
    plain(ui, rc, &q.prompt, &font, rc.p.card_text);
    if let Some(why) = &q.why {
        let font = rc.f.small.clone();
        plain(ui, rc, why, &font, rc.p.text_dim);
    }
    match q.form {
        QuestionForm::Choice | QuestionForm::Multi | QuestionForm::Checklist => {
            let single = q.form == QuestionForm::Choice;
            if q.form == QuestionForm::Checklist {
                let ticked = q.items.iter().filter(|i| draft.has(&i.id)).count();
                let font = rc.f.small.clone();
                plain(
                    ui,
                    rc,
                    &format!("{ticked} of {} ticked", q.items.len()),
                    &font,
                    rc.p.text_dim,
                );
            }
            for item in q.items.iter() {
                let row = MarkRow {
                    single,
                    on: draft.has(&item.id),
                    label: &item.label,
                    tag: item.recommended.then_some("recommended"),
                    details: [item.detail.as_deref(), item.reason.as_deref()],
                };
                if row.show(ui, rc, id.with(("item", item.id.as_ref()))) {
                    draft.pick(&item.id, single);
                }
            }
        }
        QuestionForm::Confirm if !single_confirm => {
            for (item_id, label) in confirm_items(q) {
                let row = MarkRow {
                    single: true,
                    on: draft.has(&item_id),
                    label: &label,
                    tag: None,
                    details: [None, None],
                };
                if row.show(ui, rc, id.with(("item", item_id.as_str()))) {
                    draft.pick(&item_id, true);
                }
            }
        }
        QuestionForm::Confirm => {}
        QuestionForm::Text => {
            TextArea::new(&mut draft.text)
                .rows(2)
                .hint("Type an answer")
                .id_salt(id.with("text"))
                .show(ui);
        }
    }
    if q.free_text && q.form != QuestionForm::Text {
        TextField::new(&mut draft.note)
            .width(ui.available_width())
            .hint("Note to the agent (optional)")
            .id_salt(id.with("note"))
            .show(ui);
    }
}

/// A mark, then the label with an optional tag and dim detail lines under it.
struct MarkRow<'r> {
    single: bool,
    on: bool,
    label: &'r str,
    tag: Option<&'r str>,
    details: [Option<&'r str>; 2],
}

impl MarkRow<'_> {
    /// Draws the row; returns whether it was tapped.
    fn show(self, ui: &mut Ui, rc: &mut RowCx<'_>, id: Id) -> bool {
        let backdrop = ui.painter().add(Shape::Noop);
        let row_h = RowCx::row_h(ui, &rc.f.body);
        let inner = ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = rc.m.gap.x / 2.0;
            let (slot, _) = ui.allocate_exact_size(vec2(rc.m.icon_sm + 2.0, row_h), Sense::hover());
            paint_mark(ui, rc, slot, self.single, self.on);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let mut rich = super::rich::Rich::new(markdown::wrap_width(ui));
                let ctx = ui.ctx().clone();
                rich.text(
                    &ctx,
                    self.label,
                    egui::TextFormat::simple(rc.f.body.clone(), rc.p.card_text),
                );
                if let Some(tag) = self.tag {
                    let small = Fonts::bold(ui, &rc.f.small);
                    rich.text(
                        &ctx,
                        "  ",
                        egui::TextFormat::simple(small.clone(), rc.p.accent),
                    );
                    rich.text(&ctx, tag, egui::TextFormat::simple(small, rc.p.success));
                }
                let (galley, _) = rich.layout(&ctx);
                ui.add(egui::Label::new(galley).selectable(false));
                for detail in self.details.into_iter().flatten() {
                    let font = rc.f.small.clone();
                    plain(ui, rc, detail, &font, rc.p.text_dim);
                }
            });
        });
        let rect = inner.response.rect;
        let response = ui
            .interact(rect, id, Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        if response.hovered() && ui.is_enabled() {
            ui.painter().set(
                backdrop,
                Shape::rect_filled(
                    rect.expand(2.0),
                    CornerRadius::same(rc.p.radius_chip),
                    rc.t.hover,
                ),
            );
        }
        ui.is_enabled() && rc.tapped(&response)
    }
}

/// A radio or check mark in `slot`.
fn paint_mark(ui: &Ui, rc: &RowCx<'_>, slot: Rect, single: bool, on: bool) {
    let size = rc.m.icon_sm;
    let ink = if on { rc.p.accent } else { rc.p.text_faint };
    match (single, on) {
        (true, true) => icons::CHECK_CIRCLE.paint(ui, slot, size, ink),
        (true, false) => icons::CIRCLE.paint(ui, slot, size, ink),
        (false, on) => {
            icons::SQUARE.paint(ui, slot, size, ink);
            if on {
                icons::CHECK.paint(ui, slot, size * 0.7, ink);
            }
        }
    }
}

/// A plan card's fold and the `open` value it was last reset to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PlanFold {
    open: bool,
    seed: Option<bool>,
}

impl PlanFold {
    /// The stored fold, reset to `seed` when the app's starting state changed.
    fn resolve(stored: Option<Self>, seed: Option<bool>) -> Self {
        match stored {
            Some(fold) if fold.seed == seed => fold,
            _ => Self {
                open: seed.unwrap_or(true),
                seed,
            },
        }
    }
}

/// An icon button `slot` boxes in from the right end of the header row.
fn header_button(
    ui: &mut Ui,
    rc: &RowCx<'_>,
    slot: usize,
    icon: Icon,
    tip: &str,
) -> egui::Response {
    let side = IconBox::Sm.side(&rc.m);
    let step = side + rc.m.gap.x / 4.0;
    let right_top = ui.min_rect().right_top() - vec2(side + step * slot as f32, 0.0);
    ui.place(
        Rect::from_min_size(right_top, Vec2::splat(side)),
        |ui: &mut Ui| IconButton::new(icon, IconBox::Sm).tip(tip).show(ui),
    )
}

/// Draws a plan card that folds to its current step; returns [`CardAction::Dismiss`] when closed.
pub(crate) fn plan(
    ui: &mut Ui,
    rc: &mut RowCx<'_>,
    card: &PlanCard<'_>,
    view: Id,
) -> Option<CardAction> {
    let key = card.key.as_ref();
    let open_id = view.with((key, "plan_open"));
    let mut fold = PlanFold::resolve(ui.data(|d| d.get_temp::<PlanFold>(open_id)), card.open);
    let title = card.title.as_deref().unwrap_or("Plan");
    let progress = plan_progress(card);
    let buttons = 1 + usize::from(card.dismiss.is_some());
    let side = IconBox::Sm.side(&rc.m);
    let mut action = None;
    let _ = frame(
        ui,
        rc,
        ("plan", key),
        Head {
            icon: icons::LIST,
            tone: Tone::Accent,
            kind: title,
            context: &progress,
            trail: buttons as f32 * (side + rc.m.gap.x / 4.0),
        },
        |ui, rc| {
            let chevron = if fold.open {
                rc.icons.chevron_up
            } else {
                rc.icons.chevron_down
            };
            let toggle = header_button(
                ui,
                rc,
                0,
                chevron,
                if fold.open { "Fold" } else { "Unfold" },
            );
            if rc.tapped(&toggle) {
                fold.open = !fold.open;
            }
            if let Some(tip) = &card.dismiss {
                let close = header_button(ui, rc, 1, rc.icons.close, tip);
                if rc.tapped(&close) {
                    action = Some(CardAction::Dismiss);
                }
            }
            if !fold.open {
                if let Some(step) = card.current() {
                    step_row(ui, rc, step);
                }
                return;
            }
            if let Some(why) = &card.explanation {
                let font = rc.f.small.clone();
                plain(ui, rc, why, &font, rc.p.text_dim);
            }
            for step in card.steps.iter() {
                step_row(ui, rc, step);
            }
        },
    );
    ui.data_mut(|d| d.insert_temp(open_id, fold));
    action
}

fn step_row(ui: &mut Ui, rc: &RowCx<'_>, step: &PlanStep<'_>) {
    let row_h = RowCx::row_h(ui, &rc.f.body);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = rc.m.gap.x / 2.0;
        let (slot, _) = ui.allocate_exact_size(vec2(rc.m.icon_sm + 2.0, row_h), Sense::hover());
        let size = rc.m.icon_sm;
        let (ink, strike) = match step.status {
            PlanStatus::Completed => {
                icons::CHECK_CIRCLE.paint(ui, slot, size, rc.p.success);
                (rc.p.text_dim, true)
            }
            PlanStatus::InProgress => {
                icons::CIRCLE.paint(ui, slot, size, rc.p.accent);
                icons::DOT.paint(ui, slot, size, rc.p.accent);
                (rc.p.card_text, false)
            }
            PlanStatus::Pending => {
                icons::CIRCLE.paint(ui, slot, size, rc.p.text_faint);
                (rc.p.card_text, false)
            }
        };
        let text = super::rich::renderable(ui.ctx(), &rc.f.body, &step.text);
        let mut job = egui::text::LayoutJob::default();
        job.wrap.max_width = ui.available_width().max(48.0);
        job.append(
            &text,
            0.0,
            egui::TextFormat {
                strikethrough: if strike {
                    egui::Stroke::new(1.0, ink)
                } else {
                    egui::Stroke::NONE
                },
                ..egui::TextFormat::simple(rc.f.body.clone(), ink)
            },
        );
        let galley = ui.ctx().fonts_mut(|f| f.layout_job(job));
        ui.add(egui::Label::new(galley).selectable(rc.selectable));
    });
}

/// Draws the card `entry` holds, if it is one, scoped by its key; returns the user's action.
pub(crate) fn entry_card(
    ui: &mut Ui,
    rc: &mut RowCx<'_>,
    entry: &ChatEntry<'_>,
    view: Id,
) -> Option<CardAction> {
    ui.push_id(
        ("egui_theme_kit.chat.card", entry.key()),
        |ui| match entry {
            ChatEntry::Approval(card) => approval(ui, rc, card, view),
            ChatEntry::Question(card) => question(ui, rc, card, view),
            ChatEntry::Plan(card) => plan(ui, rc, card, view),
            _ => None,
        },
    )
    .inner
}

/// Whether `entry` is a card [`floating_cards`] shows.
fn floats(entry: &ChatEntry<'_>) -> bool {
    matches!(
        entry,
        ChatEntry::Approval(_) | ChatEntry::Question(_) | ChatEntry::Plan(_)
    )
}

/// Shows the cards among `cards` one at a time with a pager; returns each acted-on key and action.
pub fn floating_cards(
    ui: &mut Ui,
    id_salt: impl AsIdSalt,
    cards: &[ChatEntry<'_>],
) -> Vec<(String, CardAction)> {
    let max_h = ui.ctx().content_rect().height() * FLOATING_SHARE;
    floating_cards_within(ui, id_salt, cards, max_h)
}

/// [`floating_cards`] scrolling the card past `max_height` points.
pub fn floating_cards_within(
    ui: &mut Ui,
    id_salt: impl AsIdSalt,
    cards: &[ChatEntry<'_>],
    max_height: f32,
) -> Vec<(String, CardAction)> {
    let shown: Vec<&ChatEntry<'_>> = cards.iter().filter(|e| floats(e)).collect();
    let mut out = Vec::new();
    if shown.is_empty() {
        return out;
    }
    let id = ui
        .scope_id()
        .with(("egui_theme_kit.chat.floating", id_salt));
    let page_id = id.with("page");
    let mut page = ui
        .data(|d| d.get_temp::<usize>(page_id))
        .unwrap_or(0)
        .min(shown.len() - 1);
    let mut rc = RowCx::new(ui, id);
    if shown.len() > 1 {
        ui.horizontal(|ui| {
            let prev = IconButton::new(rc.icons.chevron_up, IconBox::Sm)
                .tip("Previous card")
                .enabled(page > 0)
                .show(ui);
            if rc.tapped(&prev) {
                page -= 1;
            }
            let label = format!("{} of {}", page + 1, shown.len());
            let font = rc.f.small.clone();
            plain(ui, &rc, &label, &font, rc.p.text_dim);
            let next = IconButton::new(rc.icons.chevron_down, IconBox::Sm)
                .tip("Next card")
                .enabled(page + 1 < shown.len())
                .show(ui);
            if rc.tapped(&next) {
                page += 1;
            }
        });
    }
    let entry = shown[page];
    ScrollArea::vertical()
        .id_salt(id.with(("scroll", entry.key())))
        .max_height(max_height.max(0.0))
        .auto_shrink([false, true])
        .show(ui, |ui| {
            if let Some(action) = entry_card(ui, &mut rc, entry, id) {
                out.push((entry.key().to_owned(), action));
            }
        });
    ui.data_mut(|d| d.insert_temp(page_id, page));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    fn item(id: &'static str, recommended: bool) -> QuestionItem<'static> {
        QuestionItem {
            id: Cow::Borrowed(id),
            label: Cow::Borrowed(id),
            detail: None,
            recommended,
            reason: None,
        }
    }

    fn question(form: QuestionForm, items: Vec<QuestionItem<'static>>) -> Question<'static> {
        Question {
            id: Cow::Borrowed("q"),
            prompt: Cow::Borrowed("Pick"),
            form,
            items: CowList::Owned(items),
            free_text: false,
            header: None,
            why: None,
        }
    }

    #[test]
    fn a_checklist_starts_on_its_recommendations_and_answers_in_item_order() {
        let q = question(
            QuestionForm::Checklist,
            vec![item("a", false), item("b", true), item("c", true)],
        );
        let mut d = Draft::new(&q);
        assert_eq!(d.picked, vec!["b", "c"]);
        d.pick("a", false);
        d.pick("c", false);
        d.note = "  careful ".into();
        let a = d.answer(&q).expect("checklist answers");
        assert_eq!(a.picked, vec!["a", "b"]);
        assert_eq!(a.note.as_deref(), Some("careful"));
        d.picked.clear();
        assert_eq!(d.answer(&q).map(|a| a.picked.len()), Some(0));
    }

    #[test]
    fn choice_multi_text_and_confirm_need_an_answer() {
        let choice = question(
            QuestionForm::Choice,
            vec![item("x", false), item("y", false)],
        );
        let mut d = Draft::new(&choice);
        assert!(d.answer(&choice).is_none());
        d.pick("x", true);
        d.pick("y", true);
        assert_eq!(
            d.answer(&choice).map(|a| a.picked),
            Some(vec!["y".to_owned()])
        );

        let mut noted = question(QuestionForm::Multi, vec![item("x", false)]);
        noted.free_text = true;
        let mut d = Draft::new(&noted);
        assert!(d.answer(&noted).is_none());
        d.note = "none of these".into();
        assert!(d.answer(&noted).is_some_and(|a| a.picked.is_empty()));

        let text = question(QuestionForm::Text, Vec::new());
        let mut d = Draft::new(&text);
        assert!(d.answer(&text).is_none());
        d.text = " hello ".into();
        let a = d.answer(&text).expect("text answers");
        assert!(a.picked.is_empty());
        assert_eq!(a.note.as_deref(), Some("hello"));

        let confirm = question(QuestionForm::Confirm, Vec::new());
        let mut d = Draft::new(&confirm);
        assert!(d.answer(&confirm).is_none());
        d.pick(CONFIRM_NO, true);
        assert_eq!(
            d.answer(&confirm).map(|a| a.picked),
            Some(vec![CONFIRM_NO.to_owned()])
        );
    }

    #[test]
    fn shortcuts_parse_with_modifiers() {
        assert_eq!(
            parse_shortcut("Y"),
            Some(KeyboardShortcut::new(Modifiers::NONE, Key::Y))
        );
        assert_eq!(
            parse_shortcut("n"),
            Some(KeyboardShortcut::new(Modifiers::NONE, Key::N))
        );
        assert_eq!(
            parse_shortcut("Ctrl+Enter"),
            Some(KeyboardShortcut::new(Modifiers::COMMAND, Key::Enter))
        );
        assert_eq!(parse_shortcut("Y+N"), None);
        assert_eq!(parse_shortcut("Hyper"), None);
    }

    #[test]
    fn a_shortcut_chooses_only_on_the_card_under_the_pointer() {
        let choices = [
            CardChoice {
                label: Cow::Borrowed("Approve"),
                kind: ButtonKind::Primary,
                shortcut: Some(Cow::Borrowed("Y")),
                while_locked: false,
            },
            CardChoice {
                label: Cow::Borrowed("Deny"),
                kind: ButtonKind::Danger,
                shortcut: Some(Cow::Borrowed("N")),
                while_locked: false,
            },
        ];
        let card = ApprovalCard {
            key: Cow::Borrowed("ap"),
            context: Cow::Borrowed("desk"),
            title: Cow::Borrowed("Run a command"),
            command: None,
            copy_text: None,
            command_max_lines: None,
            detail: None,
            reason: None,
            choices: CowList::Borrowed(&choices),
            state: CardState::Open,
        };
        let ctx = egui::Context::default();
        let rect = std::cell::Cell::new(Rect::NOTHING);
        let run = |events: Vec<egui::Event>| {
            let raw = egui::RawInput {
                events,
                ..Default::default()
            };
            let mut action = None;
            let mut out = ctx.run_ui(raw, |ui| {
                let mut rc = RowCx::new(ui, Id::unique("cards"));
                let inner = ui.scope(|ui| approval(ui, &mut rc, &card, Id::unique("view")));
                rect.set(inner.response.rect);
                action = inner.inner;
            });
            out.textures_delta.clear();
            action
        };
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        run(Vec::new());
        assert_eq!(run(vec![key(Key::N)]), None);
        let inside = rect.get().center();
        run(vec![egui::Event::PointerMoved(inside)]);
        assert_eq!(run(vec![key(Key::N)]), Some(CardAction::Choose(1)));
    }

    #[test]
    fn drafts_are_kept_per_view() {
        let q = question(
            QuestionForm::Choice,
            vec![item("x", false), item("y", false)],
        );
        let card = QuestionCard {
            key: Cow::Borrowed("0"),
            context: Cow::Borrowed(""),
            header: None,
            why: None,
            questions: CowList::Owned(vec![q.clone()]),
            state: CardState::Open,
        };
        let ctx = egui::Context::default();
        let (a, b) = (Id::unique("session-a"), Id::unique("session-b"));
        let mut out = ctx.run_ui(Default::default(), |ui| {
            let mut drafts = load_drafts(ui, a, &card);
            drafts.drafts[0].pick("x", true);
            ui.data_mut(|d| d.insert_temp(draft_id(a, "0"), drafts));
            assert_eq!(load_drafts(ui, a, &card).drafts[0].picked, vec!["x"]);
            assert!(load_drafts(ui, b, &card).drafts[0].picked.is_empty());
        });
        out.textures_delta.clear();
    }

    #[test]
    fn a_cut_command_keeps_whole_lines_and_says_how_many_were_left_out() {
        assert_eq!(cut_command("a\nb", 2), None);
        assert_eq!(
            cut_command("a\nb\nc\nd", 3).as_deref(),
            Some("a\nb\n… 2 more lines")
        );
        assert_eq!(cut_command("a\nb", 1).as_deref(), Some("a\n… 1 more line"));
    }

    #[test]
    fn plans_count_progress_and_find_the_current_step() {
        let step = |text, status| PlanStep {
            text: Cow::Borrowed(text),
            status,
        };
        let plan = PlanCard {
            key: Cow::Borrowed("p"),
            title: None,
            explanation: None,
            steps: CowList::Owned(vec![
                step("scan", PlanStatus::Completed),
                step("fix", PlanStatus::InProgress),
                step("verify", PlanStatus::Pending),
            ]),
            ..PlanCard::default()
        };
        assert_eq!(plan_progress(&plan), "1 of 3 done");
        assert_eq!(plan.current().map(|s| s.text.as_ref()), Some("fix"));
    }
}
