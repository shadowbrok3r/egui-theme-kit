//! Chat chrome: the global app bar, the per-session title card and the session list.

mod sessions;

use std::hash::Hash;

use egui::{AsIdSalt, IdSalt, Ui};

pub use sessions::{
    MenuEntry, NewSession, NewSessionButton, RowKind, SessionList, SessionListAction, SessionRow,
    SessionSection,
};

use super::transcript::DetailLevel;
use super::types::{CowStr, MessageNav, NavStep, SessionStatus, Tone};
use crate::controls::Icon;

/// One entry of a [`ViewMenu`].
#[derive(Clone, Debug)]
pub struct ViewItem<'a, V> {
    pub value: V,
    pub label: CowStr<'a>,
    pub icon: Option<Icon>,
    pub badge: Option<u32>,
    pub tone: Tone,
}

/// A dropdown that switches the main view; the closed menu shows the badge total.
#[derive(Clone, Debug)]
pub struct ViewMenu<'a, V> {
    pub selected: V,
    pub items: &'a [ViewItem<'a, V>],
}

impl<'a, V: Clone + PartialEq> ViewMenu<'a, V> {
    pub fn new(selected: V, items: &'a [ViewItem<'a, V>]) -> Self {
        Self { selected, items }
    }

    /// The picked value when it differs from `selected`.
    pub fn show(self, ui: &mut Ui) -> Option<V> {
        let _ = (ui, &self.selected, self.items);
        None
    }
}

/// A square icon button in a bar.
#[derive(Clone, Debug)]
pub struct BarButton<'a, K> {
    pub id: K,
    pub icon: Icon,
    pub tip: CowStr<'a>,
    pub selected: bool,
    pub badge: Option<u32>,
}

/// What the user did in the app bar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppBarAction<V, K> {
    View(V),
    Detail(DetailLevel),
    Thinking(bool),
    Button(K),
}

/// The flat global bar: view menu, detail switch, thinking toggle and trailing buttons.
pub struct ChatAppBar<'a, V, K> {
    id_salt: IdSalt,
    view: Option<ViewMenu<'a, V>>,
    detail: Option<&'a mut DetailLevel>,
    thinking: Option<&'a mut bool>,
    trailing: &'a [BarButton<'a, K>],
}

impl<'a, V: Clone + PartialEq, K: Clone + Hash> ChatAppBar<'a, V, K> {
    pub fn new(id_salt: impl AsIdSalt) -> Self {
        Self {
            id_salt: IdSalt::new(id_salt),
            view: None,
            detail: None,
            thinking: None,
            trailing: &[],
        }
    }

    pub fn view(mut self, view: ViewMenu<'a, V>) -> Self {
        self.view = Some(view);
        self
    }

    /// Adds the Simple and Detailed switch.
    pub fn detail(mut self, detail: &'a mut DetailLevel) -> Self {
        self.detail = Some(detail);
        self
    }

    /// Adds the Thinking toggle, label on the left.
    pub fn thinking(mut self, thinking: &'a mut bool) -> Self {
        self.thinking = Some(thinking);
        self
    }

    pub fn trailing(mut self, buttons: &'a [BarButton<'a, K>]) -> Self {
        self.trailing = buttons;
        self
    }

    /// Draws the bar with `leading` content first and a rule under it.
    pub fn show(self, ui: &mut Ui, leading: impl FnOnce(&mut Ui)) -> Option<AppBarAction<V, K>> {
        let Self {
            id_salt,
            view,
            detail,
            thinking,
            trailing,
        } = self;
        let _ = (ui, leading, id_salt, view, detail, thinking, trailing);
        None
    }
}

/// What an [`OptionChip`] does when used.
#[derive(Clone, Debug)]
pub enum ChipKind<'a> {
    /// Opens a menu of options.
    Menu {
        options: &'a [CowStr<'a>],
        selected: Option<usize>,
    },
    /// A fill meter with an optional action, such as Compact.
    Meter {
        fraction: f32,
        action: Option<CowStr<'a>>,
    },
    Toggle {
        on: bool,
    },
    /// Read-only.
    Info,
}

/// One session option in the title card, such as model, permissions or context use.
#[derive(Clone, Debug)]
pub struct OptionChip<'a, K> {
    pub id: K,
    pub label: CowStr<'a>,
    pub value: CowStr<'a>,
    pub icon: Option<Icon>,
    pub tone: Tone,
    pub kind: ChipKind<'a>,
}

/// What the user did in the session title card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionBarAction<K> {
    Rename(String),
    Chip { id: K, choice: usize },
    ChipAction(K),
    ChipToggle { id: K, on: bool },
    Button(K),
    Nav(NavStep),
    DismissError,
}

/// The per-session title card: status, centred title and subtitle, chips, buttons and message jump.
pub struct SessionBar<'a, K> {
    id_salt: IdSalt,
    status: SessionStatus,
    status_label: Option<CowStr<'a>>,
    title: CowStr<'a>,
    subtitle: Option<CowStr<'a>>,
    renamable: bool,
    chips: &'a [OptionChip<'a, K>],
    buttons: &'a [BarButton<'a, K>],
    nav: Option<MessageNav>,
    error: Option<CowStr<'a>>,
}

impl<'a, K: Clone + Hash> SessionBar<'a, K> {
    pub fn new(id_salt: impl AsIdSalt, title: impl Into<CowStr<'a>>) -> Self {
        Self {
            id_salt: IdSalt::new(id_salt),
            status: SessionStatus::Idle,
            status_label: None,
            title: title.into(),
            subtitle: None,
            renamable: false,
            chips: &[],
            buttons: &[],
            nav: None,
            error: None,
        }
    }

    /// Status dot and label; the label defaults to [`SessionStatus::label`].
    pub fn status(mut self, status: SessionStatus, label: Option<CowStr<'a>>) -> Self {
        self.status = status;
        self.status_label = label;
        self
    }

    pub fn subtitle(mut self, subtitle: impl Into<CowStr<'a>>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn renamable(mut self, renamable: bool) -> Self {
        self.renamable = renamable;
        self
    }

    pub fn chips(mut self, chips: &'a [OptionChip<'a, K>]) -> Self {
        self.chips = chips;
        self
    }

    pub fn buttons(mut self, buttons: &'a [BarButton<'a, K>]) -> Self {
        self.buttons = buttons;
        self
    }

    /// Adds the labelled Your messages jump with `n/N`.
    pub fn nav(mut self, nav: MessageNav) -> Self {
        self.nav = Some(nav);
        self
    }

    /// A dismissable error callout under the card.
    pub fn error(mut self, error: impl Into<CowStr<'a>>) -> Self {
        self.error = Some(error.into());
        self
    }

    pub fn show(self, ui: &mut Ui) -> Option<SessionBarAction<K>> {
        let Self {
            id_salt,
            status,
            status_label,
            title,
            subtitle,
            renamable,
            chips,
            buttons,
            nav,
            error,
        } = self;
        let _ = (
            ui,
            id_salt,
            status,
            status_label,
            title,
            subtitle,
            renamable,
            chips,
            buttons,
            nav,
            error,
        );
        None
    }
}
