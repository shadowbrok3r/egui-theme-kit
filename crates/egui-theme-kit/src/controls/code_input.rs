//! Segmented one-time-code entry drawn over one hidden text editor.

use egui::{
    AsIdSalt, Color32, CursorIcon, Event, Frame, Id, IdSalt, ImeEvent, Rect, Response, Sense,
    Stroke, StrokeKind, TextEdit, TextStyle, Ui, UiBuilder,
    class::HasClasses as _,
    pos2,
    text::{CCursor, CCursorRange},
    vec2,
};

use super::{State, kit, mix, preview_focused, semibold_font_sized, state_of, text_galley};
use crate::widget_theme;

/// Default number of cells.
pub const CODE_LEN: usize = 6;
/// Gap between two cells.
const CELL_GAP: f32 = 6.0;
/// Gap between two groups of cells, around the painted dash.
const GROUP_GAP: f32 = 18.0;
/// Width of the dash between groups.
const DASH_W: f32 = 8.0;
/// Extra cell height over a prominent button.
const CELL_EXTRA_H: f32 = 10.0;
/// Cell width as a share of its height.
const CELL_ASPECT: f32 = 0.84;
/// Narrowest a cell gets when the row is squeezed.
const MIN_CELL_W: f32 = 18.0;
/// Code character size as a share of the cell height.
const CHAR_SHARE: f32 = 0.5;
/// Caret height as a share of the cell height.
const CARET_SHARE: f32 = 0.46;
/// Radius of a masked character's dot.
const MASK_DOT: f32 = 4.0;
/// Share of the text colour in a filled cell's border.
const FILLED_EDGE: f32 = 0.22;

/// Characters a [`CodeInput`] accepts; input is uppercased before it is checked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CodeCharset {
    /// 0 to 9.
    #[default]
    Digits,
    /// A to Z and 0 to 9.
    Alphanumeric,
    /// A to Z and 2 to 9 without the look-alikes 0, O, 1, I and L.
    Unambiguous,
}

impl CodeCharset {
    /// `c` uppercased when this set accepts it.
    pub fn accept(self, c: char) -> Option<char> {
        let c = c.to_ascii_uppercase();
        let ok = match self {
            Self::Digits => c.is_ascii_digit(),
            Self::Alphanumeric => c.is_ascii_uppercase() || c.is_ascii_digit(),
            Self::Unambiguous => {
                (c.is_ascii_uppercase() || c.is_ascii_digit())
                    && !matches!(c, '0' | 'O' | '1' | 'I' | 'L')
            }
        };
        ok.then_some(c)
    }

    /// The accepted characters of `text`, uppercased, at most `len` of them.
    pub fn filter(self, text: &str, len: usize) -> String {
        text.chars()
            .filter_map(|c| self.accept(c))
            .take(len)
            .collect()
    }
}

/// What a [`CodeInput`] reported this frame.
#[derive(Clone, Debug)]
pub struct CodeInputResponse {
    /// The editor's response; `rect` covers every cell.
    pub response: Response,
    /// The code changed this frame.
    pub changed: bool,
    /// The last cell filled this frame.
    pub completed: bool,
    /// Every cell is filled.
    pub full: bool,
}

/// A row of single-character cells for a one-time or pairing code.
#[must_use = "call `show` to draw the code input"]
pub struct CodeInput<'a> {
    code: &'a mut String,
    length: usize,
    group: Option<usize>,
    charset: CodeCharset,
    salt: Option<IdSalt>,
    cell_width: Option<f32>,
    invalid: bool,
    masked: bool,
    enabled: bool,
}

impl<'a> CodeInput<'a> {
    /// Edits `code`, which holds only the accepted characters, uppercased.
    pub fn new(code: &'a mut String) -> Self {
        Self {
            code,
            length: CODE_LEN,
            group: None,
            charset: CodeCharset::Digits,
            salt: None,
            cell_width: None,
            invalid: false,
            masked: false,
            enabled: true,
        }
    }

    /// Number of cells.
    pub fn length(mut self, length: usize) -> Self {
        self.length = length.max(1);
        self
    }

    /// A dash between every `size` cells, such as 4 for a 4-4 code.
    pub fn group(mut self, size: usize) -> Self {
        self.group = (size > 0).then_some(size);
        self
    }

    pub fn charset(mut self, charset: CodeCharset) -> Self {
        self.charset = charset;
        self
    }

    /// Stable id salt for the editor's focus and state.
    pub fn id_salt(mut self, salt: impl AsIdSalt) -> Self {
        self.salt = Some(IdSalt::new(salt));
        self
    }

    /// Cell width in place of one derived from the cell height.
    pub fn cell_width(mut self, width: f32) -> Self {
        self.cell_width = Some(width);
        self
    }

    /// Error-tinted cell borders.
    pub fn invalid(mut self, invalid: bool) -> Self {
        self.invalid = invalid;
        self
    }

    /// Dots in place of the characters.
    pub fn masked(mut self, masked: bool) -> Self {
        self.masked = masked;
        self
    }

    /// Draws the cells disabled and ignores input when `false`.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Height of a cell at the current density.
    pub fn cell_height(ui: &Ui) -> f32 {
        kit(ui).1.button_h + CELL_EXTRA_H
    }

    /// Left edge offsets of each cell and the row width, for cells `cell_w` wide.
    fn layout(&self, cell_w: f32) -> (Vec<f32>, f32) {
        let mut x = 0.0;
        let mut lefts = Vec::with_capacity(self.length);
        for i in 0..self.length {
            if i > 0 {
                x += if self.group.is_some_and(|g| i % g == 0) {
                    GROUP_GAP
                } else {
                    CELL_GAP
                };
            }
            lefts.push(x);
            x += cell_w;
        }
        (lefts, x)
    }

    /// Width of the gaps between cells.
    fn gaps(&self) -> f32 {
        self.layout(0.0).1
    }

    pub fn show(self, ui: &mut Ui) -> CodeInputResponse {
        if !self.enabled {
            return ui
                .add_enabled_ui(false, |ui| self.enabled(true).show(ui))
                .inner;
        }
        let (t, _) = kit(ui);
        let height = Self::cell_height(ui);
        let wanted = self.cell_width.unwrap_or((height * CELL_ASPECT).round());
        let room = (ui.available_width() - self.gaps()) / self.length as f32;
        let cell_w = wanted.min(room.floor()).max(MIN_CELL_W);
        let (lefts, width) = self.layout(cell_w);
        let id = match &self.salt {
            Some(salt) => ui.make_persistent_id(salt),
            None => ui.next_auto_id(),
        };
        ui.skip_ahead_auto_ids(1);
        let (rect, frame) = ui.allocate_exact_size(vec2(width, height), Sense::CLICK);

        let before = self.charset.filter(self.code, self.length);
        let mut changed = before != *self.code;
        if changed {
            *self.code = before.clone();
        }
        let was_full = before.chars().count() == self.length;
        let focused_before = ui.memory(|m| m.has_focus(id));
        let pasted = focused_before
            .then(|| {
                ui.input(|i| {
                    i.events.iter().rev().find_map(|e| match e {
                        Event::Paste(text)
                        | Event::Text(text)
                        | Event::Ime(ImeEvent::Commit(text)) => {
                            let code = self.charset.filter(text, self.length);
                            (code.chars().count() == self.length).then_some(code)
                        }
                        _ => None,
                    })
                })
            })
            .flatten();

        let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
        let visuals = child.visuals_mut();
        visuals.text_cursor.stroke = Stroke::NONE;
        visuals.text_cursor.blink = false;
        visuals.selection.bg_fill = Color32::TRANSPARENT;
        visuals.selection.stroke = Stroke::NONE;
        let edit = TextEdit::singleline(self.code)
            .with_class(widget_theme::STOCK)
            .id(id)
            .frame(Frame::NONE)
            .background_color(Color32::TRANSPARENT)
            .text_color(Color32::TRANSPARENT)
            .font(TextStyle::Body)
            .clip_text(true)
            .desired_width(rect.width())
            .min_size(rect.size());
        let mut response = child.add(edit);
        if frame.clicked() {
            response.request_focus();
        }

        let mut code = self.charset.filter(self.code, self.length);
        if let Some(whole_code) = pasted {
            code = whole_code;
        }
        if code != *self.code {
            *self.code = code;
        }
        changed |= *self.code != before;
        if response.has_focus() || focused_before {
            let end = self.code.chars().count();
            if let Some(mut state) = TextEdit::load_state(ui.ctx(), id) {
                state
                    .cursor
                    .set_char_range(Some(CCursorRange::one(CCursor::new(end))));
                state.store(ui.ctx(), id);
            }
        }
        if changed {
            response.mark_changed();
        }
        response = response.union(frame).on_hover_cursor(CursorIcon::Text);

        let full = self.code.chars().count() == self.length;
        if ui.is_rect_visible(rect) {
            let focused = preview_focused(ui) || response.has_focus();
            let (state, _) = state_of(ui, &response);
            let chars: Vec<char> = self.code.chars().collect();
            let active = focused.then(|| chars.len().min(self.length - 1));
            let font = semibold_font_sized(ui, (height * CHAR_SHARE).round());
            for (i, left) in lefts.iter().enumerate() {
                let cell =
                    Rect::from_min_size(pos2(rect.left() + left, rect.top()), vec2(cell_w, height));
                let filled = i < chars.len();
                let is_active = active == Some(i);
                let (edge, width) = if !ui.is_enabled() {
                    (t.field_border, 1.0)
                } else if self.invalid {
                    (t.error, if is_active { 2.0 } else { 1.0 })
                } else if is_active {
                    (t.primary, 2.0)
                } else if filled {
                    (mix(t.field_border, t.text, FILLED_EDGE), 1.0)
                } else if state != State::Rest {
                    (mix(t.field_border, t.text, FILLED_EDGE / 2.0), 1.0)
                } else {
                    (t.field_border, 1.0)
                };
                ui.painter().rect(
                    cell,
                    t.radius_sm,
                    t.field,
                    Stroke::new(width, edge),
                    StrokeKind::Inside,
                );
                let ink = if ui.is_enabled() {
                    t.text
                } else {
                    t.text_faint
                };
                if let Some(c) = chars.get(i) {
                    if self.masked {
                        ui.painter().circle_filled(cell.center(), MASK_DOT, ink);
                    } else {
                        let galley = text_galley(ui, &c.to_string(), font.clone());
                        let pos = (cell.center() - galley.size() / 2.0).round();
                        ui.painter().galley(pos, galley, ink);
                    }
                } else if is_active {
                    let x = ui.painter().round_to_pixel_center(cell.center().x);
                    let half = height * CARET_SHARE / 2.0;
                    ui.painter().vline(
                        x,
                        (cell.center().y - half)..=(cell.center().y + half),
                        Stroke::new(2.0, t.primary),
                    );
                }
                let next = lefts.get(i + 1).copied();
                if let Some(next) = next
                    && next - (left + cell_w) > CELL_GAP
                {
                    let mid = rect.left() + (left + cell_w + next) / 2.0;
                    let y = ui.painter().round_to_pixel_center(rect.center().y);
                    ui.painter().hline(
                        (mid - DASH_W / 2.0)..=(mid + DASH_W / 2.0),
                        y,
                        Stroke::new(2.0, t.text_faint),
                    );
                }
            }
        }
        CodeInputResponse {
            response,
            changed,
            completed: changed && full && !was_full,
            full,
        }
    }
}

/// A code input of `length` digit cells.
pub fn code_input(ui: &mut Ui, code: &mut String, length: usize) -> CodeInputResponse {
    CodeInput::new(code).length(length).show(ui)
}

/// The id a code input built with `salt` in `ui` stores its focus under.
pub fn code_input_id(ui: &Ui, salt: impl AsIdSalt) -> Id {
    ui.make_persistent_id(IdSalt::new(salt))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charsets_uppercase_and_drop_what_they_refuse() {
        assert_eq!(CodeCharset::Digits.filter("12a-34 5", 8), "12345");
        assert_eq!(CodeCharset::Alphanumeric.filter("ab-c1", 8), "ABC1");
        assert_eq!(CodeCharset::Unambiguous.filter("o0l1iAbz9", 8), "ABZ9");
        assert_eq!(CodeCharset::Digits.filter("123456789", 6), "123456");
    }

    #[test]
    fn groups_widen_the_gap_between_them() {
        let mut code = String::new();
        let plain = CodeInput::new(&mut code).length(8).layout(30.0);
        let mut code = String::new();
        let grouped = CodeInput::new(&mut code).length(8).group(4).layout(30.0);
        assert_eq!(plain.0.len(), 8);
        assert_eq!(grouped.1 - plain.1, GROUP_GAP - CELL_GAP);
        assert_eq!(grouped.0[4] - grouped.0[3], 30.0 + GROUP_GAP);
    }
}
