//! Tool arguments, results and card details: JSON trees when JSON, capped monospace text otherwise.

use egui::{Align, Color32, Id, Layout, Ui, text::LayoutJob};
use serde_json::Value;

use super::bubble::{Piece, split_claimed};
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

/// Draws `payload` under `id`; with `claim`, lines of non-JSON text the app claims are drawn by the app for that entry key.
pub(crate) fn show(
    ui: &mut Ui,
    rc: &mut RowCx<'_>,
    payload: &Payload<'_>,
    id: Id,
    part: Part,
    claim: Option<&str>,
) {
    match payload {
        Payload::Json(value) => values(ui, rc, std::slice::from_ref(value.as_ref()), id, part),
        Payload::Text(text) => match cached_json(ui.ctx(), text) {
            Some(parsed) => values(ui, rc, &parsed, id, part),
            None => {
                let ink = rc.p.text;
                match claim {
                    Some(key) => mono_lines(ui, rc, text, ink, id, key),
                    None => mono_text(ui, rc, text, ink, id),
                }
            }
        },
        Payload::Plain(text) => {
            let ink = rc.p.text;
            mono_text(ui, rc, text, ink, id);
        }
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
    mono_column(ui, rc, text, id, |ui, rc| {
        let (shown, left) = capped(text, MONO_MAX_CHARS);
        mono_label(ui, rc, shown, ink);
        if left > 0 {
            more_note(ui, rc, left);
        }
    });
}

/// [`mono_text`] whose lines the app claims are drawn by the app for the entry keyed `key`.
pub(crate) fn mono_lines(
    ui: &mut Ui,
    rc: &mut RowCx<'_>,
    text: &str,
    ink: Color32,
    id: Id,
    key: &str,
) {
    let Some(lines) = rc.agent_lines else {
        return mono_text(ui, rc, text, ink, id);
    };
    let (shown, left) = capped(text, MONO_MAX_CHARS);
    let pieces = split_claimed(shown, |l| lines.claims(l));
    if !pieces.iter().any(|p| matches!(p, Piece::Line(_))) {
        return mono_text(ui, rc, text, ink, id);
    }
    let last = pieces.len() - 1;
    mono_column(ui, rc, text, id, |ui, rc| {
        for (n, piece) in pieces.into_iter().enumerate() {
            match piece {
                Piece::Text(run) if n < last => {
                    // Line ending before a claimed line.
                    let run = run.strip_suffix('\n').unwrap_or(run);
                    mono_label(ui, rc, run.strip_suffix('\r').unwrap_or(run), ink);
                }
                Piece::Text(run) => mono_label(ui, rc, run, ink),
                Piece::Line(line) => {
                    ui.push_id(n, |ui| lines.show(ui, key, line));
                }
            }
        }
        if left > 0 {
            more_note(ui, rc, left);
        }
    });
}

/// A copy button for all of `text` beside a column that `body` fills.
fn mono_column(
    ui: &mut Ui,
    rc: &mut RowCx<'_>,
    text: &str,
    id: Id,
    body: impl FnOnce(&mut Ui, &mut RowCx<'_>),
) {
    ui.push_id(id, |ui| {
        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
            let copy = IconButton::new(rc.icons.copy, IconBox::Sm)
                .tip("Copy")
                .show(ui);
            if rc.tapped(&copy) {
                rc.copy(ui, text.to_owned());
            }
            ui.with_layout(Layout::top_down(Align::Min), |ui| body(ui, rc));
        });
    });
}

/// `text` as one wrapped monospace label.
fn mono_label(ui: &mut Ui, rc: &RowCx<'_>, text: &str, ink: Color32) {
    let mut job = LayoutJob::simple(text.to_owned(), rc.f.mono.clone(), ink, wrap_width(ui));
    job.wrap.break_anywhere = true;
    let galley = ui.ctx().fonts_mut(|f| f.layout_job(job));
    ui.add(egui::Label::new(galley).selectable(rc.selectable));
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
                    None,
                );
                show(
                    ui,
                    &mut rc,
                    &Payload::Text(Cow::Borrowed("{\"ok\": true}")),
                    id.with(1),
                    Part::Result,
                    None,
                );
                let long = "x".repeat(MONO_MAX_CHARS + 10);
                show(
                    ui,
                    &mut rc,
                    &Payload::Text(Cow::Owned(long)),
                    id.with(2),
                    Part::Result,
                    None,
                );
                assert!(rc.actions.is_empty());
            });
            out.textures_delta.clear();
        }
    }

    #[test]
    fn plain_text_that_reads_as_json_draws_as_written() {
        let ctx = egui::Context::default();
        let text = "{\"steer\": [1, 2]}";
        let mut drawn = false;
        for _ in 0..2 {
            let mut out = ctx.run_ui(Default::default(), |ui| {
                let mut rc = RowCx::new(ui, Id::unique("plain-test"));
                let id = rc.id;
                let payload = Payload::Plain(Cow::Borrowed(text));
                show(ui, &mut rc, &payload, id, Part::Detail, None);
            });
            out.textures_delta.clear();
            drawn = out.shapes.iter().any(|c| match &c.shape {
                egui::Shape::Text(t) => t.galley.text() == text,
                _ => false,
            });
        }
        assert!(drawn, "the text is drawn whole, not as a tree");
    }
}
