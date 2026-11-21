//! The icon set chat widgets draw, replaceable per context.

use egui::{Context, Id};

use crate::controls::{Icon, icons};

/// Icons chat widgets draw; defaults to the kit's painted icons.
#[derive(Clone, Copy, Debug)]
pub struct ChatIcons {
    pub plus: Icon,
    pub attach: Icon,
    pub send: Icon,
    pub queue: Icon,
    pub steer: Icon,
    pub stop: Icon,
    pub mention: Icon,
    pub slash: Icon,
    pub copy: Icon,
    pub cut: Icon,
    pub paste: Icon,
    pub select_all: Icon,
    pub expand: Icon,
    pub collapse: Icon,
    pub reasoning: Icon,
    pub tool: Icon,
    pub command: Icon,
    pub file: Icon,
    pub approval: Icon,
    pub question: Icon,
    pub chevron_up: Icon,
    pub chevron_down: Icon,
    pub chevron_right: Icon,
    pub error: Icon,
    pub check: Icon,
    pub close: Icon,
    pub more: Icon,
}

impl Default for ChatIcons {
    fn default() -> Self {
        Self {
            plus: icons::PLUS,
            attach: icons::PAPERCLIP,
            send: icons::SEND,
            queue: icons::QUEUE,
            steer: icons::LIGHTNING,
            stop: icons::STOP,
            mention: icons::AT,
            slash: icons::SLASH,
            copy: icons::COPY,
            cut: icons::CUT,
            paste: icons::PASTE,
            select_all: icons::SELECT_ALL,
            expand: icons::EXPAND,
            collapse: icons::COLLAPSE,
            reasoning: icons::BRAIN,
            tool: icons::WRENCH,
            command: icons::TERMINAL,
            file: icons::FILE_DIFF,
            approval: icons::SHIELD,
            question: icons::QUESTION,
            chevron_up: icons::CHEVRON_UP,
            chevron_down: icons::CHEVRON_DOWN,
            chevron_right: icons::CARET_RIGHT,
            error: icons::ERROR,
            check: icons::CHECK,
            close: icons::X,
            more: icons::DOTS,
        }
    }
}

fn icons_id() -> Id {
    Id::unique("egui_theme_kit.chat.icons")
}

impl ChatIcons {
    /// The icons published on `ctx`, or the defaults.
    pub fn get(ctx: &Context) -> Self {
        ctx.data(|d| d.get_temp::<Self>(icons_id()))
            .unwrap_or_default()
    }

    pub fn publish(ctx: &Context, icons: Self) {
        ctx.data_mut(|d| d.insert_temp(icons_id(), icons));
    }
}
