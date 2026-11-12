//! A multi-series history chart, big value readouts and the heat ramp.

use std::sync::Arc;

use egui::{
    Color32, Galley, Rect, Response, Role, Sense, Shape, Stroke, TextStyle, Ui, Widget, WidgetInfo,
    lerp, pos2, vec2,
};

use super::gauge::{spark_points, value_range, wash_under};
use super::{Tokens, elided, kit, mix, semibold_font_sized, text_galley, whole};

/// Opacity of the wash under a chart's first series.
const TRACE_WASH: f32 = 0.14;
/// Width of a chart line.
const TRACE_STROKE: f32 = 1.6;
/// Radius of the dot on a series' last value.
const TRACE_DOT: f32 = 2.2;
/// Inset of the lines from the chart well's edge.
const TRACE_INSET: f32 = 4.0;
/// Share of the sunken colour in the chart well.
const WELL: f32 = 0.6;
/// Share of the separator colour in a grid line.
const GRID_MIX: f32 = 0.5;
/// Default number of horizontal grid lines.
const GRID_LINES: usize = 3;
/// Default chart height.
pub const CHART_H: f32 = 120.0;
/// Radius of a legend dot.
const LEGEND_DOT: f32 = 3.0;
/// Gap between a legend dot and its name.
const LEGEND_DOT_GAP: f32 = 5.0;
/// Gap between legend entries.
const LEGEND_ITEM_GAP: f32 = 12.0;
/// Gap above the legend.
const LEGEND_GAP: f32 = 4.0;
/// Shortest well a chart draws lines in.
const MIN_WELL: f32 = 24.0;
/// Radius of the dots on the hovered values.
const HOVER_DOT: f32 = 3.0;
/// Default readout value size.
pub const READOUT_VALUE_PT: f32 = 26.0;
/// Readout unit size as a share of the value size.
const UNIT_SHARE: f32 = 0.5;
/// Gap between a readout's value and its unit.
const UNIT_GAP: f32 = 3.0;
/// Heat shares where the ramp turns from cool to warm and from warm to hot.
const COOL_END: f32 = 0.55;
const WARM_END: f32 = 0.8;

/// Colour of a heat `share`: the info role when cool, through success and warn, to error at 1.
pub fn heat_tone(t: &Tokens, share: f32) -> Color32 {
    let s = if share.is_finite() {
        share.clamp(0.0, 1.0)
    } else {
        0.0
    };
    if s < COOL_END {
        mix(t.info, t.success, s / COOL_END)
    } else if s < WARM_END {
        mix(t.success, t.warn, (s - COOL_END) / (WARM_END - COOL_END))
    } else {
        mix(t.warn, t.error, (s - WARM_END) / (1.0 - WARM_END))
    }
}

/// One named line of a [`LineChart`].
#[derive(Clone, Copy, Debug)]
pub struct Series<'a> {
    pub name: &'a str,
    pub values: &'a [f32],
    pub tone: Option<Color32>,
}

impl<'a> Series<'a> {
    pub fn new(name: &'a str, values: &'a [f32]) -> Self {
        Self {
            name,
            values,
            tone: None,
        }
    }

    /// Line colour in place of the series' place in the token cycle.
    pub fn tone(mut self, tone: Color32) -> Self {
        self.tone = Some(tone);
        self
    }
}

/// Line colour of the `index`th series without a tone of its own.
pub fn series_tone(t: &Tokens, index: usize) -> Color32 {
    let cycle = [t.primary, t.secondary, t.success, t.warn, t.info, t.error];
    cycle[index % cycle.len()]
}

/// Formats a hovered value.
pub type ValueFormatter = fn(f32) -> String;

fn default_format(v: f32) -> String {
    if v.abs() >= 100.0 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}")
    }
}

/// Recent history of one or more series in a sunken well, right-aligned on a window of slots.
#[must_use = "call `show` to draw the chart"]
pub struct LineChart<'a> {
    series: &'a [Series<'a>],
    width: Option<f32>,
    height: f32,
    range: Option<(f32, f32)>,
    slots: Option<usize>,
    legend: bool,
    grid: usize,
    wash: bool,
    span: &'a str,
    empty: &'a str,
    hover: bool,
    format: ValueFormatter,
}

impl<'a> LineChart<'a> {
    pub fn new(series: &'a [Series<'a>]) -> Self {
        Self {
            series,
            width: None,
            height: CHART_H,
            range: None,
            slots: None,
            legend: true,
            grid: GRID_LINES,
            wash: true,
            span: "",
            empty: "No data yet",
            hover: true,
            format: default_format,
        }
    }

    /// Outer width; the available width by default.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Outer height, legend included.
    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    /// Fixed y range in place of the range of every series together.
    pub fn range(mut self, min: f32, max: f32) -> Self {
        self.range = Some((min, max));
        self
    }

    /// Width of the window in samples; a shorter series hugs the right edge.
    pub fn slots(mut self, slots: usize) -> Self {
        self.slots = Some(slots.max(2));
        self
    }

    /// The legend of series names under the well; on by default.
    pub fn legend(mut self, legend: bool) -> Self {
        self.legend = legend;
        self
    }

    /// Number of faint horizontal grid lines; zero hides them.
    pub fn grid(mut self, lines: usize) -> Self {
        self.grid = lines;
        self
    }

    /// The wash under the first series; on by default.
    pub fn wash(mut self, wash: bool) -> Self {
        self.wash = wash;
        self
    }

    /// Faint text at the right end of the legend, such as the window's duration.
    pub fn span(mut self, span: &'a str) -> Self {
        self.span = span;
        self
    }

    /// Text in the well while no series has two values.
    pub fn empty_text(mut self, text: &'a str) -> Self {
        self.empty = text;
        self
    }

    /// A guide line and the values under the pointer; on by default.
    pub fn hover(mut self, hover: bool) -> Self {
        self.hover = hover;
        self
    }

    /// Formats the values the hover tooltip shows.
    pub fn formatter(mut self, format: ValueFormatter) -> Self {
        self.format = format;
        self
    }

    fn window(&self) -> usize {
        self.slots.unwrap_or_else(|| {
            self.series
                .iter()
                .map(|s| s.values.len())
                .max()
                .unwrap_or(0)
                .max(2)
        })
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let (t, _) = kit(ui);
        let width = self.width.unwrap_or_else(|| ui.available_width()).max(1.0);
        let (rect, response) = ui.allocate_exact_size(vec2(width, self.height), Sense::hover());
        response.widget_info(|| {
            let names: Vec<&str> = self.series.iter().map(|s| s.name).collect();
            WidgetInfo::labeled(Role::Image, ui.is_enabled(), names.join(", "))
        });
        if !ui.is_rect_visible(rect) {
            return response;
        }
        let small = TextStyle::Small.resolve(ui.style());
        let legend_h = if self.legend {
            ui.text_style_height(&TextStyle::Small) + LEGEND_GAP
        } else {
            0.0
        };
        let well = Rect::from_min_max(rect.min, pos2(rect.right(), rect.bottom() - legend_h));
        let window = self.window();
        let range = self
            .range
            .or_else(|| {
                value_range(
                    self.series
                        .iter()
                        .flat_map(|s| s.values[s.values.len().saturating_sub(window)..].iter())
                        .copied(),
                )
            })
            .unwrap_or((0.0, 1.0));
        let inner = well.shrink(TRACE_INSET);
        let mut traced: Vec<(usize, Vec<egui::Pos2>, usize)> = Vec::new();
        if well.height() >= MIN_WELL {
            let p = ui.painter();
            p.rect_filled(well, t.radius_sm, mix(t.raised, t.sunken, WELL));
            let guide = Stroke::new(1.0, mix(t.raised, t.separator, GRID_MIX));
            for k in 1..=self.grid {
                let y = lerp(
                    well.top()..=well.bottom(),
                    k as f32 / (self.grid + 1) as f32,
                );
                let y = p.round_to_pixel_center(y);
                p.hline(
                    (well.left() + TRACE_INSET)..=(well.right() - TRACE_INSET),
                    y,
                    guide,
                );
            }
            for (i, s) in self.series.iter().enumerate() {
                let n = s.values.len().min(window);
                if n < 2 {
                    continue;
                }
                let values = &s.values[s.values.len() - n..];
                let share = (n - 1) as f32 / (window - 1) as f32;
                let area = Rect::from_min_max(
                    pos2(inner.right() - inner.width() * share, inner.top()),
                    inner.max,
                );
                traced.push((i, spark_points(values, Some(range), area), window - n));
            }
            for (i, points, _) in &traced {
                let tone = self.series[*i].tone.unwrap_or_else(|| series_tone(&t, *i));
                if self.wash && *i == 0 {
                    p.add(wash_under(
                        points,
                        inner.bottom(),
                        tone.gamma_multiply(TRACE_WASH),
                    ));
                }
                p.add(Shape::line(points.clone(), Stroke::new(TRACE_STROKE, tone)));
                if let Some(last) = points.last() {
                    p.circle_filled(*last, TRACE_DOT, tone);
                }
            }
            if traced.is_empty() && !self.empty.is_empty() {
                let text = elided(ui, self.empty, small.clone(), well.width());
                p.galley(
                    (well.center() - text.size() / 2.0).round(),
                    text,
                    t.text_faint,
                );
            }
        }
        if self.legend {
            self.paint_legend(ui, &t, rect, well.bottom() + LEGEND_GAP);
        }
        if self.hover
            && !traced.is_empty()
            && let Some(pointer) = response.hover_pos()
            && inner.x_range().contains(pointer.x)
        {
            let share = (pointer.x - inner.left()) / inner.width().max(1.0);
            let slot = (share * (window - 1) as f32).round() as usize;
            let x = lerp(
                inner.left()..=inner.right(),
                slot as f32 / (window - 1) as f32,
            );
            ui.painter().vline(
                ui.painter().round_to_pixel_center(x),
                inner.y_range(),
                Stroke::new(1.0, t.text_faint),
            );
            let mut lines = Vec::new();
            for (i, points, offset) in &traced {
                let Some(k) = slot.checked_sub(*offset) else {
                    continue;
                };
                let s = &self.series[*i];
                let values = &s.values[s.values.len() - points.len()..];
                if let (Some(pt), Some(v)) = (points.get(k), values.get(k)) {
                    let tone = s.tone.unwrap_or_else(|| series_tone(&t, *i));
                    ui.painter().circle_filled(*pt, HOVER_DOT, tone);
                    lines.push(format!("{}: {}", s.name, (self.format)(*v)));
                }
            }
            if !lines.is_empty() {
                return response.on_hover_text_at_pointer(lines.join("\n"));
            }
        }
        response
    }

    fn paint_legend(&self, ui: &Ui, t: &Tokens, rect: Rect, y: f32) {
        let small = TextStyle::Small.resolve(ui.style());
        let span = (!self.span.is_empty()).then(|| text_galley(ui, self.span, small.clone()));
        let right = rect.right() - span.as_ref().map_or(0.0, |g| g.size().x + LEGEND_ITEM_GAP);
        let p = ui.painter();
        let mut x = rect.left();
        for (i, s) in self.series.iter().enumerate() {
            let room = right - x - 2.0 * LEGEND_DOT - LEGEND_DOT_GAP;
            if room <= 0.0 {
                break;
            }
            let label = elided(ui, s.name, small.clone(), room);
            let h = label.size().y;
            let tone = s.tone.unwrap_or_else(|| series_tone(t, i));
            p.circle_filled(pos2(x + LEGEND_DOT, y + h / 2.0), LEGEND_DOT, tone);
            x += 2.0 * LEGEND_DOT + LEGEND_DOT_GAP;
            let w = label.size().x;
            p.galley(pos2(x, y), label, t.text_dim);
            x += w + LEGEND_ITEM_GAP;
        }
        if let Some(span) = span {
            p.galley(pos2(rect.right() - span.size().x, y), span, t.text_faint);
        }
    }
}

impl Widget for LineChart<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        self.show(ui)
    }
}

/// A full-width chart of `series` `height` tall.
pub fn line_chart(ui: &mut Ui, series: &[Series<'_>], height: f32) -> Response {
    LineChart::new(series).height(height).show(ui)
}

/// Offset of the first row's baseline from the top of `galley`.
fn baseline(galley: &Galley) -> f32 {
    galley
        .rows
        .first()
        .and_then(|row| row.row.glyphs.first().map(|g| row.pos.y + g.pos.y))
        .unwrap_or(galley.size().y * 0.8)
}

/// A large value and its unit on a shared baseline, over a dim caption and a faint detail line.
#[must_use = "call `show` to draw the readout"]
pub struct Readout<'a> {
    value: &'a str,
    unit: &'a str,
    caption: &'a str,
    detail: &'a str,
    tone: Option<Color32>,
    size: f32,
    width: Option<f32>,
}

impl<'a> Readout<'a> {
    /// An empty value or "—" paints faint.
    pub fn new(value: &'a str, unit: &'a str) -> Self {
        Self {
            value,
            unit,
            caption: "",
            detail: "",
            tone: None,
            size: READOUT_VALUE_PT,
            width: None,
        }
    }

    /// Dim text under the value.
    pub fn caption(mut self, caption: &'a str) -> Self {
        self.caption = caption;
        self
    }

    /// Faint text under the caption.
    pub fn detail(mut self, detail: &'a str) -> Self {
        self.detail = detail;
        self
    }

    /// Value colour in place of the text colour.
    pub fn tone(mut self, tone: Color32) -> Self {
        self.tone = Some(tone);
        self
    }

    /// Value size in points; the unit is half of it.
    pub fn size(mut self, points: f32) -> Self {
        self.size = points.max(1.0);
        self
    }

    /// Fixed width that cuts long captions; the value's own width by default.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    fn galleys(&self, ui: &Ui) -> (Arc<Galley>, Option<Arc<Galley>>, Vec<Arc<Galley>>) {
        let shown = if self.value.is_empty() {
            "—"
        } else {
            self.value
        };
        let value = text_galley(ui, shown, semibold_font_sized(ui, self.size));
        let unit = (!self.unit.is_empty()).then(|| {
            let font = semibold_font_sized(ui, (self.size * UNIT_SHARE).round());
            text_galley(ui, self.unit, font)
        });
        let room = self.width.unwrap_or(f32::INFINITY);
        let small = TextStyle::Small.resolve(ui.style());
        let lines = [self.caption, self.detail]
            .into_iter()
            .filter(|s| !s.is_empty())
            .map(|s| elided(ui, s, small.clone(), room))
            .collect();
        (value, unit, lines)
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let (t, _) = kit(ui);
        let (value, unit, lines) = self.galleys(ui);
        let unit_w = unit.as_ref().map_or(0.0, |u| UNIT_GAP + u.size().x);
        let head_w = value.size().x + unit_w;
        let width = self
            .width
            .unwrap_or_else(|| lines.iter().map(|g| g.size().x).fold(head_w, f32::max));
        let height = value.size().y + lines.iter().map(|g| g.size().y).sum::<f32>();
        let (rect, response) =
            ui.allocate_exact_size(vec2(whole(width), height.ceil()), Sense::hover());
        response.widget_info(|| {
            let label = format!(
                "{} {}{} {}",
                self.caption, self.value, self.unit, self.detail
            );
            WidgetInfo::labeled(Role::Label, ui.is_enabled(), label.trim())
        });
        if !ui.is_rect_visible(rect) {
            return response;
        }
        let p = ui.painter();
        let faint = self.value.is_empty() || self.value == "—";
        let ink = if faint {
            t.text_faint
        } else {
            self.tone.unwrap_or(t.text)
        };
        let value_base = baseline(&value);
        let value_h = value.size().y;
        let value_w = value.size().x;
        p.galley(rect.min, value, ink);
        if let Some(unit) = unit {
            let y = rect.top() + value_base - baseline(&unit);
            p.galley(
                pos2(rect.left() + value_w + UNIT_GAP, y.round()),
                unit,
                t.text_dim,
            );
        }
        let mut y = rect.top() + value_h;
        for (i, line) in lines.into_iter().enumerate() {
            let h = line.size().y;
            let ink = if i == 0 && !self.caption.is_empty() {
                t.text_dim
            } else {
                t.text_faint
            };
            p.galley(pos2(rect.left(), y), line, ink);
            y += h;
        }
        response
    }
}

impl Widget for Readout<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        self.show(ui)
    }
}

/// A large `value` with its `unit` over a dim `caption`.
pub fn readout(ui: &mut Ui, value: &str, unit: &str, caption: &str) -> Response {
    Readout::new(value, unit).caption(caption).show(ui)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_heat_ramp_runs_from_info_to_error() {
        let t = Tokens::derive(&egui::Style::default(), Color32::GREEN, Color32::RED);
        assert_eq!(heat_tone(&t, 0.0), t.info);
        assert_eq!(heat_tone(&t, 1.0), t.error);
        assert_eq!(heat_tone(&t, -3.0), t.info);
        assert_eq!(heat_tone(&t, f32::NAN), t.info);
        assert_eq!(heat_tone(&t, COOL_END), t.success);
        assert_eq!(heat_tone(&t, WARM_END), t.warn);
    }

    #[test]
    fn untoned_series_cycle_through_the_tokens() {
        let t = Tokens::derive(&egui::Style::default(), Color32::GREEN, Color32::RED);
        assert_eq!(series_tone(&t, 0), t.primary);
        assert_eq!(series_tone(&t, 1), t.secondary);
        assert_eq!(series_tone(&t, 6), t.primary);
    }
}
