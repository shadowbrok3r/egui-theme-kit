//! Step indicators: a horizontal stepper and vertical timeline rows.

use egui::{
    Color32, Painter, Rect, Response, Role, Sense, Stroke, TextStyle, Ui, Vec2, Widget, WidgetInfo,
    pos2, text::LayoutJob, vec2,
};

use super::labels::paint_small_spinner;
use super::{
    Icon, State, Tokens, elided, focus_ring, icons, ink_on, kit, medium_font_sized,
    semibold_font_sized, state_of, text_galley,
};

/// Diameter of a stepper node.
const NODE: f32 = 26.0;
/// Size of the glyph inside a stepper node.
const NODE_GLYPH: f32 = 14.0;
/// Number size inside a stepper node.
const NODE_PT: f32 = 12.0;
/// Gap between a node and its label.
const NODE_GAP: f32 = 8.0;
/// Inset of a step's content from its cell.
const CELL_PAD: f32 = 8.0;
/// Narrowest label a cell shows before the stepper drops to nodes only.
const MIN_LABEL_W: f32 = 44.0;
/// Shortest connector drawn between two steps.
const MIN_LINK_W: f32 = 10.0;
/// Width of the connector between steps.
const LINK_W: f32 = 2.0;
/// Step label size.
const LABEL_PT: f32 = 12.5;
/// Diameter of a timeline node.
const DOT: f32 = 18.0;
/// Size of the glyph inside a timeline node.
const DOT_GLYPH: f32 = 11.0;
/// Gap between a timeline node and its text.
const DOT_GAP: f32 = 10.0;
/// Space under a timeline row's text before the next row.
const ROW_GAP: f32 = 10.0;
/// Gap between a timeline title and its trailing meta text.
const META_GAP: f32 = 8.0;

/// Where one step of a flow stands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum StepState {
    /// Not started.
    #[default]
    Pending,
    /// Being worked on.
    Active,
    /// Finished.
    Done,
    /// Finished with something to look at.
    Warning,
    /// Failed.
    Error,
}

impl StepState {
    /// Fill of the node, or `None` for an outlined node.
    pub fn fill(self, t: &Tokens) -> Option<Color32> {
        match self {
            Self::Pending | Self::Active => None,
            Self::Done => Some(t.success),
            Self::Warning => Some(t.warn),
            Self::Error => Some(t.error),
        }
    }

    fn glyph(self) -> Option<Icon> {
        match self {
            Self::Pending | Self::Active => None,
            Self::Done => Some(icons::CHECK),
            Self::Warning => Some(Icon::painted(exclaim)),
            Self::Error => Some(icons::X),
        }
    }

    /// Word a screen reader hears after the step's label.
    pub fn word(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Active => "in progress",
            Self::Done => "done",
            Self::Warning => "needs attention",
            Self::Error => "failed",
        }
    }
}

/// An exclamation mark filling the middle of `rect`.
fn exclaim(p: &Painter, rect: Rect, color: Color32) {
    let w = (rect.width() / 7.0).max(1.0);
    let top = rect.lerp_inside(vec2(0.5, 0.18));
    let bottom = rect.lerp_inside(vec2(0.5, 0.6));
    p.line_segment([top, bottom], Stroke::new(w, color));
    p.circle_filled(rect.lerp_inside(vec2(0.5, 0.82)), w * 0.7, color);
}

/// One step of a [`Stepper`].
#[derive(Clone, Copy, Debug)]
pub struct Step<'a> {
    pub label: &'a str,
    pub state: StepState,
}

impl<'a> Step<'a> {
    pub fn new(label: &'a str, state: StepState) -> Self {
        Self { label, state }
    }
}

/// Paints a node of `diameter` centred in `rect`: a filled status disc, or an outlined one holding `number`.
fn paint_node(
    ui: &Ui,
    t: &Tokens,
    rect: Rect,
    diameter: f32,
    glyph: f32,
    state: StepState,
    number: Option<usize>,
) {
    let center = rect.center();
    let radius = diameter / 2.0;
    let painter = ui.painter();
    match state.fill(t) {
        Some(fill) => {
            painter.circle_filled(center, radius, fill);
            if let Some(icon) = state.glyph() {
                icon.paint(ui, rect, glyph, ink_on(fill));
            }
        }
        None => {
            let (edge, ink, width) = if state == StepState::Active {
                (t.primary, t.primary, 2.0)
            } else {
                (t.field_border, t.text_dim, 1.0)
            };
            painter.circle_filled(center, radius, t.field);
            painter.circle_stroke(center, radius - width / 2.0, Stroke::new(width, edge));
            if let Some(n) = number {
                let text = text_galley(ui, &n.to_string(), semibold_font_sized(ui, NODE_PT));
                painter.galley((center - text.size() / 2.0).round(), text, ink);
            } else if state == StepState::Active {
                painter.circle_filled(center, radius / 3.0, t.primary);
            }
        }
    }
}

/// A row of numbered steps joined by connectors; one step is selected.
#[must_use = "call `show` to draw the stepper"]
pub struct Stepper<'a> {
    steps: &'a [Step<'a>],
    selected: Option<usize>,
    width: Option<f32>,
    clickable: bool,
}

impl<'a> Stepper<'a> {
    pub fn new(steps: &'a [Step<'a>]) -> Self {
        Self {
            steps,
            selected: None,
            width: None,
            clickable: true,
        }
    }

    /// Washes the step at `index` and draws its label semibold.
    pub fn selected(mut self, index: impl Into<Option<usize>>) -> Self {
        self.selected = index.into();
        self
    }

    /// Outer width; the available width by default.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Steps react to hover and clicks; on by default.
    pub fn clickable(mut self, clickable: bool) -> Self {
        self.clickable = clickable;
        self
    }

    /// Height of a stepper at the current density.
    pub fn height(ui: &Ui) -> f32 {
        let (_, m) = kit(ui);
        (NODE + 2.0 * m.pad.y).max(m.button_h)
    }

    /// Draws the steps and returns the index of a clicked one.
    pub fn show(self, ui: &mut Ui) -> Option<usize> {
        let (t, _) = kit(ui);
        let count = self.steps.len();
        if count == 0 {
            return None;
        }
        let width = self.width.unwrap_or_else(|| ui.available_width());
        let cell_w = (width / count as f32).floor().max(1.0);
        let height = Self::height(ui);
        let label_room = cell_w - 2.0 * CELL_PAD - NODE - NODE_GAP;
        let with_labels = label_room >= MIN_LABEL_W;
        let (row, row_response) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
        let id = row_response.id;
        let sense = if self.clickable {
            Sense::click()
        } else {
            Sense::hover()
        };
        let mut clicked = None;

        for (i, step) in self.steps.iter().enumerate() {
            let cell = Rect::from_min_size(
                pos2(row.left() + cell_w * i as f32, row.top()),
                vec2(cell_w, height),
            );
            let selected = self.selected == Some(i);
            let response = ui.interact(cell, id.with(i), sense);
            response.widget_info(|| {
                let label = format!("{} ({})", step.label, step.state.word());
                WidgetInfo::selected(Role::Button, ui.is_enabled(), selected, label)
            });
            if response.clicked() {
                clicked = Some(i);
            }
            if !with_labels {
                let _ = response.clone().on_hover_text(step.label);
            }
            if !ui.is_rect_visible(cell) {
                continue;
            }
            let (state, focused) = if self.clickable {
                state_of(ui, &response)
            } else {
                (State::Rest, false)
            };
            let fill = match (selected, state) {
                (true, _) => t.primary_soft,
                (false, State::Hovered) => t.hover,
                (false, State::Pressed) => t.pressed,
                (false, State::Rest) => Color32::TRANSPARENT,
            };
            let pill_w = if with_labels {
                cell_w
            } else {
                NODE + 2.0 * CELL_PAD
            };
            let pill = Rect::from_min_size(cell.min, vec2(pill_w.min(cell_w), height));
            ui.painter().rect_filled(pill, t.radius_md, fill);

            let node = Rect::from_center_size(
                pos2(cell.left() + CELL_PAD + NODE / 2.0, cell.center().y),
                Vec2::splat(NODE),
            );
            paint_node(ui, &t, node, NODE, NODE_GLYPH, step.state, Some(i + 1));

            let mut content_right = node.right();
            if with_labels {
                let font = if selected {
                    semibold_font_sized(ui, LABEL_PT)
                } else {
                    medium_font_sized(ui, LABEL_PT)
                };
                let label = elided(ui, step.label, font, label_room);
                if label.elided {
                    let _ = response.clone().on_hover_text(step.label);
                }
                let ink = match step.state {
                    StepState::Error => t.error,
                    StepState::Pending if !selected => t.text_dim,
                    _ => t.text,
                };
                let pos = pos2(
                    node.right() + NODE_GAP,
                    (cell.center().y - label.size().y / 2.0).round(),
                );
                content_right = pos.x + label.size().x;
                ui.painter().galley(pos, label, ink);
            }
            if i + 1 < count {
                let from = content_right + NODE_GAP;
                let to = cell.right() + CELL_PAD - NODE_GAP;
                if to - from >= MIN_LINK_W {
                    let tone = if step.state == StepState::Done {
                        t.success
                    } else {
                        t.separator
                    };
                    let y = ui.painter().round_to_pixel_center(cell.center().y);
                    ui.painter().hline(from..=to, y, Stroke::new(LINK_W, tone));
                }
            }
            if focused {
                focus_ring(ui, pill, t.radius_md, &t);
            }
        }
        clicked
    }
}

/// A stepper over `steps` with the step at `selected` washed; returns the index of a clicked step.
pub fn stepper(ui: &mut Ui, steps: &[Step<'_>], selected: usize) -> Option<usize> {
    Stepper::new(steps).selected(selected).show(ui)
}

/// One row of a vertical timeline: a status node, a title, wrapped detail text and a connector down.
#[must_use = "call `show` to draw the row"]
pub struct TimelineRow<'a> {
    title: &'a str,
    detail: &'a str,
    meta: &'a str,
    state: StepState,
    busy: bool,
    last: bool,
    width: Option<f32>,
}

impl<'a> TimelineRow<'a> {
    pub fn new(title: &'a str, state: StepState) -> Self {
        Self {
            title,
            detail: "",
            meta: "",
            state,
            busy: false,
            last: false,
            width: None,
        }
    }

    /// Dim text under the title, wrapped to the row.
    pub fn detail(mut self, detail: &'a str) -> Self {
        self.detail = detail;
        self
    }

    /// Faint text at the right end of the title row, such as a time.
    pub fn meta(mut self, meta: &'a str) -> Self {
        self.meta = meta;
        self
    }

    /// A spinner in place of the node.
    pub fn busy(mut self, busy: bool) -> Self {
        self.busy = busy;
        self
    }

    /// Omits the connector to the next row.
    pub fn last(mut self, last: bool) -> Self {
        self.last = last;
        self
    }

    /// Outer width; the available width by default.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let (t, _) = kit(ui);
        let width = self.width.unwrap_or_else(|| ui.available_width());
        let text_w = (width - DOT - DOT_GAP).max(1.0);
        let small = TextStyle::Small.resolve(ui.style());
        let meta = (!self.meta.is_empty()).then(|| text_galley(ui, self.meta, small.clone()));
        let meta_w = meta.as_ref().map_or(0.0, |g| g.size().x + META_GAP);
        let title_font = medium_font_sized(ui, TextStyle::Body.resolve(ui.style()).size);
        let title = elided(ui, self.title, title_font, (text_w - meta_w).max(1.0));
        let detail = (!self.detail.is_empty()).then(|| {
            let job =
                LayoutJob::simple(self.detail.to_owned(), small, Color32::PLACEHOLDER, text_w);
            ui.painter().layout_job(job)
        });
        let text_h = title.size().y + detail.as_ref().map_or(0.0, |g| g.size().y);
        let height = (text_h.max(DOT) + ROW_GAP).round();
        let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
        response.widget_info(|| {
            let label = format!("{} ({})", self.title, self.state.word());
            WidgetInfo::labeled(Role::Label, ui.is_enabled(), label)
        });
        if !ui.is_rect_visible(rect) {
            return response;
        }
        let node = Rect::from_min_size(rect.min, Vec2::splat(DOT));
        if !self.last {
            let x = ui.painter().round_to_pixel_center(node.center().x);
            let tone = if self.state == StepState::Done {
                t.success.gamma_multiply(0.6)
            } else {
                t.separator
            };
            ui.painter()
                .vline(x, node.bottom()..=rect.bottom(), Stroke::new(LINK_W, tone));
        }
        if self.busy {
            paint_small_spinner(ui, node, None);
        } else {
            paint_node(ui, &t, node, DOT, DOT_GLYPH, self.state, None);
        }
        let x = node.right() + DOT_GAP;
        let title_y = if detail.is_some() {
            rect.top()
        } else {
            (node.center().y - title.size().y / 2.0).round()
        };
        let title_h = title.size().y;
        let ink = match self.state {
            StepState::Error => t.error,
            StepState::Pending if !self.busy => t.text_dim,
            _ => t.text,
        };
        ui.painter().galley(pos2(x, title_y), title, ink);
        if let Some(meta) = meta {
            let pos = pos2(
                rect.right() - meta.size().x,
                (title_y + (title_h - meta.size().y) / 2.0).round(),
            );
            ui.painter().galley(pos, meta, t.text_faint);
        }
        if let Some(detail) = detail {
            ui.painter()
                .galley(pos2(x, title_y + title_h), detail, t.text_dim);
        }
        response
    }
}

impl Widget for TimelineRow<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        self.show(ui)
    }
}

/// A timeline row with a `title` and wrapped `detail`.
pub fn timeline_row(ui: &mut Ui, title: &str, detail: &str, state: StepState) -> Response {
    TimelineRow::new(title, state).detail(detail).show(ui)
}
