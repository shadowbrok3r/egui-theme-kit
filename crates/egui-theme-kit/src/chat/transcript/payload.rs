//! Tool arguments, results and card details: JSON trees when JSON, capped monospace text otherwise.

use egui::{Align, Color32, Id, Layout, Ui, text::LayoutJob};
use serde_json::Value;

use super::entry::Payload;
use super::markdown::{cached_json, capped, more_note, wrap_width};
use super::row::RowCx;
use crate::controls::{IconBox, IconButton};

/// Longest monospace payload laid out.
pub(crate) const MONO_MAX_CHARS: usize = 16_000;

/// Which part of a call a payload is, which picks how deep it opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Part {
    Arguments,
    Result,
    Detail,
}

#[cfg(feature = "chat-json")]
impl Part {
    fn role(self) -> super::json::PayloadRole {
        match self {
            Self::Arguments | Self::Detail => super::json::PayloadRole::Arguments,
            Self::Result => super::json::PayloadRole::Result,
        }
    }
}

/// Draws `payload` under `id`.
pub(crate) fn show(ui: &mut Ui, rc: &mut RowCx<'_>, payload: &Payload<'_>, id: Id, part: Part) {
    match payload {
        Payload::Json(value) => values(ui, rc, std::slice::from_ref(value.as_ref()), id, part),
        Payload::Text(text) => match cached_json(ui.ctx(), text) {
            Some(parsed) => values(ui, rc, &parsed, id, part),
            None => {
                let ink = rc.p.text;
                mono_text(ui, rc, text, ink, id);
            }
        },
    }
}

/// Draws each of `values` as a tree, keyed `id` and its index.
#[cfg(feature = "chat-json")]
pub(crate) fn values(ui: &mut Ui, rc: &mut RowCx<'_>, values: &[Value], id: Id, part: Part) {
    for (n, value) in values.iter().enumerate() {
        let out = super::json::PayloadTree::new(id.with(n), value, part.role())
            .touch(rc.touch)
            .show(ui);
        if let Some(text) = out.copied {
            rc.actions
                .push(crate::chat::types::ChatAction::Copied(text));
        }
    }
}

/// Draws each of `values` as pretty monospace text.
#[cfg(not(feature = "chat-json"))]
pub(crate) fn values(ui: &mut Ui, rc: &mut RowCx<'_>, values: &[Value], id: Id, _part: Part) {
    for (n, value) in values.iter().enumerate() {
        let text = serde_json::to_string_pretty(value).unwrap_or_default();
        let ink = rc.p.text;
        mono_text(ui, rc, &text, ink, id.with(n));
    }
}

/// Monospace text with a copy button, capped at [`MONO_MAX_CHARS`].
pub(crate) fn mono_text(ui: &mut Ui, rc: &mut RowCx<'_>, text: &str, ink: Color32, id: Id) {
    ui.push_id(id, |ui| {
        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
            let copy = IconButton::new(rc.icons.copy, IconBox::Sm)
                .tip("Copy")
                .show(ui);
            if rc.tapped(&copy) {
                rc.copy(ui, text.to_owned());
            }
            ui.with_layout(Layout::top_down(Align::Min), |ui| {
                let (shown, left) = capped(text, MONO_MAX_CHARS);
                let mut job =
                    LayoutJob::simple(shown.to_owned(), rc.f.mono.clone(), ink, wrap_width(ui));
                job.wrap.break_anywhere = true;
                let galley = ui.ctx().fonts_mut(|f| f.layout_job(job));
                ui.add(egui::Label::new(galley).selectable(rc.selectable));
                if left > 0 {
                    more_note(ui, rc, left);
                }
            });
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    #[test]
    fn text_json_and_plain_payloads_draw() {
        let ctx = egui::Context::default();
        let value = serde_json::json!({ "cmd": "ls -la", "n": [1, 2] });
        for _ in 0..2 {
            let mut out = ctx.run_ui(Default::default(), |ui| {
                let mut rc = RowCx::new(ui, Id::unique("payload-test"));
                let id = rc.id;
                show(
                    ui,
                    &mut rc,
                    &Payload::Json(Cow::Borrowed(&value)),
                    id.with(0),
                    Part::Arguments,
                );
                show(
                    ui,
                    &mut rc,
                    &Payload::Text(Cow::Borrowed("{\"ok\": true}")),
                    id.with(1),
                    Part::Result,
                );
                let long = "x".repeat(MONO_MAX_CHARS + 10);
                show(
                    ui,
                    &mut rc,
                    &Payload::Text(Cow::Owned(long)),
                    id.with(2),
                    Part::Result,
                );
                assert!(rc.actions.is_empty());
            });
            out.textures_delta.clear();
        }
    }
}
