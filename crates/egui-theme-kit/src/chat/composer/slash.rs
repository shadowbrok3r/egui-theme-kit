//! Slash commands and the app entries of the `+` menu.

use crate::chat::types::CowStr;
use crate::controls::Icon;

/// Supplies the slash commands matching a query.
pub trait SlashSource {
    /// Commands for `query`, the text after `/`.
    fn commands(&self, query: &str) -> Vec<SlashCommand<'_>>;
}

impl<F: Fn(&str) -> Vec<SlashCommand<'static>>> SlashSource for F {
    fn commands(&self, query: &str) -> Vec<SlashCommand<'_>> {
        self(query)
    }
}

/// One slash command.
#[derive(Clone, Debug)]
pub struct SlashCommand<'a> {
    pub id: CowStr<'a>,
    /// Name with its slash, such as `/compact`.
    pub name: CowStr<'a>,
    /// Argument hint; empty when the command takes none.
    pub args: CowStr<'a>,
    pub about: CowStr<'a>,
    pub section: Option<CowStr<'a>>,
    pub icon: Option<Icon>,
    pub enabled: bool,
    /// Runs when picked even though it takes arguments.
    pub run_on_pick: bool,
}

/// An app entry of the `+` menu, such as New session.
#[derive(Clone, Debug)]
pub struct PlusItem<'a> {
    pub id: CowStr<'a>,
    pub label: CowStr<'a>,
    pub icon: Option<Icon>,
    pub detail: Option<CowStr<'a>>,
}
