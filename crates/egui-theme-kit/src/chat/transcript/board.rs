//! Waiting cards across sessions, grouped per session: the inbox and the manager desk.

use std::hash::Hash;

use egui::{AsIdSalt, IdSalt, Ui};

use super::entry::ChatEntry;
use crate::chat::types::{CardAction, CowStr, SessionStatus};

/// The waiting cards of one session.
#[derive(Clone, Debug)]
pub struct BoardGroup<'a, K> {
    pub session: K,
    pub header: CowStr<'a>,
    pub subtitle: CowStr<'a>,
    pub status: SessionStatus,
    /// Approval and question entries; other variants are skipped.
    pub cards: &'a [ChatEntry<'a>],
}

/// What the user did on the board.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BoardAction<K> {
    OpenSession(K),
    Card {
        session: K,
        key: String,
        action: CardAction,
    },
}

/// Every waiting card of several sessions, one section per session.
pub struct WaitingBoard<'a, K> {
    id_salt: IdSalt,
    groups: &'a [BoardGroup<'a, K>],
}

impl<'a, K: Clone + Hash> WaitingBoard<'a, K> {
    pub fn new(id_salt: impl AsIdSalt, groups: &'a [BoardGroup<'a, K>]) -> Self {
        Self {
            id_salt: IdSalt::new(id_salt),
            groups,
        }
    }

    pub fn show(self, ui: &mut Ui) -> Option<BoardAction<K>> {
        let Self { id_salt, groups } = self;
        let _ = (ui, id_salt, groups);
        None
    }
}
