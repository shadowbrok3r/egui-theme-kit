//! The session list and the new-session button.

use std::hash::Hash;

use egui::{AsIdSalt, IdSalt, Ui};

use crate::chat::types::{CowStr, SessionStatus, TimeFormat, UnixMs};
use crate::controls::{Icon, PaletteItem};

/// How a session row is styled and sorted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RowKind {
    #[default]
    Normal,
    Pinned,
    Manager,
    Inbox,
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
    time_format: Option<&'a dyn TimeFormat>,
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
            time_format,
            now,
        } = self;
        let _ = (
            ui,
            id_salt,
            pinned,
            sections,
            selected,
            search,
            icon_only,
            row_menu,
            time_format.is_some(),
            now,
        );
        None
    }
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
        let _ = (ui, &self.label, self.items);
        None
    }
}
