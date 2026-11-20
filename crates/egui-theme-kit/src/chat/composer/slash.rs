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

impl From<&SlashCommand<'_>> for super::popup::SlashPick {
    fn from(c: &SlashCommand<'_>) -> Self {
        Self {
            id: c.id.to_string(),
            name: c.name.trim_start_matches('/').to_owned(),
            args: c.args.to_string(),
            about: c.about.to_string(),
            icon: c.icon,
            enabled: c.enabled,
            run_on_pick: c.run_on_pick,
        }
    }
}

/// A known command typed as `/name args`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Parsed {
    Run {
        id: String,
        args: String,
    },
    /// The command exists but is disabled; it is neither run nor sent.
    Disabled,
}

/// The command `wire` invokes as `/name args`; unknown names give `None`.
pub(super) fn parse_command(source: &dyn SlashSource, wire: &str) -> Option<Parsed> {
    let rest = wire.strip_prefix('/')?;
    let (name, args) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
    if name.is_empty() {
        return None;
    }
    let commands = source.commands(name);
    let c = commands
        .iter()
        .filter(|c| c.name.trim_start_matches('/') == name || c.id == name)
        .max_by_key(|c| c.enabled)?;
    Some(if c.enabled {
        Parsed::Run {
            id: c.id.to_string(),
            args: args.trim().to_owned(),
        }
    } else {
        Parsed::Disabled
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commands(_: &str) -> Vec<SlashCommand<'static>> {
        let base = SlashCommand {
            id: "compact".into(),
            name: "/compact".into(),
            args: "".into(),
            about: "".into(),
            section: None,
            icon: None,
            enabled: true,
            run_on_pick: false,
        };
        vec![
            base.clone(),
            SlashCommand {
                id: "model.set".into(),
                name: "/model".into(),
                args: "<name>".into(),
                ..base.clone()
            },
            SlashCommand {
                id: "off".into(),
                name: "/off".into(),
                enabled: false,
                ..base
            },
        ]
    }

    #[test]
    fn known_commands_parse_and_unknown_stay_text() {
        let run = |id: &str, args: &str| {
            Some(Parsed::Run {
                id: id.into(),
                args: args.into(),
            })
        };
        assert_eq!(parse_command(&commands, "/compact"), run("compact", ""));
        assert_eq!(
            parse_command(&commands, "/model  gpt x\n"),
            run("model.set", "gpt x")
        );
        assert_eq!(parse_command(&commands, "/foo bar"), None);
        assert_eq!(parse_command(&commands, "/off now"), Some(Parsed::Disabled));
        assert_eq!(parse_command(&commands, "compact"), None);
        assert_eq!(parse_command(&commands, "/"), None);
    }
}
