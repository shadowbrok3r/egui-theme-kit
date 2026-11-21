//! The chat transcript: user bubbles, agent markdown, notices, folded work groups and cards.

mod board;
mod bubble;
mod cards;
mod entry;
mod group;
#[cfg(feature = "chat-json")]
mod json;
pub mod markdown;
mod payload;
mod rich;
mod row;
mod shell;
mod views;
mod work;

use std::collections::HashMap;
use std::hash::{BuildHasher, BuildHasherDefault, DefaultHasher, Hash};

use egui::{
    Align, AsIdSalt, Id, IdSalt, Layout, Rect, ScrollArea, Shape, Ui, UiBuilder,
    containers::scroll_area::State as ScrollState, vec2,
};

pub use board::{BoardAction, BoardGroup, WaitingBoard};
pub use cards::{
    ApprovalCard, CONFIRM_NO, CONFIRM_YES, CardChoice, CardState, PlanCard, PlanStatus, PlanStep,
    Question, QuestionCard, QuestionForm, QuestionItem, floating_cards, plan_progress,
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
pub use shell::is_command_key;
pub use views::ChatText;

use super::mention::MentionSource;
use super::types::{ChatAction, CowStr, MessageNav, TimeFormat, UnixMs};
use row::RowCx;

/// Seconds a jumped-to row stays highlighted.
const FLASH_SECS: f64 = 1.2;
/// Scroll speed above which a press stops a fling instead of tapping, in points per second.
const FLING_SPEED: f32 = 40.0;
/// Share of the viewport drawn above and below it.
const OVERSCAN: f32 = 0.5;
/// Share of the viewport height above which a user message counts as current.
const NAV_LINE: f32 = 0.33;

/// Context slot holding the tap suppression of the transcript being drawn.
fn suppress_id() -> Id {
    Id::unique("egui_theme_kit.chat.transcript.tap_suppressed")
}

/// Draws app-defined lines, such as media tags, in place of their text.
///
/// Lines are offered from agent messages and, once a work item is opened, from its non-JSON text
/// result and its body; file-change diffs and JSON trees are drawn as they are.
pub trait AgentLines {
    /// Whether the app draws `line`; lines inside code fences are never offered.
    fn claims(&self, line: &str) -> bool;

    /// Draws a claimed `line` of the agent message or work item keyed `key`.
    fn show(&self, ui: &mut Ui, key: &str, line: &str);
}

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

    /// Changes whenever entry `i` changes; `None` keeps its last measured height until it is drawn again.
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

impl TranscriptSource for Vec<ChatEntry<'_>> {
    fn len(&self) -> usize {
        self.as_slice().len()
    }

    fn kind(&self, i: usize) -> EntryKind {
        self.as_slice().kind(i)
    }

    fn key(&self, i: usize) -> CowStr<'_> {
        self.as_slice().key(i)
    }

    fn entry(&self, i: usize) -> ChatEntry<'_> {
        self.as_slice().entry(i)
    }

    fn revision(&self, i: usize) -> Option<u64> {
        self.as_slice().revision(i)
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
    /// Shows the author under every user bubble, not only on hover.
    pub show_user_author: bool,
}

impl Default for TranscriptOptions {
    fn default() -> Self {
        Self {
            detail: DetailLevel::Simple,
            show_thinking: true,
            max_width: None,
            now: None,
            show_user_author: false,
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

/// A measured row height and the inputs it was measured with.
#[derive(Clone, Copy, Debug)]
struct Height {
    sig: u64,
    h: f32,
    width: f32,
}

impl Height {
    /// The height for inputs `sig` at `width`: exact when they match, else rescaled for the width change.
    fn guess(&self, sig: u64, width: f32) -> f32 {
        if sig == self.sig || width <= 0.0 || self.width <= 0.0 {
            self.h
        } else {
            self.h * (self.width / width).clamp(0.5, 2.0)
        }
    }
}

/// Scroll, fold and height state the app keeps per transcript.
#[derive(Clone, Debug)]
pub struct TranscriptState {
    following: bool,
    pending: Option<Jump>,
    stick_frames: u32,
    group_open: HashMap<String, bool>,
    heights: HashMap<u64, Height>,
    /// Row key hash and how far the viewport top is into that row.
    anchor: Option<(u64, f32)>,
    scroll_id: Option<Id>,
    view_h: f32,
    suppress: bool,
    flash: Option<(u64, f64)>,
}

impl Default for TranscriptState {
    fn default() -> Self {
        Self {
            following: true,
            pending: None,
            stick_frames: 0,
            group_open: HashMap::new(),
            heights: HashMap::new(),
            anchor: None,
            scroll_id: None,
            view_h: 0.0,
            suppress: false,
            flash: None,
        }
    }
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

    /// The user's open choice for the work group keyed `key`, if any.
    pub fn group_open(&self, key: &str) -> Option<bool> {
        self.group_open.get(key).copied()
    }

    /// Opens or closes the work group keyed `key`; `None` returns it to automatic.
    pub fn set_group_open(&mut self, key: &str, open: Option<bool>) {
        match open {
            Some(open) => self.group_open.insert(key.to_owned(), open),
            None => self.group_open.remove(key),
        };
    }

    /// Forgets measured heights, such as after the app swaps the whole history.
    pub fn clear_heights(&mut self) {
        self.heights.clear();
    }

    /// Whether the transcript ignored taps in its last frame because the press stopped a fling.
    ///
    /// Inside [`Transcript::show`] callbacks, read [`Transcript::tap_suppressed`] instead.
    pub fn tap_suppressed(&self) -> bool {
        self.suppress
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
    mention_stamp: Option<u64>,
    time_format: Option<&'a dyn TimeFormat>,
    older: Option<LoadState>,
    agent_lines: Option<&'a dyn AgentLines>,
}

fn key_hash(key: &str) -> u64 {
    BuildHasherDefault::<DefaultHasher>::default().hash_one(key)
}

fn hash_of(value: impl Hash) -> u64 {
    BuildHasherDefault::<DefaultHasher>::default().hash_one(value)
}

/// Index of the first entry of `run`.
fn first(run: &Run) -> usize {
    match run {
        Run::Single(i) => *i,
        Run::Work(r) => r.start,
    }
}

/// Combined revision of the entries of `run`; `None` when any entry has none.
fn run_revision<S: TranscriptSource + ?Sized>(src: &S, run: &Run) -> Option<u64> {
    match run {
        Run::Single(i) => src.revision(*i),
        Run::Work(r) => r
            .clone()
            .map(|i| src.revision(i))
            .collect::<Option<Vec<u64>>>()
            .map(hash_of),
    }
}

/// Height guess for a row never measured.
fn estimate(kind: EntryKind, rc: &RowCx<'_>) -> f32 {
    let line = rc.f.body.size * 1.4;
    let gap = rc.m.gap.y * 1.5;
    gap + match kind {
        EntryKind::User => line * 2.0 + rc.m.control_h,
        EntryKind::Agent => line * 3.0,
        EntryKind::Notice | EntryKind::Divider => line,
        EntryKind::Work(_) => rc.m.row_h,
        EntryKind::Approval | EntryKind::Question | EntryKind::Plan => line * 7.0,
        EntryKind::Custom => line * 2.0,
    }
}

/// Space above a row, by its kind and the kind before it.
fn row_gap(kind: EntryKind, prev: Option<EntryKind>, rc: &RowCx<'_>) -> f32 {
    let g = rc.m.gap.y;
    match (prev, kind) {
        (None, _) => g,
        (Some(EntryKind::Work(_)), EntryKind::Work(_)) => g / 2.0,
        (Some(p), k) if p == k => g,
        (_, EntryKind::User) | (Some(EntryKind::User), _) => g * 2.5,
        _ => g * 1.5,
    }
}

/// One folded row ready to lay out.
struct Row {
    run: Run,
    kind: EntryKind,
    hash: u64,
    /// Key hashes of the entries after the first in a work group.
    members: Vec<u64>,
    sig: u64,
    h: f32,
}

/// The row keyed `hash`, else the work group holding an entry keyed `hash`.
fn find_row(rows: &[Row], hash: u64) -> Option<usize> {
    rows.iter()
        .position(|r| r.hash == hash)
        .or_else(|| rows.iter().position(|r| r.members.contains(&hash)))
}

/// The open choice stored under the first key of the group over `range` that has one.
fn group_choice<S: TranscriptSource + ?Sized>(
    state: &TranscriptState,
    src: &S,
    range: std::ops::Range<usize>,
) -> Option<bool> {
    if state.group_open.is_empty() {
        return None;
    }
    range
        .into_iter()
        .find_map(|i| state.group_open.get(src.key(i).as_ref()).copied())
}

impl<'a> Transcript<'a> {
    pub fn new(id_salt: impl AsIdSalt) -> Self {
        Self {
            id_salt: IdSalt::new(id_salt),
            options: TranscriptOptions::default(),
            mentions: None,
            mention_stamp: None,
            time_format: None,
            older: None,
            agent_lines: None,
        }
    }

    /// Lets the app draw lines it claims, such as media tags, in agent messages and opened work items.
    pub fn agent_lines(mut self, lines: &'a dyn AgentLines) -> Self {
        self.agent_lines = Some(lines);
        self
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

    /// Caches resolved mention labels until `revision` changes; without it they resolve every frame.
    pub fn mention_revision(mut self, revision: u64) -> Self {
        self.mention_stamp = Some(revision);
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

    /// Whether the transcript being drawn ignores taps this frame because the press stopped a fling.
    ///
    /// Valid inside the `custom` callback of [`Self::show`] and in [`AgentLines::show`]; false
    /// outside a transcript. App rows count a tap as `response.clicked() && !Transcript::tap_suppressed(ui.ctx())`.
    pub fn tap_suppressed(ctx: &egui::Context) -> bool {
        ctx.data(|d| d.get_temp::<bool>(suppress_id()))
            .unwrap_or(false)
    }

    /// Draws the entries of `src`; `custom` draws [`ChatEntry::Custom`] rows by key.
    ///
    /// Taps in `custom` rows should be ignored while [`Self::tap_suppressed`] is true.
    pub fn show<S: TranscriptSource + ?Sized>(
        self,
        ui: &mut Ui,
        state: &mut TranscriptState,
        src: &S,
        mut custom: impl FnMut(&mut Ui, &str),
    ) -> TranscriptResponse {
        let id = ui
            .scope_id()
            .with(("egui_theme_kit.chat.transcript", self.id_salt));
        let mut rc = RowCx::new(ui, id);
        rc.options = self.options;
        rc.mentions = self.mentions;
        rc.mention_stamp = self.mention_stamp;
        rc.agent_lines = self.agent_lines;
        if let Some(format) = self.time_format {
            rc.time_format = format;
        }
        let now = ui.input(|i| i.time);

        let scroll = state
            .scroll_id
            .and_then(|sid| ScrollState::load(ui.ctx(), sid));
        let current = scroll.as_ref().map_or(0.0, |s| s.unclamped_offset().y);
        let moving = scroll
            .as_ref()
            .is_some_and(|s| s.velocity().length() > FLING_SPEED);
        let (pressed, down, released) = ui.input(|i| {
            (
                i.pointer.any_pressed(),
                i.pointer.any_down(),
                i.pointer.any_released(),
            )
        });
        state.suppress = suppress_taps(state.suppress, pressed, down, released, moving);
        rc.suppress = state.suppress;
        let outer_suppress = ui.data_mut(|d| {
            let outer = d.get_temp::<bool>(suppress_id());
            d.insert_temp(suppress_id(), state.suppress);
            outer
        });

        let width = state_width(ui, self.options.max_width);
        let em = rc.f.body.size;
        let kinds: Vec<EntryKind> = (0..src.len()).map(|i| src.kind(i)).collect();
        let mut rows: Vec<Row> = runs(kinds.iter().copied())
            .into_iter()
            .map(|run| {
                let kind = kinds[first(&run)];
                let key = src.key(first(&run));
                let (chosen, members) = match &run {
                    Run::Work(range) => (
                        group_choice(state, src, range.clone()),
                        range
                            .clone()
                            .skip(1)
                            .map(|i| key_hash(&src.key(i)))
                            .collect(),
                    ),
                    Run::Single(_) => (None, Vec::new()),
                };
                let sig = hash_of((
                    run_revision(src, &run),
                    width.round() as i32,
                    self.options.detail,
                    self.options.show_thinking,
                    chosen,
                    em.to_bits(),
                ));
                let hash = key_hash(&key);
                let h = state
                    .heights
                    .get(&hash)
                    .map_or_else(|| estimate(kind, &rc), |c| c.guess(sig, width));
                Row {
                    run,
                    kind,
                    hash,
                    members,
                    sig,
                    h,
                }
            })
            .collect();

        let header_h = self
            .older
            .map_or(0.0, |_| rc.m.control_h + rc.m.gap.y * 2.0);
        let bottom_pad = rc.m.gap.y * 2.0;
        let tops = |rows: &[Row]| -> Vec<f32> {
            let mut y = header_h;
            rows.iter()
                .map(|r| {
                    let top = y;
                    y += r.h;
                    top
                })
                .collect()
        };
        let mut row_tops = tops(&rows);
        let total = |rows: &[Row], tops: &[f32]| {
            tops.last()
                .zip(rows.last())
                .map_or(header_h, |(t, r)| t + r.h)
                + bottom_pad
        };

        if let Some(jump) = state.pending.take() {
            resolve_jump(ui, state, src, &rows, &row_tops, &jump, current, now);
        }
        if state.stick_frames > 0 {
            state.following = true;
        }

        let view_h = state.view_h;
        let content_h = total(&rows, &row_tops);
        let desired = if state.following {
            Some((content_h - view_h).max(0.0))
        } else {
            state
                .anchor
                .and_then(|(hash, off)| find_row(&rows, hash).map(|i| row_tops[i] + off))
        };
        let mut area = ScrollArea::vertical()
            .id_salt(id.with("scroll"))
            .auto_shrink([false, false]);
        let start = match desired {
            Some(d) if (d - current).abs() > 0.5 => {
                area = area.vertical_scroll_offset(d);
                d
            }
            _ => current,
        };
        let kinetic = ui.spacing().scroll.kinetic;
        if start != current {
            ui.spacing_mut().scroll.kinetic.rubber_band = false;
        }

        let mut measured: Vec<(usize, f32)> = Vec::new();
        let mut card_actions: Vec<ChatAction> = Vec::new();
        let flash = state.flash.filter(|(_, t)| now - t < FLASH_SECS);
        let older = self.older;
        let output = area.show_viewport(ui, |ui, viewport| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let full = ui.available_width().min(ui.clip_rect().width());
            let col_w = self.options.max_width.map_or(full, |m| m.min(full));
            let left = ui.max_rect().left() + ((full - col_w) / 2.0).max(0.0);
            let col = Rect::from_min_size(
                egui::pos2(left, ui.cursor().top()),
                vec2(col_w, f32::INFINITY),
            );
            let lo = viewport.min.y - viewport.height() * OVERSCAN;
            let hi = viewport.max.y + viewport.height() * OVERSCAN;
            let first_row = rows
                .iter()
                .zip(&row_tops)
                .position(|(r, t)| t + r.h >= lo)
                .unwrap_or(rows.len());
            let last_row = rows
                .iter()
                .zip(&row_tops)
                .rposition(|(_, t)| *t <= hi)
                .map_or(first_row, |i| i + 1)
                .max(first_row);
            ui.scope_builder(
                UiBuilder::new()
                    .max_rect(col)
                    .layout(Layout::top_down(Align::Min)),
                |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let origin = ui.cursor().top();
                    let column = ui.max_rect().x_range();
                    let row_rect = |top: f32| Rect::from_x_y_ranges(column, top..=f32::INFINITY);
                    if let Some(load) = older {
                        if lo < header_h {
                            ui.allocate_ui_with_layout(
                                vec2(ui.available_width(), header_h),
                                Layout::top_down(Align::Center),
                                |ui| {
                                    ui.add_space(rc.m.gap.y);
                                    bubble::load_older(ui, &mut rc, load);
                                },
                            );
                        }
                        let at = ui.cursor().top() - origin;
                        ui.add_space((header_h - at).max(0.0));
                    }
                    let pivot = state
                        .anchor
                        .filter(|_| !state.following)
                        .and_then(|(hash, _)| find_row(&rows, hash))
                        .filter(|p| (first_row..last_row).contains(p))
                        .unwrap_or(first_row);
                    let mut pass = RowPass {
                        rc: &mut rc,
                        state,
                        src,
                        custom: &mut custom,
                        card_actions: &mut card_actions,
                        flash,
                        now,
                    };
                    for r in first_row..pivot {
                        let row = &rows[r];
                        let slot = ui.painter().add(Shape::Noop);
                        let mut child = ui.new_child(
                            UiBuilder::new()
                                .id_salt(("row", row.hash))
                                .max_rect(row_rect(origin + row_tops[r]))
                                .layout(Layout::top_down(Align::Min)),
                        );
                        pass.contents(&mut child, &rows, r);
                        let drawn = child.min_rect();
                        pass.flash_row(ui, slot, row.hash, drawn);
                        if (drawn.height() - row.h).abs() > 0.5 {
                            ui.ctx()
                                .request_discard("chat transcript rows above the anchor resized");
                        }
                        measured.push((r, drawn.height()));
                    }
                    let above = row_tops.get(pivot).copied().unwrap_or(header_h);
                    let at = ui.cursor().top() - origin;
                    ui.add_space((above - at).max(0.0));
                    for r in pivot..last_row {
                        let row = &rows[r];
                        let top = ui.cursor().top();
                        let slot = ui.painter().add(Shape::Noop);
                        let mut child = ui.new_child(
                            UiBuilder::new()
                                .id_salt(("row", row.hash))
                                .max_rect(row_rect(top))
                                .layout(Layout::top_down(Align::Min)),
                        );
                        pass.contents(&mut child, &rows, r);
                        let h = child.min_rect().height();
                        ui.add_space(h);
                        let drawn = Rect::from_x_y_ranges(column, top..=top + h);
                        pass.flash_row(ui, slot, row.hash, drawn);
                        measured.push((r, h));
                    }
                    let rest: f32 = rows[last_row..].iter().map(|r| r.h).sum();
                    ui.add_space(rest + bottom_pad);
                },
            );
        });
        ui.spacing_mut().scroll.kinetic = kinetic;

        for (r, h) in measured {
            rows[r].h = h;
            state.heights.insert(
                rows[r].hash,
                Height {
                    sig: rows[r].sig,
                    h,
                    width,
                },
            );
        }
        row_tops = tops(&rows);
        let content_h = total(&rows, &row_tops);
        let view_h = output.inner_rect.height();
        let offset = output.state.unclamped_offset().y;
        let max_offset = (content_h - view_h).max(0.0);
        state.scroll_id = Some(output.id);
        state.view_h = view_h;

        let user_delta = offset - start;
        if state.stick_frames > 0 {
            state.stick_frames -= 1;
            state.following = true;
        } else if state.following && user_delta < -0.5 && offset < max_offset - 1.0 {
            state.following = false;
        } else if !state.following && offset >= max_offset - 1.0 {
            state.following = true;
        }

        let anchored = state
            .anchor
            .and_then(|(hash, off)| find_row(&rows, hash).map(|i| (i, off + user_delta)));
        state.anchor = match anchored {
            Some((i, off)) if (0.0..rows[i].h).contains(&off) => Some((rows[i].hash, off)),
            Some((i, off)) => row_at(&rows, &row_tops, row_tops[i] + off),
            None => row_at(&rows, &row_tops, offset),
        };

        let users: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.kind == EntryKind::User)
            .map(|(i, _)| i)
            .collect();
        let at_bottom = offset >= max_offset - 1.0;
        let line = if at_bottom {
            offset + view_h
        } else {
            offset + view_h * NAV_LINE
        };
        let current_user = users.iter().rposition(|&i| row_tops[i] < line).or_else(|| {
            users
                .first()
                .filter(|&&i| row_tops[i] < offset + view_h)
                .map(|_| 0)
        });

        ui.data_mut(|d| match outer_suppress {
            Some(outer) => {
                d.insert_temp(suppress_id(), outer);
            }
            None => d.remove::<bool>(suppress_id()),
        });
        let mut actions = rc.actions;
        actions.append(&mut card_actions);
        TranscriptResponse {
            actions,
            nav: MessageNav::new(current_user, users.len()),
            at_bottom,
        }
    }
}

/// Whether taps stay ignored: from a press that stopped a fling until the pointer is released.
fn suppress_taps(was: bool, pressed: bool, down: bool, released: bool, moving: bool) -> bool {
    (was && (down || released)) || (pressed && moving)
}

/// Width rows lay out at, for the height signature.
fn state_width(ui: &Ui, max_width: Option<f32>) -> f32 {
    let full = ui.available_width().min(ui.clip_rect().width());
    max_width.map_or(full, |m| m.min(full))
}

/// The anchor for content position `y`: the row holding it and how far into it `y` is.
fn row_at(rows: &[Row], tops: &[f32], y: f32) -> Option<(u64, f32)> {
    let i = tops.iter().rposition(|t| *t <= y).unwrap_or(0);
    rows.get(i).map(|r| (r.hash, y - tops[i]))
}

/// Points `state` at the row `jump` targets and opens the work group holding a keyed entry.
#[expect(clippy::too_many_arguments)]
fn resolve_jump<S: TranscriptSource + ?Sized>(
    ui: &Ui,
    state: &mut TranscriptState,
    src: &S,
    rows: &[Row],
    tops: &[f32],
    jump: &Jump,
    current: f32,
    now: f64,
) {
    let margin = crate::controls::Metrics::get(ui.ctx()).gap.y;
    let edge = current + margin;
    let users = || {
        rows.iter()
            .enumerate()
            .filter(|(_, r)| r.kind == EntryKind::User)
            .map(|(i, _)| i)
    };
    let target = match jump {
        Jump::Bottom => {
            state.following = true;
            state.anchor = None;
            return;
        }
        Jump::Key(key) => {
            let Some(entry) = (0..src.len()).find(|&i| src.key(i) == key.as_str()) else {
                return;
            };
            let Some(row) = rows.iter().position(|r| match &r.run {
                Run::Single(i) => *i == entry,
                Run::Work(range) => range.contains(&entry),
            }) else {
                return;
            };
            if let Run::Work(range) = &rows[row].run {
                state
                    .group_open
                    .insert(src.key(range.start).into_owned(), true);
            }
            Some(row)
        }
        Jump::PrevUser => users().rev().find(|&i| tops[i] < edge - 1.0),
        Jump::NextUser => users().find(|&i| tops[i] > edge + 1.0),
        Jump::User(n) => users().nth(*n),
    };
    let Some(row) = target else {
        return;
    };
    state.following = false;
    state.stick_frames = 0;
    state.anchor = Some((rows[row].hash, -margin));
    state.flash = Some((rows[row].hash, now));
}

/// What drawing a row needs, borrowed for one frame.
struct RowPass<'p, 'a, S: ?Sized, F> {
    rc: &'p mut RowCx<'a>,
    state: &'p mut TranscriptState,
    src: &'p S,
    custom: &'p mut F,
    card_actions: &'p mut Vec<ChatAction>,
    flash: Option<(u64, f64)>,
    now: f64,
}

impl<S: TranscriptSource + ?Sized, F: FnMut(&mut Ui, &str)> RowPass<'_, '_, S, F> {
    /// Draws the gap and contents of row `r` into its own `ui`.
    fn contents(&mut self, ui: &mut Ui, rows: &[Row], r: usize) {
        let row = &rows[r];
        let prev = r.checked_sub(1).map(|p| rows[p].kind);
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.add_space(row_gap(row.kind, prev, self.rc));
        ui.spacing_mut().item_spacing.y = self.rc.m.gap.y / 2.0;
        draw_row(
            ui,
            self.rc,
            self.state,
            self.src,
            row,
            prev,
            self.custom,
            self.card_actions,
        );
    }

    /// Fills `slot` behind the row keyed `hash` while it is flashing.
    fn flash_row(&self, ui: &Ui, slot: egui::layers::ShapeIdx, hash: u64, rect: Rect) {
        let Some((flashed, start)) = self.flash else {
            return;
        };
        if flashed != hash {
            return;
        }
        let fade = (1.0 - (self.now - start) / FLASH_SECS).clamp(0.0, 1.0) as f32;
        ui.painter().set(
            slot,
            Shape::rect_filled(
                rect.expand2(vec2(self.rc.m.gap.x / 2.0, 0.0)),
                self.rc.p.radius_chip,
                self.rc.p.accent_soft.gamma_multiply(fade),
            ),
        );
        ui.ctx().request_repaint();
    }
}

/// Draws one folded row.
#[expect(clippy::too_many_arguments)]
fn draw_row<S: TranscriptSource + ?Sized>(
    ui: &mut Ui,
    rc: &mut RowCx<'_>,
    state: &mut TranscriptState,
    src: &S,
    row: &Row,
    prev: Option<EntryKind>,
    custom: &mut impl FnMut(&mut Ui, &str),
    card_actions: &mut Vec<ChatAction>,
) {
    match &row.run {
        Run::Single(i) => {
            let entry = src.entry(*i);
            match &entry {
                ChatEntry::User(m) => bubble::user(ui, rc, m),
                ChatEntry::Agent(m) => bubble::agent(ui, rc, m, prev != Some(EntryKind::Agent)),
                ChatEntry::Notice(n) => bubble::notice(ui, rc, n),
                ChatEntry::Divider { caption, .. } => bubble::divider(ui, rc, caption),
                ChatEntry::Work(item) => {
                    let key = item.key.to_string();
                    let chosen = state.group_open.get(&key).copied();
                    if let Some(open) =
                        work::group(ui, rc, &key, std::slice::from_ref(item), chosen)
                    {
                        state.group_open.insert(key, open);
                    }
                }
                ChatEntry::Approval(_) | ChatEntry::Question(_) | ChatEntry::Plan(_) => {
                    let view = rc.id;
                    if let Some(action) = cards::entry_card(ui, rc, &entry, view) {
                        card_actions.push(ChatAction::Card {
                            key: entry.key().to_owned(),
                            action,
                        });
                    }
                }
                ChatEntry::Custom { key } => custom(ui, key),
            }
        }
        Run::Work(range) => {
            let items: Vec<WorkItem<'_>> = range
                .clone()
                .filter_map(|i| match src.entry(i) {
                    ChatEntry::Work(item) => Some(item),
                    _ => None,
                })
                .collect();
            let key = src.key(range.start).into_owned();
            let chosen = group_choice(state, src, range.clone());
            if let Some(open) = work::group(ui, rc, &key, &items, chosen) {
                state.group_open.insert(key, open);
            }
        }
    }
}

#[cfg(test)]
mod tests;
