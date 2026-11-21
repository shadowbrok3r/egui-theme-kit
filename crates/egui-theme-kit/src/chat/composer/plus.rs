//! The `+` menu: attach, mention, app entries and slash commands in a kit command palette.

use egui::{Context, Id, Key};

use super::popup::SlashPick;
use super::slash::{PlusItem, SlashCommand, SlashSource};
use super::state::ComposerState;
use super::{ComposerAction, Outcome, cursor_of, insert_command, store_cursor};
use crate::chat::icons::ChatIcons;
use crate::controls::{CommandPalette, PaletteItem};

/// Section the slash commands without one are listed under.
const COMMANDS_SECTION: &str = "Commands";

/// What a `+` menu row does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Pick {
    Attach,
    Mention,
    Plus(usize),
    Slash(usize),
}

/// The inputs of the `+` menu.
pub(super) struct Menu<'s, 'a> {
    pub id: Id,
    pub show_attach: bool,
    pub editable: bool,
    pub mentions: bool,
    pub items: &'s [PlusItem<'a>],
    pub slash: Option<&'s dyn SlashSource>,
}

/// Commands the menu lists: enabled ones, and only those that run on pick when the text is read-only.
pub(super) fn listed<'c>(
    source: Option<&'c dyn SlashSource>,
    editable: bool,
) -> Vec<SlashCommand<'c>> {
    source
        .map(|s| s.commands(""))
        .unwrap_or_default()
        .into_iter()
        .filter(|c| c.enabled && (editable || SlashPick::from(c).runs_on_pick()))
        .collect()
}

/// Rows of the menu: attach, mention, app entries, then commands by section.
pub(super) fn rows<'r>(
    menu: &Menu<'r, '_>,
    commands: &'r [SlashCommand<'_>],
    icons: &ChatIcons,
) -> Vec<PaletteItem<'r, Pick>> {
    let mut rows = Vec::new();
    if menu.show_attach {
        rows.push(PaletteItem::new(Pick::Attach, "Attach file or image").icon(icons.attach));
    }
    if menu.mentions && menu.editable {
        rows.push(PaletteItem::new(Pick::Mention, "Mention a session").icon(icons.mention));
    }
    for (i, item) in menu.items.iter().enumerate() {
        let mut row = PaletteItem::new(Pick::Plus(i), item.label.as_ref());
        if let Some(icon) = item.icon {
            row = row.icon(icon);
        }
        if let Some(detail) = &item.detail {
            row = row.detail(detail.as_ref());
        }
        rows.push(row);
    }
    for (i, c) in commands.iter().enumerate() {
        rows.push(
            PaletteItem::new(Pick::Slash(i), c.name.as_ref())
                .icon(c.icon.unwrap_or(icons.slash))
                .detail(c.about.as_ref())
                .section(c.section.as_deref().unwrap_or(COMMANDS_SECTION)),
        );
    }
    rows
}

/// Draws the menu while open and applies a pick.
pub(super) fn show(
    ctx: &Context,
    text_id: Id,
    state: &mut ComposerState,
    menu: Menu<'_, '_>,
    out: &mut Outcome,
) {
    if !state.plus.is_open() {
        return;
    }
    let icons = ChatIcons::get(ctx);
    let commands = listed(menu.slash, menu.editable);
    let list = rows(&menu, &commands, &icons);
    let back = ctx.input(|i| i.key_pressed(Key::BrowserBack));
    let picked = CommandPalette::new()
        .id_salt(menu.id)
        .hint("Search commands")
        .show(ctx, &mut state.plus, &list);
    let Some(pick) = picked else {
        // Back closes the menu without refocusing the text.
        if back && !state.plus.is_open() {
            state.refocus = false;
        }
        return;
    };
    state.plus.close();
    match pick {
        Pick::Attach => out.action = Some(ComposerAction::Attach),
        Pick::Plus(i) => out.action = Some(ComposerAction::Plus(menu.items[i].id.to_string())),
        Pick::Mention => {
            let len = state.char_len();
            let at = cursor_of(ctx, text_id).map_or(len, |(a, b)| a.max(b).min(len));
            let after_space = at == 0
                || state
                    .text()
                    .chars()
                    .nth(at - 1)
                    .is_some_and(char::is_whitespace);
            let cursor = state.replace(at..at, if after_space { "@" } else { " @" });
            state.dismissed = None;
            store_cursor(ctx, text_id, cursor, cursor, Some(state.text()));
        }
        Pick::Slash(i) => {
            let pick = SlashPick::from(&commands[i]);
            if pick.runs_on_pick() {
                out.action = Some(ComposerAction::Command {
                    id: pick.id,
                    args: String::new(),
                });
            } else {
                let cursor = insert_command(state, &pick.name);
                store_cursor(ctx, text_id, cursor, cursor, Some(state.text()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(id: &'static str, args: &'static str, enabled: bool) -> SlashCommand<'static> {
        SlashCommand {
            id: id.into(),
            name: format!("/{id}").into(),
            args: args.into(),
            about: "".into(),
            section: None,
            icon: None,
            enabled,
            run_on_pick: false,
        }
    }

    fn source(_: &str) -> Vec<SlashCommand<'static>> {
        vec![
            command("compact", "", true),
            command("model", "<name>", true),
            command("off", "", false),
        ]
    }

    #[test]
    fn rows_follow_the_menu_order_and_the_text_mode() {
        let items = [PlusItem {
            id: "new".into(),
            label: "New session".into(),
            icon: None,
            detail: None,
        }];
        let mut menu = Menu {
            id: Id::unique("plus-test"),
            show_attach: true,
            editable: true,
            mentions: true,
            items: &items,
            slash: Some(&source),
        };
        let icons = ChatIcons::default();
        let commands = listed(menu.slash, menu.editable);
        let ids: Vec<Pick> = rows(&menu, &commands, &icons)
            .iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(
            ids,
            vec![
                Pick::Attach,
                Pick::Mention,
                Pick::Plus(0),
                Pick::Slash(0),
                Pick::Slash(1)
            ]
        );
        menu.editable = false;
        menu.show_attach = false;
        let commands = listed(menu.slash, menu.editable);
        let ids: Vec<Pick> = rows(&menu, &commands, &icons)
            .iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(ids, vec![Pick::Plus(0), Pick::Slash(0)]);
        assert_eq!(commands[0].id, "compact");
    }
}
