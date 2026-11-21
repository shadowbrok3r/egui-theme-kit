//! The session list and the new-session button.

use std::hash::Hash;

use egui::{AsIdSalt, Id, IdSalt, Ui};

use super::row::SessionItem;
use crate::chat::icons::ChatIcons;
use crate::chat::types::{ClockFormat, CowStr, SessionStatus, TimeFormat, Tone, UnixMs};
use crate::controls::{
    ButtonKind, Icon, MenuItem, PaletteItem, Section, TextButton, fuzzy_match, hairline, kit,
    menu_item, menu_separator, popup_menu, search_field, section_label,
};

/// How a session row is styled and sorted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RowKind {
    #[default]
    Normal,
    Pinned,
    Manager,
    Inbox,
}

impl RowKind {
    /// Badge text and tone of the kind; none for plain and pinned rows.
    pub fn badge(self) -> Option<(&'static str, Tone)> {
        match self {
            Self::Normal | Self::Pinned => None,
            Self::Manager => Some(("Manager", Tone::Accent)),
            Self::Inbox => Some(("Inbox", Tone::Info)),
        }
    }
}

/// One session in the list: title, subtitle, status, last update and unread count.
#[derive(Clone, Debug)]
pub struct SessionRow<'a, K> {
    pub key: K,
    pub title: CowStr<'a>,
    pub subtitle: CowStr<'a>,
    pub status: SessionStatus,
    pub updated: Option<UnixMs>,
    pub unread: u32,
    pub kind: RowKind,
}

impl<'a, K> SessionRow<'a, K> {
    pub fn new(key: K, title: impl Into<CowStr<'a>>) -> Self {
        Self {
            key,
            title: title.into(),
            subtitle: CowStr::Borrowed(""),
            status: SessionStatus::Idle,
            updated: None,
            unread: 0,
            kind: RowKind::Normal,
        }
    }

    pub fn subtitle(mut self, subtitle: impl Into<CowStr<'a>>) -> Self {
        self.subtitle = subtitle.into();
        self
    }

    pub fn status(mut self, status: SessionStatus) -> Self {
        self.status = status;
        self
    }

    pub fn updated(mut self, at: UnixMs) -> Self {
        self.updated = Some(at);
        self
    }

    pub fn unread(mut self, count: u32) -> Self {
        self.unread = count;
        self
    }

    pub fn kind(mut self, kind: RowKind) -> Self {
        self.kind = kind;
        self
    }
}

/// Whether a row titled `title` over `subtitle` matches the search `query`.
pub(super) fn row_matches(query: &str, title: &str, subtitle: &str) -> bool {
    let query = query.trim();
    query.is_empty()
        || fuzzy_match(query, title).is_some()
        || fuzzy_match(query, subtitle).is_some()
}

/// A titled, foldable group of rows.
#[derive(Clone, Debug)]
pub struct SessionSection<'a, K> {
    pub title: CowStr<'a>,
    pub rows: &'a [SessionRow<'a, K>],
    pub collapsed_by_default: bool,
}

/// An app-supplied entry of a row's context menu.
#[derive(Clone, Debug)]
pub struct MenuEntry<'a, M> {
    pub id: M,
    pub label: CowStr<'a>,
    pub icon: Option<Icon>,
    pub danger: bool,
}

/// How a row menu entry appears on one row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MenuEntryState {
    #[default]
    Shown,
    Disabled,
    Hidden,
}

/// Decides per row how each [`MenuEntry`] appears.
pub type RowMenuState<'a, K, M> = &'a dyn Fn(&K, &M) -> MenuEntryState;

/// What the user did in the session list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionListAction<K, M> {
    Open(K),
    Menu { key: K, item: M },
    MentionInChat(K),
}

/// Pinned rows, then sections, with an optional search field.
pub struct SessionList<'a, K, M> {
    id_salt: IdSalt,
    pinned: &'a [SessionRow<'a, K>],
    sections: &'a [SessionSection<'a, K>],
    selected: Option<&'a K>,
    search: Option<&'a mut String>,
    icon_only: bool,
    row_menu: &'a [MenuEntry<'a, M>],
    row_menu_state: Option<RowMenuState<'a, K, M>>,
    mention_entry: bool,
    time_format: Option<&'a dyn TimeFormat>,
    now: Option<UnixMs>,
}

/// What every row of one list draws with.
struct RowCtx<'a, K, M> {
    id: Id,
    selected: Option<&'a K>,
    icon_only: bool,
    row_menu: &'a [MenuEntry<'a, M>],
    row_menu_state: Option<RowMenuState<'a, K, M>>,
    mention_entry: bool,
    format: &'a dyn TimeFormat,
    now: Option<UnixMs>,
}

impl<'a, K: Clone + Hash + PartialEq, M: Clone> SessionList<'a, K, M> {
    pub fn new(id_salt: impl AsIdSalt, sections: &'a [SessionSection<'a, K>]) -> Self {
        Self {
            id_salt: IdSalt::new(id_salt),
            pinned: &[],
            sections,
            selected: None,
            search: None,
            icon_only: false,
            row_menu: &[],
            row_menu_state: None,
            mention_entry: false,
            time_format: None,
            now: None,
        }
    }

    pub fn pinned(mut self, rows: &'a [SessionRow<'a, K>]) -> Self {
        self.pinned = rows;
        self
    }

    pub fn selected(mut self, key: Option<&'a K>) -> Self {
        self.selected = key;
        self
    }

    /// Adds a search field bound to `query`.
    pub fn search(mut self, query: &'a mut String) -> Self {
        self.search = Some(query);
        self
    }

    /// Draws status icons only, for a collapsed sidebar.
    pub fn icon_only(mut self, icon_only: bool) -> Self {
        self.icon_only = icon_only;
        self
    }

    pub fn row_menu(mut self, entries: &'a [MenuEntry<'a, M>]) -> Self {
        self.row_menu = entries;
        self
    }

    /// Hides or disables row menu entries per row; every entry shows without it.
    pub fn row_menu_state(mut self, state: RowMenuState<'a, K, M>) -> Self {
        self.row_menu_state = Some(state);
        self
    }

    /// Adds Mention in chat to every row menu; off by default.
    pub fn mention_entry(mut self, show: bool) -> Self {
        self.mention_entry = show;
        self
    }

    /// Formats the updated times; [`crate::chat::ClockFormat`] by default.
    pub fn time_format(mut self, format: &'a dyn TimeFormat, now: Option<UnixMs>) -> Self {
        self.time_format = Some(format);
        self.now = now;
        self
    }

    pub fn show(self, ui: &mut Ui) -> Option<SessionListAction<K, M>> {
        let Self {
            id_salt,
            pinned,
            sections,
            selected,
            search,
            icon_only,
            row_menu,
            row_menu_state,
            mention_entry,
            time_format,
            now,
        } = self;
        let id = ui.make_persistent_id(id_salt);
        let fallback = ClockFormat::default();
        let c = RowCtx {
            id,
            selected,
            icon_only,
            row_menu,
            row_menu_state,
            mention_entry,
            format: time_format.unwrap_or(&fallback),
            now: now.or_else(UnixMs::now),
        };
        let query = match search {
            Some(query) if !icon_only => {
                let width = ui.available_width();
                search_field(ui, query, "Search sessions", width);
                query.trim().to_owned()
            }
            Some(query) => query.trim().to_owned(),
            None => String::new(),
        };
        let mut action = None;
        let shown = |r: &&SessionRow<'a, K>| row_matches(&query, &r.title, &r.subtitle);
        for row in pinned.iter().filter(shown) {
            action = action.or(row_ui(ui, &c, row));
        }
        for (i, section) in sections.iter().enumerate() {
            let rows: Vec<&SessionRow<'a, K>> = section.rows.iter().filter(shown).collect();
            if rows.is_empty() && !query.is_empty() {
                continue;
            }
            if icon_only {
                hairline(ui);
                for row in rows {
                    action = action.or(row_ui(ui, &c, row));
                }
                continue;
            }
            let count = rows.len().to_string();
            let forced = (!query.is_empty()).then_some(true);
            let picked = Section::new(("section", i), &section.title)
                .id(id.with(("section", i)))
                .default_open(!section.collapsed_by_default)
                .open(forced)
                .suffix(&count, None)
                .show(ui, |ui| {
                    let mut picked = None;
                    for row in rows {
                        picked = picked.or(row_ui(ui, &c, row));
                    }
                    picked
                })
                .flatten();
            action = action.or(picked);
        }
        action
    }
}

/// Id of the row keyed `key` in the list `list`.
pub(super) fn row_id(list: Id, key: &impl Hash) -> Id {
    list.with(("row", egui::util::hash(key)))
}

/// One row and its context menu.
fn row_ui<K: Clone + Hash + PartialEq, M: Clone>(
    ui: &mut Ui,
    c: &RowCtx<'_, K, M>,
    row: &SessionRow<'_, K>,
) -> Option<SessionListAction<K, M>> {
    let time = row.updated.map(|at| c.format.label(at, c.now));
    let mut item = SessionItem::new(&row.title)
        .subtitle(&row.subtitle)
        .status(row.status)
        .unread(row.unread)
        .selected(c.selected.is_some_and(|k| *k == row.key))
        .collapsed(c.icon_only)
        .id(row_id(c.id, &row.key));
    if let Some(time) = &time {
        item = item.updated(time);
    }
    if let Some((text, tone)) = row.kind.badge() {
        item = item.badge(text, tone);
    }
    let response = item.show(ui);
    let mut action = response
        .clicked()
        .then(|| SessionListAction::Open(row.key.clone()));
    let entries: Vec<(&MenuEntry<'_, M>, bool)> = c
        .row_menu
        .iter()
        .filter_map(|entry| {
            match c
                .row_menu_state
                .map_or(MenuEntryState::Shown, |f| f(&row.key, &entry.id))
            {
                MenuEntryState::Shown => Some((entry, true)),
                MenuEntryState::Disabled => Some((entry, false)),
                MenuEntryState::Hidden => None,
            }
        })
        .collect();
    if !c.mention_entry && entries.is_empty() {
        return action;
    }
    response.context_menu(|ui| {
        if c.mention_entry {
            let icons = ChatIcons::get(ui.ctx());
            if menu_item(ui, icons.mention, "Mention in chat").clicked() {
                action = Some(SessionListAction::MentionInChat(row.key.clone()));
            }
            if !entries.is_empty() {
                menu_separator(ui);
            }
        }
        for (entry, enabled) in &entries {
            let clicked = MenuItem::new(&entry.label)
                .icon(entry.icon)
                .danger(entry.danger)
                .enabled(*enabled)
                .show(ui)
                .clicked();
            if clicked {
                action = Some(SessionListAction::Menu {
                    key: row.key.clone(),
                    item: entry.id.clone(),
                });
            }
        }
    });
    action
}

/// What the new-session button produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NewSession<T> {
    /// The main button.
    Default,
    /// An entry of its menu.
    Item(T),
}

/// A primary button with a menu of session templates.
pub struct NewSessionButton<'a, T> {
    pub label: CowStr<'a>,
    pub items: &'a [PaletteItem<'a, T>],
}

impl<'a, T: Clone> NewSessionButton<'a, T> {
    pub fn new(label: impl Into<CowStr<'a>>, items: &'a [PaletteItem<'a, T>]) -> Self {
        Self {
            label: label.into(),
            items,
        }
    }

    pub fn show(self, ui: &mut Ui) -> Option<NewSession<T>> {
        let (_, m) = kit(ui);
        let icons = ChatIcons::get(ui.ctx());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = (m.gap.x / 4.0).round();
            let main = TextButton::new(&self.label)
                .icon(icons.plus)
                .kind(ButtonKind::Primary)
                .prominent(true)
                .show(ui);
            let mut out = main.clicked().then_some(NewSession::Default);
            if self.items.is_empty() {
                return out;
            }
            let caret = TextButton::new("")
                .icon(icons.chevron_down)
                .kind(ButtonKind::Primary)
                .prominent(true)
                .show(ui)
                .on_hover_text("More session types");
            let picked = popup_menu(&caret, false, |ui| template_rows(ui, self.items)).flatten();
            out = out.or(picked.map(NewSession::Item));
            out
        })
        .inner
    }
}

/// Template rows grouped under their section labels; returns the picked id.
fn template_rows<T: Clone>(ui: &mut Ui, items: &[PaletteItem<'_, T>]) -> Option<T> {
    let mut picked = None;
    let mut section = None;
    for item in items {
        if section != Some(item.section) {
            if section.is_some() {
                menu_separator(ui);
            }
            if !item.section.is_empty() {
                section_label(ui, item.section);
            }
            section = Some(item.section);
        }
        let row = MenuItem::new(item.title).icon(item.icon).show(ui);
        let row = if item.detail.is_empty() {
            row
        } else {
            row.on_hover_text(item.detail)
        };
        if row.clicked() {
            picked = Some(item.id.clone());
        }
    }
    picked
}
