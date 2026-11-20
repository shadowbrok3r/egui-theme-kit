//! Small icon set the kit paints with vector strokes, so controls need no icon font or image loader.

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, vec2};

use super::Icon;

/// Stroke width as a share of the icon side.
const STROKE_SHARE: f32 = 1.0 / 11.0;

pub const CARET_DOWN: Icon = Icon::painted(caret_down);
pub const CARET_UP: Icon = Icon::painted(caret_up);
pub const CARET_RIGHT: Icon = Icon::painted(caret_right);
pub const CARET_LEFT: Icon = Icon::painted(caret_left);
pub const CHECK: Icon = Icon::painted(check);
pub const X: Icon = Icon::painted(cross);
pub const PLUS: Icon = Icon::painted(plus);
pub const MINUS: Icon = Icon::painted(minus);
pub const SQUARE: Icon = Icon::painted(square);
pub const COPY: Icon = Icon::painted(copy);
pub const SEARCH: Icon = Icon::painted(search);
pub const CIRCLE: Icon = Icon::painted(circle);
pub const DOT: Icon = Icon::painted(dot);
pub const INFO: Icon = Icon::painted(info);
pub const WARNING: Icon = Icon::painted(warning);
pub const ERROR: Icon = Icon::painted(error);
pub const CHECK_CIRCLE: Icon = Icon::painted(check_circle);
pub const MINUS_CIRCLE: Icon = Icon::painted(minus_circle);
pub const MENU: Icon = Icon::painted(menu);
pub const GRID: Icon = Icon::painted(grid);
pub const LIST: Icon = Icon::painted(list);
pub const FOLDER: Icon = Icon::painted(folder);
pub const FILE: Icon = Icon::painted(file);
pub const PLAY: Icon = Icon::painted(play);
pub const PAUSE: Icon = Icon::painted(pause);
pub const STOP: Icon = Icon::painted(stop);
pub const GEAR: Icon = Icon::painted(gear);
pub const CLOCK: Icon = Icon::painted(clock);
pub const PAPERCLIP: Icon = Icon::painted(paperclip);
pub const SEND: Icon = Icon::painted(send);
pub const AT: Icon = Icon::painted(at);
pub const SLASH: Icon = Icon::painted(slash);
pub const QUEUE: Icon = Icon::painted(queue);
pub const LIGHTNING: Icon = Icon::painted(lightning);
pub const BRAIN: Icon = Icon::painted(brain);
pub const WRENCH: Icon = Icon::painted(wrench);
pub const TERMINAL: Icon = Icon::painted(terminal);
pub const FILE_DIFF: Icon = Icon::painted(file_diff);
pub const CHEVRON_UP: Icon = Icon::painted(chevron_up);
pub const CHEVRON_DOWN: Icon = Icon::painted(chevron_down);
pub const EXPAND: Icon = Icon::painted(expand);
pub const COLLAPSE: Icon = Icon::painted(collapse);
pub const QUESTION: Icon = Icon::painted(question);
pub const SHIELD: Icon = Icon::painted(shield);
pub const CUT: Icon = Icon::painted(cut);
pub const PASTE: Icon = Icon::painted(paste);
pub const SELECT_ALL: Icon = Icon::painted(select_all);

/// Every icon of the set with its name.
pub const ALL: [(&str, Icon); 47] = [
    ("caret-down", CARET_DOWN),
    ("caret-up", CARET_UP),
    ("caret-right", CARET_RIGHT),
    ("caret-left", CARET_LEFT),
    ("check", CHECK),
    ("x", X),
    ("plus", PLUS),
    ("minus", MINUS),
    ("square", SQUARE),
    ("copy", COPY),
    ("search", SEARCH),
    ("circle", CIRCLE),
    ("dot", DOT),
    ("info", INFO),
    ("warning", WARNING),
    ("error", ERROR),
    ("check-circle", CHECK_CIRCLE),
    ("minus-circle", MINUS_CIRCLE),
    ("menu", MENU),
    ("grid", GRID),
    ("list", LIST),
    ("folder", FOLDER),
    ("file", FILE),
    ("play", PLAY),
    ("pause", PAUSE),
    ("stop", STOP),
    ("gear", GEAR),
    ("clock", CLOCK),
    ("paperclip", PAPERCLIP),
    ("send", SEND),
    ("at", AT),
    ("slash", SLASH),
    ("queue", QUEUE),
    ("lightning", LIGHTNING),
    ("brain", BRAIN),
    ("wrench", WRENCH),
    ("terminal", TERMINAL),
    ("file-diff", FILE_DIFF),
    ("chevron-up", CHEVRON_UP),
    ("chevron-down", CHEVRON_DOWN),
    ("expand", EXPAND),
    ("collapse", COLLAPSE),
    ("question", QUESTION),
    ("shield", SHIELD),
    ("cut", CUT),
    ("paste", PASTE),
    ("select-all", SELECT_ALL),
];

/// A square slot with unit coordinates, a stroke width and a colour.
struct Pen<'a> {
    painter: &'a Painter,
    rect: Rect,
    stroke: Stroke,
}

impl<'a> Pen<'a> {
    fn new(painter: &'a Painter, rect: Rect, color: Color32) -> Self {
        let width = (rect.width() * STROKE_SHARE).max(1.0);
        Self {
            painter,
            rect,
            stroke: Stroke::new(width, color),
        }
    }

    /// Unit point `(x, y)` in the slot.
    fn at(&self, x: f32, y: f32) -> Pos2 {
        self.rect.min + vec2(x, y) * self.rect.size()
    }

    /// Length `unit` in slot units.
    fn len(&self, unit: f32) -> f32 {
        unit * self.rect.width()
    }

    fn line(&self, points: &[(f32, f32)]) {
        let points = points.iter().map(|&(x, y)| self.at(x, y)).collect();
        self.painter.add(Shape::line(points, self.stroke));
    }

    fn closed(&self, points: &[(f32, f32)]) {
        let points = points.iter().map(|&(x, y)| self.at(x, y)).collect();
        self.painter.add(Shape::closed_line(points, self.stroke));
    }

    fn filled(&self, points: &[(f32, f32)]) {
        let points = points.iter().map(|&(x, y)| self.at(x, y)).collect();
        self.painter.add(Shape::convex_polygon(
            points,
            self.stroke.color,
            Stroke::NONE,
        ));
    }

    fn ring(&self, x: f32, y: f32, radius: f32) {
        self.painter
            .circle_stroke(self.at(x, y), self.len(radius), self.stroke);
    }

    fn disc(&self, x: f32, y: f32, radius: f32) {
        self.painter
            .circle_filled(self.at(x, y), self.len(radius), self.stroke.color);
    }

    fn rounded_outline(&self, min: (f32, f32), max: (f32, f32), radius: f32) {
        let rect = Rect::from_min_max(self.at(min.0, min.1), self.at(max.0, max.1));
        self.painter
            .rect_stroke(rect, self.len(radius), self.stroke, StrokeKind::Middle);
    }

    fn outline(&self, min: (f32, f32), max: (f32, f32)) {
        let rect = Rect::from_min_max(self.at(min.0, min.1), self.at(max.0, max.1));
        self.painter
            .rect_stroke(rect, 0, self.stroke, StrokeKind::Middle);
    }
}

fn caret_down(p: &Painter, r: Rect, c: Color32) {
    Pen::new(p, r, c).line(&[(0.24, 0.38), (0.5, 0.64), (0.76, 0.38)]);
}

fn caret_up(p: &Painter, r: Rect, c: Color32) {
    Pen::new(p, r, c).line(&[(0.24, 0.62), (0.5, 0.36), (0.76, 0.62)]);
}

fn caret_right(p: &Painter, r: Rect, c: Color32) {
    Pen::new(p, r, c).line(&[(0.38, 0.24), (0.64, 0.5), (0.38, 0.76)]);
}

fn caret_left(p: &Painter, r: Rect, c: Color32) {
    Pen::new(p, r, c).line(&[(0.62, 0.24), (0.36, 0.5), (0.62, 0.76)]);
}

fn check(p: &Painter, r: Rect, c: Color32) {
    Pen::new(p, r, c).line(&[(0.18, 0.52), (0.4, 0.74), (0.82, 0.28)]);
}

fn cross(p: &Painter, r: Rect, c: Color32) {
    let pen = Pen::new(p, r, c);
    pen.line(&[(0.26, 0.26), (0.74, 0.74)]);
    pen.line(&[(0.74, 0.26), (0.26, 0.74)]);
}

fn plus(p: &Painter, r: Rect, c: Color32) {
    let pen = Pen::new(p, r, c);
    pen.line(&[(0.5, 0.24), (0.5, 0.76)]);
    pen.line(&[(0.24, 0.5), (0.76, 0.5)]);
}

fn minus(p: &Painter, r: Rect, c: Color32) {
    Pen::new(p, r, c).line(&[(0.22, 0.5), (0.78, 0.5)]);
}

fn square(p: &Painter, r: Rect, c: Color32) {
    Pen::new(p, r, c).outline((0.22, 0.22), (0.78, 0.78));
}

fn copy(p: &Painter, r: Rect, c: Color32) {
    let pen = Pen::new(p, r, c);
    pen.outline((0.18, 0.34), (0.66, 0.82));
    pen.line(&[
        (0.34, 0.34),
        (0.34, 0.18),
        (0.82, 0.18),
        (0.82, 0.66),
        (0.66, 0.66),
    ]);
}

fn search(p: &Painter, r: Rect, c: Color32) {
    let pen = Pen::new(p, r, c);
    pen.ring(0.43, 0.43, 0.25);
    pen.line(&[(0.62, 0.62), (0.82, 0.82)]);
}

fn circle(p: &Painter, r: Rect, c: Color32) {
    Pen::new(p, r, c).ring(0.5, 0.5, 0.34);
}

fn dot(p: &Painter, r: Rect, c: Color32) {
    Pen::new(p, r, c).disc(0.5, 0.5, 0.2);
}

fn info(p: &Painter, r: Rect, c: Color32) {
    let pen = Pen::new(p, r, c);
    pen.ring(0.5, 0.5, 0.36);
    pen.disc(0.5, 0.35, 0.045);
    pen.line(&[(0.5, 0.47), (0.5, 0.69)]);
}

fn warning(p: &Painter, r: Rect, c: Color32) {
    let pen = Pen::new(p, r, c);
    pen.closed(&[(0.5, 0.16), (0.88, 0.8), (0.12, 0.8)]);
    pen.line(&[(0.5, 0.4), (0.5, 0.58)]);
    pen.disc(0.5, 0.69, 0.045);
}

fn error(p: &Painter, r: Rect, c: Color32) {
    let pen = Pen::new(p, r, c);
    pen.ring(0.5, 0.5, 0.36);
    pen.line(&[(0.37, 0.37), (0.63, 0.63)]);
    pen.line(&[(0.63, 0.37), (0.37, 0.63)]);
}

fn check_circle(p: &Painter, r: Rect, c: Color32) {
    let pen = Pen::new(p, r, c);
    pen.ring(0.5, 0.5, 0.36);
    pen.line(&[(0.32, 0.51), (0.45, 0.64), (0.69, 0.36)]);
}

fn minus_circle(p: &Painter, r: Rect, c: Color32) {
    let pen = Pen::new(p, r, c);
    pen.ring(0.5, 0.5, 0.36);
    pen.line(&[(0.33, 0.5), (0.67, 0.5)]);
}

fn menu(p: &Painter, r: Rect, c: Color32) {
    let pen = Pen::new(p, r, c);
    for y in [0.28, 0.5, 0.72] {
        pen.line(&[(0.2, y), (0.8, y)]);
    }
}

fn grid(p: &Painter, r: Rect, c: Color32) {
    let pen = Pen::new(p, r, c);
    for (x, y) in [(0.16, 0.16), (0.54, 0.16), (0.16, 0.54), (0.54, 0.54)] {
        pen.outline((x, y), (x + 0.3, y + 0.3));
    }
}

fn list(p: &Painter, r: Rect, c: Color32) {
    let pen = Pen::new(p, r, c);
    for y in [0.28, 0.5, 0.72] {
        pen.disc(0.2, y, 0.04);
        pen.line(&[(0.36, y), (0.84, y)]);
    }
}

fn folder(p: &Painter, r: Rect, c: Color32) {
    Pen::new(p, r, c).closed(&[
        (0.12, 0.22),
        (0.4, 0.22),
        (0.48, 0.34),
        (0.88, 0.34),
        (0.88, 0.8),
        (0.12, 0.8),
    ]);
}

fn file(p: &Painter, r: Rect, c: Color32) {
    let pen = Pen::new(p, r, c);
    pen.closed(&[
        (0.22, 0.14),
        (0.6, 0.14),
        (0.8, 0.34),
        (0.8, 0.86),
        (0.22, 0.86),
    ]);
    pen.line(&[(0.6, 0.14), (0.6, 0.34), (0.8, 0.34)]);
}

fn play(p: &Painter, r: Rect, c: Color32) {
    Pen::new(p, r, c).filled(&[(0.3, 0.2), (0.8, 0.5), (0.3, 0.8)]);
}

fn pause(p: &Painter, r: Rect, c: Color32) {
    let pen = Pen::new(p, r, c);
    pen.filled(&[(0.26, 0.2), (0.4, 0.2), (0.4, 0.8), (0.26, 0.8)]);
    pen.filled(&[(0.6, 0.2), (0.74, 0.2), (0.74, 0.8), (0.6, 0.8)]);
}

fn stop(p: &Painter, r: Rect, c: Color32) {
    Pen::new(p, r, c).filled(&[(0.24, 0.24), (0.76, 0.24), (0.76, 0.76), (0.24, 0.76)]);
}

fn gear(p: &Painter, r: Rect, c: Color32) {
    let pen = Pen::new(p, r, c);
    let teeth = 8;
    let mut points = Vec::with_capacity(4 * teeth);
    for i in 0..teeth {
        let base = i as f32 * std::f32::consts::TAU / teeth as f32;
        for (offset, radius) in [(-0.22, 0.3), (-0.12, 0.44), (0.12, 0.44), (0.22, 0.3)] {
            let (sin, cos) = (base + offset).sin_cos();
            points.push((0.5 + radius * cos, 0.5 + radius * sin));
        }
    }
    pen.closed(&points);
    pen.ring(0.5, 0.5, 0.12);
}

fn clock(p: &Painter, r: Rect, c: Color32) {
    let pen = Pen::new(p, r, c);
    pen.ring(0.5, 0.5, 0.36);
    pen.line(&[(0.5, 0.28), (0.5, 0.5), (0.65, 0.58)]);
}

/// Points on the circle at `(cx, cy)` of `radius` from angle `from` to `to`, in radians with y down.
fn arc(cx: f32, cy: f32, radius: f32, from: f32, to: f32) -> Vec<(f32, f32)> {
    let steps = ((to - from).abs() / 0.2).ceil().max(2.0) as usize;
    (0..=steps)
        .map(|i| {
            let a = from + (to - from) * i as f32 / steps as f32;
            (cx + radius * a.cos(), cy + radius * a.sin())
        })
        .collect()
}

fn paperclip(p: &Painter, r: Rect, c: Color32) {
    use std::f32::consts::PI;
    let mut points = vec![(0.66, 0.36)];
    points.extend(arc(0.5, 0.68, 0.16, 0.0, PI));
    points.extend(arc(0.45, 0.3, 0.11, PI, 2.0 * PI));
    points.push((0.56, 0.64));
    Pen::new(p, r, c).line(&points);
}

fn send(p: &Painter, r: Rect, c: Color32) {
    let pen = Pen::new(p, r, c);
    pen.closed(&[(0.14, 0.2), (0.86, 0.5), (0.14, 0.8), (0.28, 0.5)]);
    pen.line(&[(0.28, 0.5), (0.56, 0.5)]);
}

fn at(p: &Painter, r: Rect, c: Color32) {
    use std::f32::consts::PI;
    let pen = Pen::new(p, r, c);
    pen.ring(0.5, 0.5, 0.13);
    let mut points = vec![(0.63, 0.36), (0.63, 0.58)];
    points.extend(arc(0.73, 0.58, 0.1, PI, 0.0));
    points.extend(arc(0.5, 0.5, 0.34, 0.0, -1.6 * PI));
    pen.line(&points);
}

fn slash(p: &Painter, r: Rect, c: Color32) {
    let pen = Pen::new(p, r, c);
    pen.rounded_outline((0.14, 0.14), (0.86, 0.86), 0.14);
    pen.line(&[(0.6, 0.3), (0.4, 0.7)]);
}

fn queue(p: &Painter, r: Rect, c: Color32) {
    let pen = Pen::new(p, r, c);
    pen.line(&[(0.16, 0.28), (0.84, 0.28)]);
    pen.line(&[(0.16, 0.5), (0.84, 0.5)]);
    pen.line(&[(0.16, 0.72), (0.48, 0.72)]);
    pen.line(&[(0.7, 0.6), (0.7, 0.84)]);
    pen.line(&[(0.58, 0.72), (0.82, 0.72)]);
}

fn lightning(p: &Painter, r: Rect, c: Color32) {
    Pen::new(p, r, c).closed(&[
        (0.56, 0.12),
        (0.24, 0.56),
        (0.48, 0.56),
        (0.42, 0.88),
        (0.76, 0.42),
        (0.52, 0.42),
    ]);
}

fn brain(p: &Painter, r: Rect, c: Color32) {
    use std::f32::consts::TAU;
    let pen = Pen::new(p, r, c);
    let steps = 48;
    let outline: Vec<(f32, f32)> = (0..steps)
        .map(|i| {
            let a = i as f32 * TAU / steps as f32;
            let radius = 0.33 * (1.0 + 0.07 * (6.0 * a).sin());
            (0.5 + radius * a.cos(), 0.5 + 0.9 * radius * a.sin())
        })
        .collect();
    pen.closed(&outline);
    pen.line(&[(0.5, 0.22), (0.5, 0.78)]);
    pen.line(&arc(0.36, 0.5, 0.08, -1.2, 1.2));
    pen.line(&arc(0.64, 0.5, 0.08, 1.94, 4.34));
}

fn wrench(p: &Painter, r: Rect, c: Color32) {
    use std::f32::consts::FRAC_PI_4;
    let pen = Pen::new(p, r, c);
    pen.line(&arc(
        0.64,
        0.36,
        0.17,
        -FRAC_PI_4 + 0.7,
        -FRAC_PI_4 + std::f32::consts::TAU - 0.7,
    ));
    pen.line(&[(0.52, 0.48), (0.2, 0.8)]);
}

fn terminal(p: &Painter, r: Rect, c: Color32) {
    let pen = Pen::new(p, r, c);
    pen.rounded_outline((0.1, 0.18), (0.9, 0.82), 0.08);
    pen.line(&[(0.26, 0.38), (0.4, 0.5), (0.26, 0.62)]);
    pen.line(&[(0.46, 0.64), (0.68, 0.64)]);
}

fn file_diff(p: &Painter, r: Rect, c: Color32) {
    file(p, r, c);
    let pen = Pen::new(p, r, c);
    pen.line(&[(0.51, 0.38), (0.51, 0.6)]);
    pen.line(&[(0.4, 0.49), (0.62, 0.49)]);
    pen.line(&[(0.4, 0.72), (0.62, 0.72)]);
}

fn chevron_up(p: &Painter, r: Rect, c: Color32) {
    Pen::new(p, r, c).line(&[(0.18, 0.66), (0.5, 0.32), (0.82, 0.66)]);
}

fn chevron_down(p: &Painter, r: Rect, c: Color32) {
    Pen::new(p, r, c).line(&[(0.18, 0.34), (0.5, 0.68), (0.82, 0.34)]);
}

fn expand(p: &Painter, r: Rect, c: Color32) {
    let pen = Pen::new(p, r, c);
    pen.line(&[(0.28, 0.34), (0.5, 0.14), (0.72, 0.34)]);
    pen.line(&[(0.28, 0.66), (0.5, 0.86), (0.72, 0.66)]);
}

fn collapse(p: &Painter, r: Rect, c: Color32) {
    let pen = Pen::new(p, r, c);
    pen.line(&[(0.28, 0.14), (0.5, 0.36), (0.72, 0.14)]);
    pen.line(&[(0.28, 0.86), (0.5, 0.64), (0.72, 0.86)]);
}

fn question(p: &Painter, r: Rect, c: Color32) {
    use std::f32::consts::PI;
    let pen = Pen::new(p, r, c);
    pen.ring(0.5, 0.5, 0.36);
    let mut points = arc(0.5, 0.42, 0.11, PI, 2.0 * PI + 0.7);
    points.extend([(0.5, 0.54), (0.5, 0.6)]);
    pen.line(&points);
    pen.disc(0.5, 0.7, 0.045);
}

fn shield(p: &Painter, r: Rect, c: Color32) {
    let pen = Pen::new(p, r, c);
    pen.closed(&[
        (0.5, 0.12),
        (0.82, 0.24),
        (0.78, 0.56),
        (0.5, 0.88),
        (0.22, 0.56),
        (0.18, 0.24),
    ]);
    pen.line(&[(0.36, 0.5), (0.46, 0.6), (0.65, 0.4)]);
}

fn cut(p: &Painter, r: Rect, c: Color32) {
    let pen = Pen::new(p, r, c);
    pen.ring(0.3, 0.74, 0.12);
    pen.ring(0.7, 0.74, 0.12);
    pen.line(&[(0.37, 0.64), (0.68, 0.14)]);
    pen.line(&[(0.63, 0.64), (0.32, 0.14)]);
}

fn paste(p: &Painter, r: Rect, c: Color32) {
    let pen = Pen::new(p, r, c);
    pen.rounded_outline((0.2, 0.18), (0.8, 0.88), 0.06);
    pen.outline((0.36, 0.1), (0.64, 0.26));
    pen.line(&[(0.34, 0.48), (0.66, 0.48)]);
    pen.line(&[(0.34, 0.66), (0.56, 0.66)]);
}

fn select_all(p: &Painter, r: Rect, c: Color32) {
    let pen = Pen::new(p, r, c);
    for (x, y, dx, dy) in [
        (0.14, 0.14, 1.0, 1.0),
        (0.86, 0.14, -1.0, 1.0),
        (0.14, 0.86, 1.0, -1.0),
        (0.86, 0.86, -1.0, -1.0),
    ] {
        pen.line(&[(x, y + 0.16 * dy), (x, y), (x + 0.16 * dx, y)]);
    }
    pen.line(&[(0.44, 0.14), (0.56, 0.14)]);
    pen.line(&[(0.44, 0.86), (0.56, 0.86)]);
    pen.line(&[(0.14, 0.44), (0.14, 0.56)]);
    pen.line(&[(0.86, 0.44), (0.86, 0.56)]);
    pen.filled(&[(0.36, 0.36), (0.64, 0.36), (0.64, 0.64), (0.36, 0.64)]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, pos2};

    fn shape_count(icon: Icon, side: f32) -> usize {
        let ctx = Context::default();
        let mut full = ctx.run_ui(Default::default(), |ui| {
            let rect = Rect::from_min_size(pos2(10.0, 10.0), vec2(side, side));
            if let Icon::Painted(paint) = icon {
                paint(ui.painter(), rect, Color32::WHITE);
            }
        });
        full.textures_delta.clear();
        full.shapes.len()
    }

    #[test]
    fn every_icon_is_painted_and_names_are_unique() {
        let mut names: Vec<&str> = ALL.iter().map(|(n, _)| *n).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), ALL.len());
        for (name, icon) in ALL {
            assert!(shape_count(icon, 16.0) > 0, "{name} paints nothing");
        }
    }
}
