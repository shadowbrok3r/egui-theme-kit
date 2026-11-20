//! Wrapped text runs with clickable links and mention chips, and glyph fallbacks for missing symbols.

use std::borrow::Cow;
use std::ops::Range;

use egui::{
    Context, CursorIcon, FontId, Galley, Label, Response, Sense, Stroke, TextFormat, Ui, Vec2,
    text::{CCursor, CharIndex, LayoutJob},
};

use super::row::RowCx;
use crate::chat::mention::MentionRef;
use crate::chat::types::ChatAction;

/// What a clickable run opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Hit {
    Link(String),
    Mention(MentionRef),
}

struct Target {
    chars: Range<usize>,
    hit: Option<Hit>,
    tip: Option<String>,
}

/// A wrapped layout job that remembers which characters are links and mentions.
pub(crate) struct Rich {
    job: LayoutJob,
    targets: Vec<Target>,
    chars: usize,
}

impl Rich {
    pub fn new(max_width: f32) -> Self {
        let mut job = LayoutJob::default();
        job.wrap.max_width = max_width.max(1.0);
        Self {
            job,
            targets: Vec::new(),
            chars: 0,
        }
    }

    /// Appends `text`; symbols the font lacks are replaced.
    pub fn text(&mut self, ctx: &Context, text: &str, format: TextFormat) {
        let text = renderable(ctx, &format.font_id, text);
        self.chars += text.chars().count();
        self.job.append(&text, 0.0, format);
    }

    /// Appends `text` that opens `hit` when clicked, or shows `tip` on hover.
    pub fn target(
        &mut self,
        ctx: &Context,
        text: &str,
        format: TextFormat,
        hit: Option<Hit>,
        tip: Option<String>,
    ) {
        let start = self.chars;
        self.text(ctx, text, format);
        self.targets.push(Target {
            chars: start..self.chars,
            hit,
            tip,
        });
    }

    /// Appends a mention chip of `m` in `font`.
    pub fn mention(&mut self, ui: &Ui, rc: &RowCx<'_>, m: &MentionRef, font: &FontId, user: bool) {
        let view = rc.mention(ui, m, user);
        let format = if view.known {
            TextFormat {
                background: rc.p.mention_fill,
                ..TextFormat::simple(font.clone(), rc.p.mention_text)
            }
        } else {
            TextFormat {
                strikethrough: Stroke::new(1.0, rc.p.text_faint),
                ..TextFormat::simple(font.clone(), rc.p.text_faint)
            }
        };
        let hit = view.clickable.then(|| {
            Hit::Mention(MentionRef {
                label: view.label.clone(),
                ..m.clone()
            })
        });
        let tip = (!view.known).then(|| "Not found".to_owned());
        self.target(ui.ctx(), &format!("@{}", view.label), format, hit, tip);
    }

    /// Lays the text out; the galley is not yet placed.
    pub fn layout(self, ctx: &Context) -> (std::sync::Arc<Galley>, RichTargets) {
        let galley = ctx.fonts_mut(|f| f.layout_job(self.job));
        (
            galley,
            RichTargets {
                targets: self.targets,
            },
        )
    }

    /// Adds the text as a label and reports clicked targets to `rc`.
    pub fn show(self, ui: &mut Ui, rc: &mut RowCx<'_>) -> Response {
        let (galley, targets) = self.layout(ui.ctx());
        targets.show(ui, rc, galley)
    }
}

/// The clickable runs of a laid-out [`Rich`].
pub(crate) struct RichTargets {
    targets: Vec<Target>,
}

impl RichTargets {
    /// Adds `galley` as a label and handles hover and clicks on its targets.
    pub fn show(self, ui: &mut Ui, rc: &mut RowCx<'_>, galley: std::sync::Arc<Galley>) -> Response {
        let sense = if self.targets.is_empty() {
            Sense::hover()
        } else {
            Sense::click()
        };
        let response = ui.add(
            Label::new(galley.clone())
                .selectable(rc.selectable)
                .sense(sense),
        );
        self.handle(ui, rc, &galley, &response);
        response
    }

    /// Handles hover and clicks on targets of `galley` drawn at `response.rect.min`.
    pub fn handle(&self, ui: &Ui, rc: &mut RowCx<'_>, galley: &Galley, response: &Response) {
        if self.targets.is_empty() {
            return;
        }
        let at = |pos: egui::Pos2| {
            char_at(galley, pos - response.rect.min)
                .and_then(|c| self.targets.iter().find(|t| t.chars.contains(&c)))
        };
        if let Some(target) = response.hover_pos().and_then(at) {
            if target.hit.is_some() {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            if let Some(tip) = &target.tip {
                response.clone().on_hover_text_at_pointer(tip.as_str());
            }
        }
        if rc.tapped(response)
            && let Some(Target { hit: Some(hit), .. }) =
                response.interact_pointer_pos().and_then(at)
        {
            rc.actions.push(match hit.clone() {
                Hit::Link(url) => ChatAction::Link(url),
                Hit::Mention(m) => ChatAction::OpenMention(m),
            });
        }
    }
}

/// Wrapped plain text in `font` and `color`; selectable when `rc` allows it.
pub(crate) fn plain(
    ui: &mut Ui,
    rc: &RowCx<'_>,
    text: &str,
    font: &FontId,
    color: egui::Color32,
) -> Response {
    let text = renderable(ui.ctx(), font, text);
    let job = LayoutJob::simple(
        text.into_owned(),
        font.clone(),
        color,
        super::markdown::wrap_width(ui),
    );
    let galley = ui.ctx().fonts_mut(|f| f.layout_job(job));
    ui.add(Label::new(galley).selectable(rc.selectable))
}

/// One row of `text` no wider than `max_w`, cut with an ellipsis.
pub(crate) fn one_row(
    ui: &Ui,
    text: &str,
    font: &FontId,
    color: egui::Color32,
    max_w: f32,
) -> std::sync::Arc<Galley> {
    let text = renderable(ui.ctx(), font, text);
    let mut job = LayoutJob::simple_singleline(text.into_owned(), font.clone(), color);
    job.wrap.max_width = max_w.max(1.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('\u{2026}');
    ui.ctx().fonts_mut(|f| f.layout_job(job))
}

/// Index of the character under `local`, a point relative to the galley origin.
pub(crate) fn char_at(galley: &Galley, local: Vec2) -> Option<usize> {
    let total = galley.text().chars().count();
    let near = galley.cursor_from_pos(local).index.0;
    [near.saturating_sub(1), near].into_iter().find(|&k| {
        if k >= total {
            return false;
        }
        let a = galley.pos_from_cursor(CCursor::new(CharIndex(k)));
        let b = galley.pos_from_cursor(CCursor::new(CharIndex(k + 1)));
        let same_row = (a.min.y - b.min.y).abs() < 0.5;
        let right = if same_row {
            b.min.x
        } else {
            galley.rect.right()
        };
        (a.min.x..=right).contains(&local.x) && (a.min.y..=a.max.y).contains(&local.y)
    })
}

/// `text` with symbols `font` lacks replaced, variation selectors dropped and full-width ASCII narrowed.
pub(crate) fn renderable<'t>(ctx: &Context, font: &FontId, text: &'t str) -> Cow<'t, str> {
    if text.chars().all(|c| (c as u32) < 0x2000) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    ctx.fonts_mut(|f| {
        for c in text.chars() {
            let code = c as u32;
            if code < 0x2000 {
                out.push(c);
                continue;
            }
            if matches!(code, 0xFE00..=0xFE0F | 0x200D) {
                continue;
            }
            if (0xFF01..=0xFF5E).contains(&code) {
                out.extend(char::from_u32(code - 0xFEE0));
                continue;
            }
            if f.has_glyph(font, c) {
                out.push(c);
                continue;
            }
            match fallback(c) {
                Some(s) => out.push_str(s),
                None if is_pictograph(code) => {}
                None => out.push(c),
            }
        }
    });
    Cow::Owned(out)
}

/// Plain text for common symbols a font may lack.
fn fallback(c: char) -> Option<&'static str> {
    Some(match c {
        '\u{2705}' | '\u{2611}' | '\u{2713}' | '\u{2714}' => "[x]",
        '\u{274C}' | '\u{2716}' | '\u{2717}' | '\u{2718}' => "x",
        '\u{26A0}' | '\u{2757}' => "!",
        '\u{2753}' | '\u{2754}' => "?",
        '\u{2192}' | '\u{27A1}' | '\u{2794}' => "->",
        '\u{2190}' | '\u{2B05}' => "<-",
        '\u{2191}' | '\u{2B06}' => "^",
        '\u{2193}' | '\u{2B07}' => "v",
        '\u{21D2}' => "=>",
        '\u{2022}' | '\u{25CF}' | '\u{25AA}' => "-",
        '\u{2026}' => "...",
        '\u{2014}' => "--",
        '\u{2013}' => "-",
        '\u{2018}' | '\u{2019}' => "'",
        '\u{201C}' | '\u{201D}' => "\"",
        '\u{00A0}' | '\u{2009}' | '\u{202F}' => " ",
        _ => return None,
    })
}

/// Emoji and pictograph ranges dropped when the font lacks them.
fn is_pictograph(code: u32) -> bool {
    matches!(code, 0x2600..=0x27BF | 0x2B00..=0x2BFF | 0x1F000..=0x1FAFF)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Color32, FontId};

    #[test]
    fn symbols_without_glyphs_fall_back_to_text() {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(Default::default(), |ui| {
            let font = FontId::proportional(12.0);
            assert!(matches!(
                renderable(ui.ctx(), &font, "plain"),
                Cow::Borrowed(_)
            ));
            let text = "\u{2705} done \u{FF21}\u{FE0F} \u{1F600}";
            let shown = renderable(ui.ctx(), &font, text);
            assert!(shown.contains("done A"), "{shown}");
            assert!(!shown.contains('\u{FE0F}'));
        });
        out.textures_delta.clear();
    }

    #[test]
    fn a_point_maps_to_the_character_under_it() {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(Default::default(), |ui| {
            let job = LayoutJob::simple_singleline(
                "abcd".to_owned(),
                FontId::monospace(10.0),
                Color32::PLACEHOLDER,
            );
            let galley = ui.ctx().fonts_mut(|f| f.layout_job(job));
            let w = galley.rect.width() / 4.0;
            let y = galley.rect.center().y;
            assert_eq!(char_at(&galley, egui::vec2(w * 0.4, y)), Some(0));
            assert_eq!(char_at(&galley, egui::vec2(w * 2.6, y)), Some(2));
            assert_eq!(char_at(&galley, egui::vec2(w * 3.9, y)), Some(3));
            assert_eq!(char_at(&galley, egui::vec2(w * 9.0, y)), None);
        });
        out.textures_delta.clear();
    }
}
