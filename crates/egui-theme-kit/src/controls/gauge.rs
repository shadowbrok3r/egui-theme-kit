//! Ring gauges and sparklines for dashboard tiles.

use std::f32::consts::{FRAC_1_SQRT_2, PI};

use egui::{
    Color32, Mesh, Pos2, Rect, Response, Role, Sense, Shape, Stroke, TextStyle, Ui, Vec2, Widget,
    WidgetInfo, lerp, pos2,
};

use super::{Tokens, elided, kit, semibold_font_sized};

/// Angle of the ring's first point, measured clockwise from the positive x axis.
const RING_START: f32 = 0.75 * PI;
/// Angle the ring's track covers; the gap sits at the bottom.
const RING_SWEEP: f32 = 1.5 * PI;
/// Stroke width of a ring as a share of its diameter.
const RING_STROKE: f32 = 0.1;
/// Thinnest ring stroke.
const RING_MIN_STROKE: f32 = 3.0;
/// Arc points per radian.
const ARC_DENSITY: f32 = 14.0;
/// Seconds a ring takes to settle on a new value.
const RING_EASE_SECS: f32 = 0.35;
/// Centre text size as a share of the diameter.
const RING_VALUE_SHARE: f32 = 0.25;
/// Smallest centre text size.
const RING_VALUE_MIN_PT: f32 = 10.0;
/// Distance of the caption's centre below the ring's centre, as a share of the radius.
const RING_CAPTION_DROP: f32 = 0.86;
/// Space kept between a caption and the ring's end caps.
const CAPTION_INSET: f32 = 2.0;
/// Default ring diameter.
pub const RING_DIAMETER: f32 = 76.0;
/// Opacity of the wash under a sparkline.
const SPARK_FILL: f32 = 0.16;
/// Width of a sparkline's stroke.
const SPARK_STROKE: f32 = 1.5;
/// Radius of the dot on a sparkline's last value.
const SPARK_DOT: f32 = 2.0;

/// Points along a clockwise arc of `sweep` radians starting at `start`.
pub(crate) fn arc_points(center: Pos2, radius: f32, start: f32, sweep: f32) -> Vec<Pos2> {
    let steps = ((sweep.abs() * ARC_DENSITY).ceil() as usize).max(2);
    (0..=steps)
        .map(|i| center + radius * Vec2::angled(start + sweep * i as f32 / steps as f32))
        .collect()
}

/// A three-quarter ring filled to a fraction, with a value in its centre and a caption in its gap.
#[must_use = "call `show` to draw the gauge"]
pub struct RingGauge<'a> {
    fraction: Option<f32>,
    value: &'a str,
    caption: &'a str,
    tone: Option<Color32>,
    thresholds: Option<(f32, f32)>,
    diameter: f32,
    animate: bool,
}

impl<'a> RingGauge<'a> {
    /// `fraction` of the ring filled, clamped to 0..=1; `None` draws an empty ring with faint text.
    pub fn new(fraction: impl Into<Option<f32>>, value: &'a str) -> Self {
        Self {
            fraction: fraction.into().map(|f| f.clamp(0.0, 1.0)),
            value,
            caption: "",
            tone: None,
            thresholds: None,
            diameter: RING_DIAMETER,
            animate: true,
        }
    }

    /// Small dim text in the ring's gap.
    pub fn caption(mut self, caption: &'a str) -> Self {
        self.caption = caption;
        self
    }

    /// Arc colour in place of the primary role; thresholds still win once crossed.
    pub fn tone(mut self, tone: Color32) -> Self {
        self.tone = Some(tone);
        self
    }

    /// Paints the arc in the warn role from `warn` and the error role from `danger` up.
    pub fn thresholds(mut self, warn: f32, danger: f32) -> Self {
        self.thresholds = Some((warn, danger.max(warn)));
        self
    }

    pub fn diameter(mut self, diameter: f32) -> Self {
        self.diameter = diameter.max(2.0 * RING_MIN_STROKE + 4.0);
        self
    }

    /// Eases the arc toward a new value; on by default.
    pub fn animate(mut self, animate: bool) -> Self {
        self.animate = animate;
        self
    }

    /// The arc colour for `fraction`.
    pub fn tone_for(&self, t: &Tokens, fraction: f32) -> Color32 {
        match self.thresholds {
            Some((_, danger)) if fraction >= danger => t.error,
            Some((warn, _)) if fraction >= warn => t.warn,
            _ => self.tone.unwrap_or(t.primary),
        }
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let (t, _) = kit(ui);
        let d = self.diameter;
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(d), Sense::hover());
        response.widget_info(|| {
            let label = format!("{} {}", self.caption, self.value);
            WidgetInfo::labeled(Role::ProgressIndicator, ui.is_enabled(), label.trim())
        });
        if !ui.is_rect_visible(rect) {
            return response;
        }
        let stroke = (d * RING_STROKE).round().max(RING_MIN_STROKE);
        let radius = (d - stroke) / 2.0;
        let center = rect.center();
        let painter = ui.painter();

        let track = Stroke::new(stroke, t.field_border);
        painter.add(Shape::line(
            arc_points(center, radius, RING_START, RING_SWEEP),
            track,
        ));
        for end in [RING_START, RING_START + RING_SWEEP] {
            painter.circle_filled(
                center + radius * Vec2::angled(end),
                stroke / 2.0,
                track.color,
            );
        }

        let target = self.fraction.unwrap_or(0.0);
        let shown = if self.animate {
            ui.ctx()
                .animate_value_with_time(response.id, target, RING_EASE_SECS)
        } else {
            target
        };
        let tone = self.tone_for(&t, target);
        if self.fraction.is_some() && shown > 0.0 {
            let sweep = RING_SWEEP * shown;
            painter.add(Shape::line(
                arc_points(center, radius, RING_START, sweep),
                Stroke::new(stroke, tone),
            ));
            for end in [RING_START, RING_START + sweep] {
                painter.circle_filled(center + radius * Vec2::angled(end), stroke / 2.0, tone);
            }
        }

        let value_pt = (d * RING_VALUE_SHARE).max(RING_VALUE_MIN_PT);
        let room = 2.0 * (radius - stroke);
        let value = elided(ui, self.value, semibold_font_sized(ui, value_pt), room);
        let ink = if self.fraction.is_some() {
            t.text
        } else {
            t.text_faint
        };
        painter.galley((center - value.size() / 2.0).round(), value, ink);

        if self.caption.is_empty() {
            return response;
        }
        let opening = 2.0 * (radius * FRAC_1_SQRT_2 - stroke / 2.0) - CAPTION_INSET;
        let font = TextStyle::Small.resolve(ui.style());
        let caption = elided(ui, self.caption, font, opening);
        let cut = caption.elided;
        let y = center.y + radius * RING_CAPTION_DROP - caption.size().y / 2.0;
        let pos = pos2((center.x - caption.size().x / 2.0).round(), y.round());
        painter.galley(pos, caption, t.text_dim);
        if cut {
            response.on_hover_text(self.caption)
        } else {
            response
        }
    }
}

impl Widget for RingGauge<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        self.show(ui)
    }
}

/// A ring gauge `diameter` wide filled to `fraction`, showing `value` with `caption` in its gap.
pub fn ring_gauge(
    ui: &mut Ui,
    fraction: impl Into<Option<f32>>,
    value: &str,
    caption: &str,
    diameter: f32,
) -> Response {
    RingGauge::new(fraction, value)
        .caption(caption)
        .diameter(diameter)
        .show(ui)
}

/// A small line chart of recent values with a wash under it and a dot on the last value.
#[must_use = "call `show` to draw the sparkline"]
pub struct Sparkline<'a> {
    values: &'a [f32],
    tone: Option<Color32>,
    size: Vec2,
    range: Option<(f32, f32)>,
    wash: bool,
    dot: bool,
}

impl<'a> Sparkline<'a> {
    pub fn new(values: &'a [f32], size: Vec2) -> Self {
        Self {
            values,
            tone: None,
            size,
            range: None,
            wash: true,
            dot: true,
        }
    }

    /// Line colour in place of the primary role.
    pub fn tone(mut self, tone: Color32) -> Self {
        self.tone = Some(tone);
        self
    }

    /// Fixed value range in place of the values' own minimum and maximum.
    pub fn range(mut self, min: f32, max: f32) -> Self {
        self.range = Some((min, max));
        self
    }

    /// The wash under the line; on by default.
    pub fn wash(mut self, wash: bool) -> Self {
        self.wash = wash;
        self
    }

    /// The dot on the last value; on by default.
    pub fn dot(mut self, dot: bool) -> Self {
        self.dot = dot;
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let (rect, response) = ui.allocate_exact_size(self.size, Sense::hover());
        response.widget_info(|| {
            let last = self.values.last().map_or(String::new(), |v| format!("{v}"));
            WidgetInfo::labeled(Role::Image, ui.is_enabled(), last)
        });
        if ui.is_rect_visible(rect) {
            self.paint(ui, rect);
        }
        response
    }

    /// Paints the line into `rect` without allocating; nothing for fewer than two values.
    pub fn paint(&self, ui: &Ui, rect: Rect) {
        if self.values.len() < 2 {
            return;
        }
        let (t, _) = kit(ui);
        let tone = self.tone.unwrap_or(t.primary);
        let points = spark_points(self.values, self.range, rect.shrink(SPARK_DOT));
        let painter = ui.painter();
        if self.wash {
            painter.add(wash_under(
                &points,
                rect.bottom(),
                tone.gamma_multiply(SPARK_FILL),
            ));
        }
        let last = points.last().copied();
        painter.add(Shape::line(points, Stroke::new(SPARK_STROKE, tone)));
        if let (true, Some(last)) = (self.dot, last) {
            painter.circle_filled(last, SPARK_DOT, tone);
        }
    }
}

impl Widget for Sparkline<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        self.show(ui)
    }
}

/// A sparkline of `values` filling `size`.
pub fn sparkline(ui: &mut Ui, values: &[f32], size: Vec2) -> Response {
    Sparkline::new(values, size).show(ui)
}

/// A mesh filling the area between `points` and the horizontal line at `bottom`.
pub(crate) fn wash_under(points: &[Pos2], bottom: f32, fill: Color32) -> Shape {
    let mut wash = Mesh::default();
    if points.len() < 2 {
        return Shape::Noop;
    }
    for p in points {
        wash.colored_vertex(*p, fill);
        wash.colored_vertex(pos2(p.x, bottom), fill);
    }
    for i in 0..points.len() as u32 - 1 {
        let a = 2 * i;
        wash.add_triangle(a, a + 1, a + 2);
        wash.add_triangle(a + 1, a + 3, a + 2);
    }
    Shape::mesh(wash)
}

/// The smallest and largest finite value of `values`, or `None` when there is none.
pub(crate) fn value_range(values: impl IntoIterator<Item = f32>) -> Option<(f32, f32)> {
    values
        .into_iter()
        .filter(|v| v.is_finite())
        .fold(None, |acc, v| match acc {
            None => Some((v, v)),
            Some((lo, hi)) => Some((lo.min(v), hi.max(v))),
        })
}

/// `values` spread left to right across `rect`, the range's top at the rect's top.
pub(crate) fn spark_points(values: &[f32], range: Option<(f32, f32)>, rect: Rect) -> Vec<Pos2> {
    let (min, max) = range
        .or_else(|| value_range(values.iter().copied()))
        .unwrap_or((0.0, 1.0));
    let span = (max - min).max(f32::EPSILON);
    let last = (values.len().max(2) - 1) as f32;
    values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let share = if v.is_finite() {
                ((v - min) / span).clamp(0.0, 1.0)
            } else {
                0.0
            };
            pos2(
                lerp(rect.left()..=rect.right(), i as f32 / last),
                lerp(rect.bottom()..=rect.top(), share),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::vec2;

    #[test]
    fn an_arc_starts_and_ends_on_its_angles() {
        let center = pos2(50.0, 50.0);
        let points = arc_points(center, 10.0, 0.0, PI);
        let (first, last) = (points[0], points[points.len() - 1]);
        assert!((first - pos2(60.0, 50.0)).length() < 1e-3);
        assert!((last - pos2(40.0, 50.0)).length() < 1e-3);
        assert!(points.iter().all(|p| p.y >= 50.0 - 1e-3));
    }

    #[test]
    fn the_ring_gap_sits_at_the_bottom() {
        let points = arc_points(Pos2::ZERO, 1.0, RING_START, RING_SWEEP);
        let (first, last) = (points[0], points[points.len() - 1]);
        assert!(first.x < 0.0 && first.y > 0.0);
        assert!(last.x > 0.0 && last.y > 0.0);
        assert!((first.y - last.y).abs() < 1e-3);
    }

    #[test]
    fn spark_points_span_the_rect_and_rise_with_the_value() {
        let rect = Rect::from_min_size(pos2(10.0, 20.0), vec2(100.0, 40.0));
        let points = spark_points(&[0.0, 5.0, 10.0], None, rect);
        assert_eq!(points[0], pos2(10.0, 60.0));
        assert_eq!(points[1], pos2(60.0, 40.0));
        assert_eq!(points[2], pos2(110.0, 20.0));
    }

    #[test]
    fn a_flat_series_and_an_out_of_range_value_stay_inside_the_rect() {
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(10.0, 10.0));
        for p in spark_points(&[3.0, 3.0, 3.0], None, rect) {
            assert!(rect.contains(p));
        }
        let clamped = spark_points(&[-50.0, 500.0, f32::NAN], Some((0.0, 100.0)), rect);
        assert_eq!(clamped[0].y, 10.0);
        assert_eq!(clamped[1].y, 0.0);
        assert!(rect.contains(clamped[2]));
    }

    #[test]
    fn thresholds_shift_the_arc_tone() {
        let t = Tokens::derive(&egui::Style::default(), Color32::GREEN, Color32::RED);
        let ring = RingGauge::new(0.5, "50%").thresholds(0.75, 0.9);
        assert_eq!(ring.tone_for(&t, 0.5), t.primary);
        assert_eq!(ring.tone_for(&t, 0.8), t.warn);
        assert_eq!(ring.tone_for(&t, 0.95), t.error);
        let toned = RingGauge::new(0.5, "").tone(Color32::BLUE);
        assert_eq!(toned.tone_for(&t, 0.99), Color32::BLUE);
    }
}
