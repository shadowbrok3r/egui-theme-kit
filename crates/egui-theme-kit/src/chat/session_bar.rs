//! Chat chrome: the global app bar, the per-session title card and the session list.

mod app_bar;
mod chips;
mod layout;
mod parts;
mod row;
mod sessions;
mod title_card;

#[cfg(test)]
mod tests;

use std::hash::Hash;

use egui::{AsIdSalt, IdSalt};

pub use layout::BarLayout;
pub use row::SessionItem;
pub use sessions::{
    MenuEntry, MenuEntryState, NewSession, NewSessionButton, RowKind, RowMenuState, SessionList,
    SessionListAction, SessionRow, SessionSection,
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

impl<'a, V> ViewItem<'a, V> {
    pub fn new(value: V, label: impl Into<CowStr<'a>>) -> Self {
        Self {
            value,
            label: label.into(),
            icon: None,
            badge: None,
            tone: Tone::Neutral,
        }
    }

    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// A count pill in `tone`; zero draws nothing.
    pub fn badge(mut self, count: u32, tone: Tone) -> Self {
        self.badge = Some(count);
        self.tone = tone;
        self
    }
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
}

/// A square icon button in a bar.
#[derive(Clone, Debug)]
pub struct BarButton<'a, K> {
    pub id: K,
    pub icon: Icon,
    pub tip: CowStr<'a>,
    pub selected: bool,
    pub badge: Option<u32>,
    /// Whether the button and its overflow menu row take clicks.
    pub enabled: bool,
}

impl<'a, K> BarButton<'a, K> {
    pub fn new(id: K, icon: impl Into<Icon>, tip: impl Into<CowStr<'a>>) -> Self {
        Self {
            id,
            icon: icon.into(),
            tip: tip.into(),
            selected: false,
            badge: None,
            enabled: true,
        }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// A count over the icon's corner; zero draws nothing.
    pub fn badge(mut self, count: u32) -> Self {
        self.badge = Some(count);
        self
    }

    /// Draws the button and its overflow menu row greyed out and ignores clicks when `false`.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
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
    /// Whether the action of a [`ChipKind::Meter`] takes clicks; when not, it shows greyed out.
    pub action_enabled: bool,
}

impl<'a, K> OptionChip<'a, K> {
    /// A read-only chip; set [`Self::kind`] to make it interactive.
    pub fn new(id: K, label: impl Into<CowStr<'a>>, value: impl Into<CowStr<'a>>) -> Self {
        Self {
            id,
            label: label.into(),
            value: value.into(),
            icon: None,
            tone: Tone::Neutral,
            kind: ChipKind::Info,
            action_enabled: true,
        }
    }

    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    pub fn kind(mut self, kind: ChipKind<'a>) -> Self {
        self.kind = kind;
        self
    }

    /// Shows the meter's action greyed out and ignores it when `false`, in the chip menu and the overflow menu.
    pub fn action_enabled(mut self, enabled: bool) -> Self {
        self.action_enabled = enabled;
        self
    }
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
    status_in_subtitle: Option<bool>,
    title: CowStr<'a>,
    subtitle: Option<CowStr<'a>>,
    renamable: bool,
    /// Hash of the session the card shows; a rename draft of another session is dropped.
    session: Option<u64>,
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
            status_in_subtitle: None,
            title: title.into(),
            subtitle: None,
            renamable: false,
            session: None,
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

    /// In [`BarLayout::Compact`] the subtitle row, shown on a title tap, starts with the status label; on by default on touch screens.
    pub fn status_in_subtitle(mut self, on: bool) -> Self {
        self.status_in_subtitle = Some(on);
        self
    }

    /// Double-clicking the title, or Rename in the overflow menu, edits it in place.
    pub fn renamable(mut self, renamable: bool) -> Self {
        self.renamable = renamable;
        self
    }

    /// The session the card shows; an open rename of another session is discarded. Without it the title stands in.
    pub fn session(mut self, key: impl Hash) -> Self {
        self.session = Some(egui::util::hash(key));
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
}
