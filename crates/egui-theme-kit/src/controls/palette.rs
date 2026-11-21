//! A modal command palette: fuzzy search, sections, keyboard navigation and shortcut hints.

use std::sync::Arc;

use egui::{
    Align, Align2, AsIdSalt, Context, EventFilter, FontId, Frame, Galley, Id, Key,
    KeyboardShortcut, Layout, Modal, Modifiers, Rect, Response, Role, ScrollArea, Sense, TextEdit,
    TextStyle, Ui, UiBuilder, Vec2, WidgetInfo,
    class::HasClasses as _,
    pos2,
    text::{LayoutJob, TextFormat},
    vec2,
};

use super::{
    Icon, Kbd, Metrics, Tokens, elided, hairline, icons, kit, medium_font_sized,
    semibold_font_sized, text_galley,
};
use crate::widget_theme;

/// The usual palette shortcut: Ctrl+K, or Cmd+K on macOS.
pub const PALETTE_SHORTCUT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::K);
/// Widest the palette gets.
pub const PALETTE_W: f32 = 560.0;
/// Tallest the result list gets.
pub const PALETTE_LIST_H: f32 = 360.0;
/// Smallest gap between the palette and the screen edges.
const SCREEN_MARGIN: f32 = 12.0;
/// Distance of the palette from the top of the screen, as a share of the screen height.
const TOP_SHARE: f32 = 0.12;
/// Largest distance of the palette from the top of the screen.
const TOP_MAX: f32 = 120.0;
/// Inner margin of the palette frame.
const MARGIN: i8 = 6;
/// Query text size above the body size.
const QUERY_EXTRA_PT: f32 = 2.0;
/// Query row height above a prominent button.
const QUERY_EXTRA_H: f32 = 8.0;
/// Result row height above a list row.
const ROW_EXTRA_H: f32 = 8.0;
/// Horizontal padding inside a result row.
const ROW_PAD_X: f32 = 8.0;
/// Section label size.
const SECTION_PT: f32 = 11.0;
/// Rows a page key moves the selection.
const PAGE_ROWS: usize = 8;
/// Narrowest palette that shows the key hint footer.
const FOOTER_MIN_W: f32 = 360.0;
/// Share of the row a title takes before its detail when both are present.
const TITLE_SHARE: f32 = 0.62;
/// Points for each matched character.
const SCORE_MATCH: i32 = 16;
/// Bonus for a match right after the previous one.
const BONUS_CONSECUTIVE: i32 = 12;
/// Bonus for a match at the start of a word.
const BONUS_WORD_START: i32 = 10;
/// Bonus for a match on the first character.
const BONUS_PREFIX: i32 = 12;
/// Bonus for a query equal to the whole text.
const BONUS_EXACT: i32 = 40;
/// Penalty for opening a gap between matches, and for each skipped character after.
const PENALTY_GAP_START: i32 = 3;
const PENALTY_GAP: i32 = 1;
/// Largest penalty for unmatched characters before the first match.
const PENALTY_LEAD_MAX: i32 = 8;
/// Largest penalty for unmatched characters overall.
const PENALTY_LENGTH_MAX: i32 = 5;
/// Penalty for matching a keyword or the detail instead of the title.
const PENALTY_KEYWORD: i32 = 12;
const PENALTY_DETAIL: i32 = 24;

/// Where a query matched a text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FuzzyMatch {
    /// Higher is better.
    pub score: i32,
    /// Char indices of the matched characters.
    pub positions: Vec<usize>,
}

/// Whether the char at `i` starts a word: the first char, after a non-alphanumeric, or a case or digit change.
fn word_start(text: &[char], i: usize) -> bool {
    let Some(prev) = i.checked_sub(1).map(|p| text[p]) else {
        return true;
    };
    let c = text[i];
    !prev.is_alphanumeric()
        || (prev.is_lowercase() && c.is_uppercase())
        || (!prev.is_ascii_digit() && c.is_ascii_digit())
}

/// Case-insensitive subsequence match of `query` (whitespace ignored) in `text`, scored.
pub fn fuzzy_match(query: &str, text: &str) -> Option<FuzzyMatch> {
    let needle: Vec<char> = query
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    if needle.is_empty() {
        return Some(FuzzyMatch::default());
    }
    let hay: Vec<char> = text.chars().collect();
    let lower: Vec<char> = hay
        .iter()
        .map(|c| c.to_lowercase().next().unwrap_or(*c))
        .collect();
    let mut q = 0;
    let mut end = None;
    for (i, c) in lower.iter().enumerate() {
        if *c == needle[q] {
            q += 1;
            if q == needle.len() {
                end = Some(i);
                break;
            }
        }
    }
    let end = end?;
    let mut positions = vec![0; needle.len()];
    let mut q = needle.len();
    let mut i = end + 1;
    while q > 0 {
        i -= 1;
        if lower[i] == needle[q - 1] {
            q -= 1;
            positions[q] = i;
        }
    }
    let mut score = 0;
    let mut prev: Option<usize> = None;
    for &p in &positions {
        score += SCORE_MATCH;
        if word_start(&hay, p) {
            score += BONUS_WORD_START;
        }
        match prev {
            Some(last) if p == last + 1 => score += BONUS_CONSECUTIVE,
            Some(last) => {
                let skipped = (p - last - 1) as i32;
                score -= PENALTY_GAP_START + PENALTY_GAP * (skipped - 1).min(10);
            }
            None if p == 0 => score += BONUS_PREFIX,
            None => score -= (p as i32).min(PENALTY_LEAD_MAX),
        }
        prev = Some(p);
    }
    if lower.len() == needle.len() {
        score += BONUS_EXACT;
    }
    score -= ((lower.len() - needle.len()) as i32 / 4).min(PENALTY_LENGTH_MAX);
    Some(FuzzyMatch { score, positions })
}

/// One command a [`CommandPalette`] lists.
#[derive(Clone, Debug)]
pub struct PaletteItem<'a, T> {
    /// What the palette returns when the item is picked.
    pub id: T,
    pub icon: Option<Icon>,
    pub title: &'a str,
    /// Dim text after the title; searched with a lower score.
    pub detail: &'a str,
    /// Group the item is listed under while the query is empty.
    pub section: &'a str,
    /// A hint such as `Ctrl+S`, drawn as keycaps.
    pub shortcut: Option<&'a str>,
    /// Extra words that find the item without showing.
    pub keywords: &'a [&'a str],
}

impl<'a, T> PaletteItem<'a, T> {
    pub fn new(id: T, title: &'a str) -> Self {
        Self {
            id,
            icon: None,
            title,
            detail: "",
            section: "",
            shortcut: None,
            keywords: &[],
        }
    }

    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn detail(mut self, detail: &'a str) -> Self {
        self.detail = detail;
        self
    }

    pub fn section(mut self, section: &'a str) -> Self {
        self.section = section;
        self
    }

    pub fn shortcut(mut self, shortcut: &'a str) -> Self {
        self.shortcut = Some(shortcut);
        self
    }

    pub fn keywords(mut self, keywords: &'a [&'a str]) -> Self {
        self.keywords = keywords;
        self
    }

    /// How well `query` finds this item; positions are in the title, empty for other matches.
    pub fn matches(&self, query: &str) -> Option<FuzzyMatch> {
        let title = fuzzy_match(query, self.title);
        let keyword = self
            .keywords
            .iter()
            .filter_map(|k| fuzzy_match(query, k))
            .map(|m| m.score - PENALTY_KEYWORD)
            .max();
        let detail = fuzzy_match(query, self.detail).map(|m| m.score - PENALTY_DETAIL);
        let other = keyword.into_iter().chain(detail).max();
        match (title, other) {
            (Some(t), Some(o)) if o > t.score => Some(FuzzyMatch {
                score: o,
                positions: Vec::new(),
            }),
            (Some(t), _) => Some(t),
            (None, Some(o)) => Some(FuzzyMatch {
                score: o,
                positions: Vec::new(),
            }),
            (None, None) => None,
        }
    }
}

/// One line of the result list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Row<'s> {
    Header(&'s str),
    Item { index: usize, positions: Vec<usize> },
}

/// The list for `query`: grouped by section while it is empty, best match first otherwise.
pub(crate) fn rows<'s, T>(items: &[PaletteItem<'s, T>], query: &str) -> Vec<Row<'s>> {
    if query.trim().is_empty() {
        let mut sections: Vec<&str> = Vec::new();
        for item in items {
            if !sections.contains(&item.section) {
                sections.push(item.section);
            }
        }
        let mut out = Vec::with_capacity(items.len() + sections.len());
        for section in sections {
            if !section.is_empty() {
                out.push(Row::Header(section));
            }
            for (index, _) in items
                .iter()
                .enumerate()
                .filter(|(_, i)| i.section == section)
            {
                out.push(Row::Item {
                    index,
                    positions: Vec::new(),
                });
            }
        }
        return out;
    }
    let mut found: Vec<(usize, FuzzyMatch)> = items
        .iter()
        .enumerate()
        .filter_map(|(i, item)| item.matches(query).map(|m| (i, m)))
        .collect();
    found.sort_by(|a, b| b.1.score.cmp(&a.1.score).then(a.0.cmp(&b.0)));
    found
        .into_iter()
        .map(|(index, m)| Row::Item {
            index,
            positions: m.positions,
        })
        .collect()
}

/// Indices of the items the list shows, in order.
pub(crate) fn entries(rows: &[Row<'_>]) -> Vec<usize> {
    rows.iter()
        .filter_map(|r| match r {
            Row::Item { index, .. } => Some(*index),
            Row::Header(_) => None,
        })
        .collect()
}

/// Open state, query and selection of a [`CommandPalette`]; keep one per palette.
#[derive(Clone, Debug, Default)]
pub struct CommandPaletteState {
    pub open: bool,
    pub query: String,
    /// Index into the shown results.
    pub selected: usize,
    scroll: bool,
    /// The palette drew on the last frame; the first frame it draws focuses the query.
    shown: bool,
}

impl CommandPaletteState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens with an empty query and the first result selected.
    pub fn open(&mut self) {
        self.open = true;
        self.query.clear();
        self.selected = 0;
        self.scroll = true;
        self.shown = false;
    }

    /// Closes and clears the query.
    pub fn close(&mut self) {
        self.open = false;
        self.query.clear();
        self.selected = 0;
        self.shown = false;
    }

    pub fn toggle(&mut self) {
        if self.open {
            self.close();
        } else {
            self.open();
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Consumes `shortcut` when pressed this frame and toggles the palette; returns whether it did.
    pub fn toggle_on(&mut self, ctx: &Context, shortcut: &KeyboardShortcut) -> bool {
        let pressed = ctx.input_mut(|i| i.consume_shortcut(shortcut));
        if pressed {
            self.toggle();
        }
        pressed
    }
}

/// Keys the palette read this frame.
#[derive(Default)]
struct Keys {
    down: usize,
    up: usize,
    page_down: usize,
    page_up: usize,
    enter: bool,
    escape: bool,
}

impl Keys {
    fn take(ctx: &Context) -> Self {
        ctx.input_mut(|i| {
            let back_tab = i.count_and_consume_key(Modifiers::SHIFT, Key::Tab);
            let tab = i.count_and_consume_key(Modifiers::NONE, Key::Tab);
            Self {
                down: i.count_and_consume_key(Modifiers::NONE, Key::ArrowDown) + tab,
                up: i.count_and_consume_key(Modifiers::NONE, Key::ArrowUp) + back_tab,
                page_down: i.count_and_consume_key(Modifiers::NONE, Key::PageDown),
                page_up: i.count_and_consume_key(Modifiers::NONE, Key::PageUp),
                enter: i.consume_key(Modifiers::NONE, Key::Enter),
                escape: i.consume_key(Modifiers::NONE, Key::Escape)
                    | i.consume_key(Modifiers::NONE, Key::BrowserBack),
            }
        })
    }

    /// `selected` moved through `count` results; wraps on the arrows, stops at the ends on pages.
    fn apply(&self, selected: usize, count: usize) -> usize {
        if count == 0 {
            return 0;
        }
        let mut sel = selected.min(count - 1);
        for _ in 0..self.down {
            sel = (sel + 1) % count;
        }
        for _ in 0..self.up {
            sel = (sel + count - 1) % count;
        }
        sel = (sel + PAGE_ROWS * self.page_down).min(count - 1);
        sel.saturating_sub(PAGE_ROWS * self.page_up)
    }

    fn moved(&self) -> bool {
        self.down + self.up + self.page_down + self.page_up > 0
    }
}

fn palette_id() -> Id {
    Id::unique("egui_theme_kit.command_palette")
}

/// A modal list of commands near the top of the screen, filtered as the user types.
#[must_use = "call `show` to draw the palette"]
pub struct CommandPalette<'a> {
    id: Id,
    hint: &'a str,
    empty: &'a str,
    width: f32,
    list_height: f32,
    footer: bool,
}

impl Default for CommandPalette<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> CommandPalette<'a> {
    pub fn new() -> Self {
        Self {
            id: palette_id(),
            hint: "Type a command or search",
            empty: "No matching commands",
            width: PALETTE_W,
            list_height: PALETTE_LIST_H,
            footer: true,
        }
    }

    /// Salt for the palette's ids, for more than one palette.
    pub fn id_salt(mut self, salt: impl AsIdSalt) -> Self {
        self.id = palette_id().with(salt);
        self
    }

    /// Placeholder in the empty query field.
    pub fn hint(mut self, hint: &'a str) -> Self {
        self.hint = hint;
        self
    }

    /// Text shown when nothing matches.
    pub fn empty_text(mut self, text: &'a str) -> Self {
        self.empty = text;
        self
    }

    /// Widest the palette gets; it never leaves the screen.
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Tallest the result list gets.
    pub fn list_height(mut self, height: f32) -> Self {
        self.list_height = height;
        self
    }

    /// The key hints under the list on wide palettes; on by default.
    pub fn footer(mut self, footer: bool) -> Self {
        self.footer = footer;
        self
    }

    /// The id of the query field, focused when the palette opens.
    pub fn query_id(&self) -> Id {
        self.id.with("query")
    }

    /// Draws the palette while `state` is open; returns the id of the item picked this frame.
    pub fn show<T: Clone>(
        self,
        ctx: &Context,
        state: &mut CommandPaletteState,
        items: &[PaletteItem<'_, T>],
    ) -> Option<T> {
        if !state.open {
            state.shown = false;
            return None;
        }
        let opening = !std::mem::replace(&mut state.shown, true);
        let keys = Keys::take(ctx);
        if keys.escape {
            state.close();
            return None;
        }
        let mut list = rows(items, &state.query);
        let mut shown = entries(&list);
        state.selected = keys.apply(state.selected, shown.len());
        if keys.moved() {
            state.scroll = true;
        }
        let mut picked = keys
            .enter
            .then(|| shown.get(state.selected).copied())
            .flatten();

        let t = Tokens::get(ctx);
        let m = Metrics::get(ctx);
        let screen = ctx.content_rect();
        let width = self
            .width
            .min(screen.width() - 2.0 * SCREEN_MARGIN)
            .max(1.0);
        let inner_w = width - 2.0 * MARGIN as f32;
        let top = (screen.height() * TOP_SHARE).clamp(SCREEN_MARGIN, TOP_MAX);
        let query_h = m.button_h + QUERY_EXTRA_H;
        let row_h = m.row_h + ROW_EXTRA_H;
        let footer = self.footer && inner_w >= FOOTER_MIN_W;
        let footer_h = if footer { query_h } else { 0.0 };
        let room = screen.height() - top - SCREEN_MARGIN - query_h - footer_h - 4.0 * MARGIN as f32;
        let list_h = room.min(self.list_height).max(2.0 * row_h);
        let has_icons = items.iter().any(|i| i.icon.is_some());

        let area = Modal::default_area(self.id).anchor(Align2::CENTER_TOP, vec2(0.0, top));
        let frame = Frame::window(&ctx.global_style())
            .inner_margin(MARGIN)
            .corner_radius(t.radius_lg);
        let response = Modal::new(self.id).area(area).frame(frame).show(ctx, |ui| {
            ui.set_width(inner_w);
            ui.spacing_mut().item_spacing.y = 0.0;
            if self.query_row(ui, state, inner_w, query_h, opening) {
                list = rows(items, &state.query);
                shown = entries(&list);
                state.selected = 0;
                state.scroll = true;
            }
            hairline(ui);
            let scroll_to = std::mem::take(&mut state.scroll);
            let mut area = ScrollArea::vertical()
                .id_salt(self.id.with("list"))
                .max_height(list_h)
                .auto_shrink([false, true]);
            if scroll_to && state.selected == 0 {
                area = area.vertical_scroll_offset(0.0);
            }
            area.show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                if shown.is_empty() {
                    let (rect, _) = ui.allocate_exact_size(
                        vec2(ui.available_width(), 2.0 * row_h),
                        Sense::hover(),
                    );
                    let text = elided(
                        ui,
                        self.empty,
                        TextStyle::Body.resolve(ui.style()),
                        rect.width(),
                    );
                    ui.painter().galley(
                        (rect.center() - text.size() / 2.0).round(),
                        text,
                        t.text_faint,
                    );
                    return;
                }
                let moving = ui.input(|i| i.pointer.delta() != Vec2::ZERO);
                let mut k = 0;
                for row in &list {
                    match row {
                        Row::Header(name) => section_row(ui, &t, name, m.row_h),
                        Row::Item { index, positions } => {
                            let selected = k == state.selected;
                            let look = RowLook {
                                height: row_h,
                                has_icons,
                                selected,
                            };
                            let hit = item_row(ui, &t, &m, &items[*index], positions, &look);
                            if hit.hovered() && moving {
                                state.selected = k;
                            }
                            if hit.clicked() {
                                picked = Some(*index);
                            }
                            if selected && scroll_to {
                                hit.scroll_to_me(None);
                            }
                            k += 1;
                        }
                    }
                }
            });
            if footer {
                hairline(ui);
                footer_row(ui, &t, inner_w, query_h);
            }
        });
        if picked.is_none() && response.should_close() {
            state.close();
            return None;
        }
        let id = picked.map(|i| items[i].id.clone());
        if id.is_some() {
            state.close();
        }
        id
    }

    /// The search icon and query editor, focused when `opening`; returns whether the query changed.
    fn query_row(
        &self,
        ui: &mut Ui,
        state: &mut CommandPaletteState,
        width: f32,
        height: f32,
        opening: bool,
    ) -> bool {
        let (t, m) = kit(ui);
        let (row, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
        let slot = Rect::from_center_size(
            pos2(row.left() + ROW_PAD_X + m.icon_md / 2.0, row.center().y),
            Vec2::splat(m.icon_md),
        );
        icons::SEARCH.paint(ui, slot, m.icon_md, t.text_dim);
        let text_rect = Rect::from_min_max(pos2(slot.right() + m.gap.x, row.top()), row.max);
        let layout = Layout::left_to_right(Align::Center);
        let mut child = ui.new_child(UiBuilder::new().max_rect(text_rect).layout(layout));
        child.visuals_mut().selection.bg_fill = t.primary_soft;
        let body = TextStyle::Body.resolve(ui.style());
        let font = FontId::new(body.size + QUERY_EXTRA_PT, body.family);
        let filter = EventFilter {
            tab: true,
            horizontal_arrows: true,
            vertical_arrows: true,
            escape: false,
        };
        let output = TextEdit::singleline(&mut state.query)
            .with_class(widget_theme::STOCK)
            .id(self.query_id())
            .frame(Frame::NONE)
            .hint_text(self.hint)
            .font(font)
            .text_color(t.text)
            .event_filter(filter)
            .return_key(None::<KeyboardShortcut>)
            .desired_width(text_rect.width())
            .min_size(text_rect.size())
            .align(Align2::LEFT_CENTER)
            .show(&mut child);
        if opening {
            output.response.request_focus();
        }
        output.response.changed()
    }
}

/// Draws the palette over `ctx` while `state` is open; returns the id of the item picked.
pub fn command_palette<T: Clone>(
    ctx: &Context,
    state: &mut CommandPaletteState,
    items: &[PaletteItem<'_, T>],
) -> Option<T> {
    CommandPalette::new().show(ctx, state, items)
}

/// A faint section label row.
fn section_row(ui: &mut Ui, t: &Tokens, name: &str, height: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let font = medium_font_sized(ui, SECTION_PT);
    let text = elided(ui, name, font, rect.width() - 2.0 * ROW_PAD_X);
    let y = (rect.bottom() - text.size().y - 2.0).round();
    ui.painter()
        .galley(pos2(rect.left() + ROW_PAD_X, y), text, t.text_faint);
}

/// How a result row draws.
struct RowLook {
    height: f32,
    has_icons: bool,
    selected: bool,
}

/// `title` on one row no wider than `max_w`, with the chars at `positions` in the primary role.
fn highlighted(ui: &Ui, t: &Tokens, title: &str, positions: &[usize], max_w: f32) -> Arc<Galley> {
    let body = TextStyle::Body.resolve(ui.style());
    let strong = semibold_font_sized(ui, body.size);
    let plain = TextFormat::simple(body, t.text);
    let hit = TextFormat::simple(strong, t.primary);
    let mut job = LayoutJob::default();
    let mut run = String::new();
    let mut run_hit = false;
    for (i, c) in title.chars().enumerate() {
        let is_hit = positions.binary_search(&i).is_ok();
        if is_hit != run_hit && !run.is_empty() {
            let format = if run_hit { hit.clone() } else { plain.clone() };
            job.append(&run, 0.0, format);
            run.clear();
        }
        run_hit = is_hit;
        run.push(c);
    }
    if !run.is_empty() {
        job.append(&run, 0.0, if run_hit { hit } else { plain });
    }
    job.wrap.max_width = max_w.max(1.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('…');
    ui.painter().layout_job(job)
}

/// One result row: icon, highlighted title, dim detail and keycaps; senses clicks without taking focus.
fn item_row<T>(
    ui: &mut Ui,
    t: &Tokens,
    m: &Metrics,
    item: &PaletteItem<'_, T>,
    positions: &[usize],
    look: &RowLook,
) -> Response {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), look.height), Sense::CLICK);
    response.widget_info(|| {
        WidgetInfo::selected(Role::Button, ui.is_enabled(), look.selected, item.title)
    });
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let p = ui.painter();
    if look.selected {
        p.rect_filled(rect, t.radius_sm, t.primary_soft);
    } else if response.hovered() {
        p.rect_filled(rect, t.radius_sm, t.hover);
    }
    let cy = rect.center().y;
    let mut x = rect.left() + ROW_PAD_X;
    if look.has_icons {
        if let Some(icon) = item.icon {
            let slot =
                Rect::from_center_size(pos2(x + m.icon_md / 2.0, cy), Vec2::splat(m.icon_md));
            let ink = if look.selected { t.primary } else { t.icon };
            icon.paint(ui, slot, m.icon_md, ink);
        }
        x += m.icon_md + m.gap.x;
    }
    let mut right = rect.right() - ROW_PAD_X;
    if let Some(shortcut) = item.shortcut {
        let keys = Kbd::new(shortcut);
        let size = keys.size(ui);
        if size.x < (right - x) / 2.0 {
            let slot = Rect::from_min_size(pos2(right - size.x, cy - size.y / 2.0), size);
            keys.paint(ui, slot);
            right = slot.left() - m.gap.x;
        }
    }
    let room = (right - x).max(1.0);
    let title_room = if item.detail.is_empty() {
        room
    } else {
        room * TITLE_SHARE
    };
    let title = highlighted(ui, t, item.title, positions, title_room);
    let title_w = title.size().x;
    ui.painter()
        .galley(pos2(x, (cy - title.size().y / 2.0).round()), title, t.text);
    let detail_x = x + title_w + m.gap.x;
    if !item.detail.is_empty() && right - detail_x > m.gap.x {
        let small = TextStyle::Small.resolve(ui.style());
        let detail = elided(ui, item.detail, small, right - detail_x);
        let pos = pos2(detail_x, (cy - detail.size().y / 2.0).round());
        ui.painter().galley(pos, detail, t.text_dim);
    }
    response
}

/// Key hints: arrows to move, Enter to run, Esc to close.
fn footer_row(ui: &mut Ui, t: &Tokens, width: f32, height: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let small = TextStyle::Small.resolve(ui.style());
    let mut x = rect.left() + ROW_PAD_X;
    let gap = 6.0;
    for (keys, label) in [("Up+Down", "move"), ("Enter", "run"), ("Esc", "close")] {
        let caps = Kbd::new(keys);
        let size = caps.size(ui);
        caps.paint(
            ui,
            Rect::from_min_size(pos2(x, rect.center().y - size.y / 2.0), size),
        );
        x += size.x + gap;
        let text = text_galley(ui, label, small.clone());
        let w = text.size().x;
        ui.painter().galley(
            pos2(x, (rect.center().y - text.size().y / 2.0).round()),
            text,
            t.text_faint,
        );
        x += w + 3.0 * gap;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_match_needs_every_query_char_in_order() {
        assert!(fuzzy_match("gst", "Git status").is_some());
        assert!(fuzzy_match("tsg", "Git status").is_none());
        assert_eq!(fuzzy_match("", "anything"), Some(FuzzyMatch::default()));
        assert_eq!(
            fuzzy_match("GS", "git status").map(|m| m.positions),
            Some(vec![0, 4])
        );
    }

    #[test]
    fn prefixes_and_word_starts_beat_scattered_matches() {
        let score = |q, t| fuzzy_match(q, t).map(|m| m.score).unwrap_or(i32::MIN);
        assert!(score("gs", "Git status") > score("gs", "Settings"));
        assert!(score("set", "Settings") > score("set", "Show settings"));
        assert!(score("set", "Show settings") > score("set", "Reset view"));
        assert!(score("open", "Open") > score("open", "Open file"));
        assert!(score("nf", "newFile") > score("nf", "unify"));
    }

    #[test]
    fn keywords_find_an_item_without_highlighting_its_title() {
        let item = PaletteItem::new(1, "Toggle theme").keywords(&["dark", "light"]);
        let found = item.matches("dark").expect("keyword match");
        assert!(found.positions.is_empty());
        assert!(item.matches("zzz").is_none());
    }

    #[test]
    fn an_empty_query_groups_by_section_in_first_seen_order() {
        let items = [
            PaletteItem::new(0, "Open").section("File"),
            PaletteItem::new(1, "Theme").section("View"),
            PaletteItem::new(2, "Save").section("File"),
            PaletteItem::new(3, "Loose"),
        ];
        let list = rows(&items, "");
        assert_eq!(list[0], Row::Header("File"));
        assert_eq!(entries(&list), [0, 2, 1, 3]);
        assert!(!list.contains(&Row::Header("")));
    }

    #[test]
    fn keys_wrap_on_arrows_and_stop_at_the_ends_on_pages() {
        let keys = |down, up, page_down, page_up| Keys {
            down,
            up,
            page_down,
            page_up,
            ..Keys::default()
        };
        assert_eq!(keys(1, 0, 0, 0).apply(4, 5), 0);
        assert_eq!(keys(0, 1, 0, 0).apply(0, 5), 4);
        assert_eq!(keys(0, 0, 1, 0).apply(2, 20), 10);
        assert_eq!(keys(0, 0, 3, 0).apply(2, 20), 19);
        assert_eq!(keys(0, 0, 0, 1).apply(5, 20), 0);
        assert_eq!(keys(2, 0, 0, 0).apply(9, 0), 0);
    }
}
