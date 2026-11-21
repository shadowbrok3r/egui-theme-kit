//! Waiting cards across sessions, grouped per session: the inbox and the manager desk.

use std::hash::Hash;

use egui::{AsIdSalt, IdSalt, Ui};

use super::cards::entry_card;
use super::entry::ChatEntry;
use super::rich::{one_row, plain};
use super::row::RowCx;
use crate::chat::types::{CardAction, CowStr, SessionStatus};
use crate::controls::{StatusDot, TextButton};
use crate::kit::ButtonKind;

/// The waiting cards of one session.
#[derive(Clone, Debug)]
pub struct BoardGroup<'a, K> {
    pub session: K,
    pub header: CowStr<'a>,
    pub subtitle: CowStr<'a>,
    pub status: SessionStatus,
    /// Approval, question and plan entries; other variants are skipped.
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
    empty: &'a str,
}

impl<'a, K: Clone + Hash> WaitingBoard<'a, K> {
    pub fn new(id_salt: impl AsIdSalt, groups: &'a [BoardGroup<'a, K>]) -> Self {
        Self {
            id_salt: IdSalt::new(id_salt),
            groups,
            empty: "Nothing is waiting",
        }
    }

    /// Text shown when no group has a card.
    pub fn empty_text(mut self, text: &'a str) -> Self {
        self.empty = text;
        self
    }

    pub fn show(self, ui: &mut Ui) -> Option<BoardAction<K>> {
        let Self {
            id_salt,
            groups,
            empty,
        } = self;
        let id = ui.scope_id().with(("egui_theme_kit.chat.board", id_salt));
        let mut rc = RowCx::new(ui, id);
        let mut out = None;
        let shown: Vec<_> = groups
            .iter()
            .filter(|g| g.cards.iter().any(is_card))
            .collect();
        if shown.is_empty() {
            let font = rc.f.body.clone();
            plain(ui, &rc, empty, &font, rc.p.text_faint);
            return None;
        }
        for group in shown {
            let salt = session_salt(&group.session);
            let session_id = id.with(salt);
            ui.scope_builder(egui::UiBuilder::new().id_salt(salt), |ui| {
                ui.spacing_mut().item_spacing.y = rc.m.gap.y;
                if group_header(ui, &rc, group) {
                    out = Some(BoardAction::OpenSession(group.session.clone()));
                }
                for entry in group.cards.iter().filter(|e| is_card(e)) {
                    if let Some(action) = entry_card(ui, &mut rc, entry, session_id) {
                        out = Some(BoardAction::Card {
                            session: group.session.clone(),
                            key: entry.key().to_owned(),
                            action,
                        });
                    }
                }
                ui.add_space(rc.m.gap.y);
            });
        }
        out
    }
}

/// A stable salt for a session key.
fn session_salt<K: Hash>(key: &K) -> u64 {
    use std::hash::{BuildHasher, BuildHasherDefault, DefaultHasher};
    BuildHasherDefault::<DefaultHasher>::default().hash_one(key)
}

fn is_card(entry: &ChatEntry<'_>) -> bool {
    matches!(
        entry,
        ChatEntry::Approval(_) | ChatEntry::Question(_) | ChatEntry::Plan(_)
    )
}

/// Status dot, header and subtitle, and an Open button; returns whether Open was tapped.
fn group_header<K>(ui: &mut Ui, rc: &RowCx<'_>, group: &BoardGroup<'_, K>) -> bool {
    let mut open = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = rc.m.gap.x / 2.0;
        StatusDot::new(group.status.tone().color(&rc.p))
            .pulse(group.status == SessionStatus::Working)
            .label(group.status.label())
            .show(ui);
        let button = TextButton::new("Open").kind(ButtonKind::Ghost);
        let button_w = button.size(ui).x + rc.m.gap.x;
        let room = (ui.available_width() - button_w).max(rc.m.control_h);
        let title = one_row(ui, &group.header, &rc.f.strong, rc.p.text, room);
        let title_w = title.size().x;
        ui.add(egui::Label::new(title).selectable(false));
        if !group.subtitle.is_empty() {
            let sub = one_row(
                ui,
                &group.subtitle,
                &rc.f.small,
                rc.p.text_dim,
                (room - title_w - rc.m.gap.x).max(0.0),
            );
            ui.add(egui::Label::new(sub).selectable(false));
        }
        ui.add_space((ui.available_width() - button_w).max(0.0));
        open = rc.tapped(&button.show(ui));
    });
    open
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::transcript::cards::{ApprovalCard, CardChoice, CardState};
    use crate::chat::types::CowList;
    use std::borrow::Cow;

    #[test]
    fn the_board_draws_cards_and_skips_sessions_without_any() {
        let choices = [CardChoice {
            label: Cow::Borrowed("Approve"),
            kind: ButtonKind::Primary,
            shortcut: Some(Cow::Borrowed("Y")),
        }];
        let cards = [ChatEntry::Approval(ApprovalCard {
            key: Cow::Borrowed("a1"),
            context: Cow::Borrowed("desk"),
            title: Cow::Borrowed("Run tests"),
            command: Some(Cow::Borrowed("cargo test")),
            detail: None,
            reason: None,
            choices: CowList::Borrowed(&choices),
            state: CardState::Open,
        })];
        let groups = [
            BoardGroup {
                session: 1u32,
                header: Cow::Borrowed("Engine"),
                subtitle: Cow::Borrowed("desk"),
                status: SessionStatus::Waiting,
                cards: &cards,
            },
            BoardGroup {
                session: 2u32,
                header: Cow::Borrowed("Idle"),
                subtitle: Cow::Borrowed(""),
                status: SessionStatus::Idle,
                cards: &[],
            },
        ];
        let ctx = egui::Context::default();
        for _ in 0..2 {
            let mut out = ctx.run_ui(Default::default(), |ui| {
                assert!(WaitingBoard::new("board", &groups).show(ui).is_none());
            });
            out.textures_delta.clear();
        }
    }
}
