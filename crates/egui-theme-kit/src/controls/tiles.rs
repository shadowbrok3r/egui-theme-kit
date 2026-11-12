//! Dashboard tiles: icon chips, stat and action tiles, status chips, avatars, check rows and tile grids.

use egui::{
    Color32, CursorIcon, Rect, Response, Role, Sense, Shape, Stroke, TextStyle, Ui, Vec2, Widget,
    WidgetInfo, ecolor::Hsva, pos2, text::LayoutJob, vec2,
};

use super::labels::paint_small_spinner;
use super::{
    Icon, Metrics, Sparkline, State, Tokens, elided, focus_ring, icons, kit, medium_font_sized,
    mix, paint_box, semibold_font_sized, state_of, text_galley, whole,
};

/// Share of the tone in an icon chip's fill.
const CHIP_WASH: f32 = 0.16;
/// Opacity of a duotone icon's back layer.
const DUO_ALPHA: f32 = 0.3;
/// Icon size as a share of its chip.
const CHIP_GLYPH: f32 = 0.56;
/// Default side of an icon chip.
pub const TILE_CHIP: f32 = 36.0;
/// Gap between a tile's chip and its text.
const TILE_GAP: f32 = 12.0;
/// Inner padding of a tile.
const TILE_PAD: f32 = 12.0;
/// Stat value size.
const STAT_VALUE_PT: f32 = 17.0;
/// Stat delta text size.
const DELTA_PT: f32 = 11.0;
/// Size of a delta's trend arrow.
const TREND_ARROW: f32 = 7.0;
/// Gap between a stat value and its delta, and inside the delta.
const DELTA_GAP: f32 = 6.0;
const ARROW_GAP: f32 = 3.0;
/// Size of a stat tile's sparkline.
const STAT_SPARK: Vec2 = Vec2::new(72.0, 28.0);
/// Narrowest sparkline a stat tile draws; below it the sparkline is left out.
const MIN_SPARK_W: f32 = 32.0;
/// Action title size.
const ACTION_TITLE_PT: f32 = 13.0;
/// Rows of blurb an action tile shows.
const BLURB_ROWS: usize = 2;
/// Size of an action tile's trailing mark.
const TRAIL: f32 = 14.0;
/// Share of the tone in a hovered tile's border.
const TILE_EDGE: f32 = 0.55;
/// Share of the error hue in a danger tile's fill, at rest and hovered.
const DANGER_WASH: [f32; 2] = [0.06, 0.12];
/// Horizontal padding of a status chip.
const STATUS_PAD_X: f32 = 9.0;
/// Gap between a status chip's icon and label.
const STATUS_GAP: f32 = 5.0;
/// Status chip label size.
const STATUS_PT: f32 = 11.5;
/// Share of the tone in a status chip's fill and border.
const STATUS_FILL: f32 = 0.12;
const STATUS_EDGE: f32 = 0.4;
/// Initials size as a share of an avatar's diameter.
const AVATAR_TEXT: f32 = 0.4;
/// Share of a name hue in an avatar's fill.
const AVATAR_WASH: f32 = 0.24;
/// Saturation of a name hue.
const HUE_SAT: f32 = 0.55;
/// Brightness of a name hue on dark and on light surfaces.
const HUE_VALUE_DARK: f32 = 0.9;
const HUE_VALUE_LIGHT: f32 = 0.55;

/// A rounded square washed in a tone, holding a duotone icon in that tone.
#[derive(Clone, Copy, Debug)]
#[must_use = "call `show` or `paint` to draw the chip"]
pub struct IconChip {
    icon: Icon,
    tone: Option<Color32>,
    side: f32,
}

impl IconChip {
    pub fn new(icon: impl Into<Icon>) -> Self {
        Self {
            icon: icon.into(),
            tone: None,
            side: TILE_CHIP,
        }
    }

    /// Wash and icon colour in place of the primary role.
    pub fn tone(mut self, tone: Color32) -> Self {
        self.tone = Some(tone);
        self
    }

    pub fn side(mut self, side: f32) -> Self {
        self.side = side;
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(self.side), Sense::hover());
        if ui.is_rect_visible(rect) {
            self.paint(ui, rect);
        }
        response
    }

    /// Paints the chip centred in `rect`.
    pub fn paint(self, ui: &Ui, rect: Rect) {
        let (t, _) = kit(ui);
        let tone = self.tone.unwrap_or(t.primary);
        let chip = Rect::from_center_size(rect.center(), Vec2::splat(self.side));
        ui.painter()
            .rect_filled(chip, t.radius_md, mix(t.raised, tone, CHIP_WASH));
        let glyph = (self.side * CHIP_GLYPH).round();
        self.icon
            .paint_duo(ui, chip, glyph, tone, tone.gamma_multiply(DUO_ALPHA));
    }
}

impl Widget for IconChip {
    fn ui(self, ui: &mut Ui) -> Response {
        self.show(ui)
    }
}

/// Fill of a tile at rest.
fn tile_fill(t: &Tokens) -> Color32 {
    if t.window == t.raised {
        mix(t.raised, t.hover, 0.5)
    } else {
        t.window
    }
}

/// Which way a stat moved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Trend {
    /// Painted with an up arrow in the success role.
    Up,
    /// Painted with a down arrow in the error role.
    Down,
    /// No arrow, dim text.
    #[default]
    Flat,
}

impl Trend {
    /// Tone of a delta moving this way.
    pub fn tone(self, t: &Tokens) -> Color32 {
        match self {
            Self::Up => t.success,
            Self::Down => t.error,
            Self::Flat => t.text_dim,
        }
    }

    /// Paints a small solid arrow centred in `rect`; nothing for `Flat`.
    fn paint(self, ui: &Ui, rect: Rect, color: Color32) {
        let (tip, a, b) = match self {
            Self::Up => (rect.center_top(), rect.left_bottom(), rect.right_bottom()),
            Self::Down => (rect.center_bottom(), rect.left_top(), rect.right_top()),
            Self::Flat => return,
        };
        ui.painter()
            .add(Shape::convex_polygon(vec![tip, b, a], color, Stroke::NONE));
    }
}

/// A read-only tile: an icon chip beside a large value, a dim label, an optional delta and sparkline.
#[must_use = "call `show` to draw the tile"]
pub struct StatTile<'a> {
    icon: Icon,
    value: &'a str,
    label: &'a str,
    tone: Option<Color32>,
    delta: Option<(&'a str, Trend, Option<Color32>)>,
    spark: Option<&'a [f32]>,
    spark_tone: Option<Color32>,
    width: Option<f32>,
}

impl<'a> StatTile<'a> {
    pub fn new(icon: impl Into<Icon>, value: &'a str, label: &'a str) -> Self {
        Self {
            icon: icon.into(),
            value,
            label,
            tone: None,
            delta: None,
            spark: None,
            spark_tone: None,
            width: None,
        }
    }

    /// Chip colour in place of the primary role.
    pub fn tone(mut self, tone: Color32) -> Self {
        self.tone = Some(tone);
        self
    }

    /// Small text after the value with a trend arrow, in the trend's tone.
    pub fn delta(mut self, text: &'a str, trend: Trend) -> Self {
        self.delta = Some((text, trend, None));
        self
    }

    /// Delta colour in place of the trend's tone.
    pub fn delta_tone(mut self, tone: Color32) -> Self {
        if let Some(delta) = &mut self.delta {
            delta.2 = Some(tone);
        }
        self
    }

    /// A sparkline of recent values at the tile's right edge.
    pub fn sparkline(mut self, values: &'a [f32]) -> Self {
        self.spark = Some(values);
        self
    }

    /// Sparkline colour in place of the chip's tone.
    pub fn sparkline_tone(mut self, tone: Color32) -> Self {
        self.spark_tone = Some(tone);
        self
    }

    /// Outer width; the available width by default.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Height of a stat tile.
    pub fn height() -> f32 {
        TILE_CHIP + 2.0 * TILE_PAD
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let (t, _) = kit(ui);
        let width = self.width.unwrap_or_else(|| ui.available_width());
        let (rect, response) = ui.allocate_exact_size(vec2(width, Self::height()), Sense::hover());
        response.widget_info(|| {
            let delta = self.delta.map_or("", |d| d.0);
            let label = format!("{}: {} {}", self.label, self.value, delta);
            WidgetInfo::labeled(Role::Label, ui.is_enabled(), label.trim())
        });
        if !ui.is_rect_visible(rect) {
            return response;
        }
        paint_box(ui, rect, t.radius_md, tile_fill(&t), t.card_border);
        let tone = self.tone.unwrap_or(t.primary);
        let chip = Rect::from_min_size(rect.min + Vec2::splat(TILE_PAD), Vec2::splat(TILE_CHIP));
        IconChip::new(self.icon).tone(tone).paint(ui, chip);

        let x = chip.right() + TILE_GAP;
        let mut right = rect.right() - TILE_PAD;
        let delta = self.delta.map(|(text, trend, own)| {
            let galley = text_galley(ui, text, medium_font_sized(ui, DELTA_PT));
            (galley, trend, own.unwrap_or_else(|| trend.tone(&t)))
        });
        let delta_w = delta.as_ref().map_or(0.0, |(g, trend, _)| {
            let arrow = if *trend == Trend::Flat {
                0.0
            } else {
                TREND_ARROW + ARROW_GAP
            };
            DELTA_GAP + arrow + g.size().x
        });
        let value_font = semibold_font_sized(ui, STAT_VALUE_PT);
        let head_w = text_galley(ui, self.value, value_font.clone()).size().x + delta_w;
        if let Some(values) = self.spark {
            let spark_w = STAT_SPARK.x.min(right - x - head_w - TILE_GAP);
            if spark_w >= MIN_SPARK_W {
                let slot = Rect::from_min_size(
                    pos2(right - spark_w, rect.center().y - STAT_SPARK.y / 2.0),
                    vec2(spark_w, STAT_SPARK.y),
                );
                Sparkline::new(values, slot.size())
                    .tone(self.spark_tone.unwrap_or(tone))
                    .paint(ui, slot);
                right = slot.left() - TILE_GAP;
            }
        }
        let text_w = (right - x).max(1.0);
        let value = elided(ui, self.value, value_font, (text_w - delta_w).max(1.0));
        let small = TextStyle::Small.resolve(ui.style());
        let label = elided(ui, self.label, small, text_w);
        let top = (rect.center().y - (value.size().y + label.size().y) / 2.0).round();
        let value_size = value.size();
        let label_y = top + value_size.y;
        ui.painter().galley(pos2(x, top), value, t.text);
        if let Some((galley, trend, tone)) = delta {
            let mut dx = x + value_size.x + DELTA_GAP;
            let cy = top + value_size.y / 2.0 + 1.0;
            if trend != Trend::Flat {
                let arrow = Rect::from_center_size(
                    pos2(dx + TREND_ARROW / 2.0, cy),
                    vec2(TREND_ARROW, TREND_ARROW * 0.8),
                );
                trend.paint(ui, arrow, tone);
                dx += TREND_ARROW + ARROW_GAP;
            }
            let pos = pos2(dx, (cy - galley.size().y / 2.0).round());
            ui.painter().galley(pos, galley, tone);
        }
        ui.painter().galley(pos2(x, label_y), label, t.text_dim);
        response
    }
}

impl Widget for StatTile<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        self.show(ui)
    }
}

/// A stat tile with `icon`, a large `value` and a dim `label`.
pub fn stat_tile(ui: &mut Ui, icon: impl Into<Icon>, value: &str, label: &str) -> Response {
    StatTile::new(icon, value, label).show(ui)
}

/// What an [`ActionTile`] shows at its right edge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TileMark {
    /// A caret pointing right.
    #[default]
    Go,
    /// A spinner; the tile ignores clicks.
    Busy,
    /// A success check.
    Done,
    /// A warning triangle.
    Warning,
    /// An error cross.
    Error,
    /// Nothing.
    None,
}

/// A large clickable tile: an icon chip, a title and up to two rows of blurb.
#[must_use = "call `show` to draw the tile"]
pub struct ActionTile<'a> {
    icon: Icon,
    title: &'a str,
    blurb: &'a str,
    tone: Option<Color32>,
    width: Option<f32>,
    enabled: bool,
    danger: bool,
    selected: bool,
    mark: TileMark,
}

impl<'a> ActionTile<'a> {
    pub fn new(icon: impl Into<Icon>, title: &'a str) -> Self {
        Self {
            icon: icon.into(),
            title,
            blurb: "",
            tone: None,
            width: None,
            enabled: true,
            danger: false,
            selected: false,
            mark: TileMark::Go,
        }
    }

    /// Dim text under the title, cut with '…' after two rows.
    pub fn blurb(mut self, blurb: &'a str) -> Self {
        self.blurb = blurb;
        self
    }

    /// Chip colour in place of the primary role.
    pub fn tone(mut self, tone: Color32) -> Self {
        self.tone = Some(tone);
        self
    }

    /// Outer width; the available width by default.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Draws the tile disabled and ignores clicks when `false`.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Error-tinted tile for destructive actions.
    pub fn danger(mut self, danger: bool) -> Self {
        self.danger = danger;
        self
    }

    /// Primary wash and border, for the picked tile of a set.
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn mark(mut self, mark: TileMark) -> Self {
        self.mark = mark;
        self
    }

    /// Height of a tile with a blurb at the current text sizes.
    pub fn height(ui: &Ui) -> f32 {
        let small = ui.text_style_height(&TextStyle::Small);
        let font = semibold_font_sized(ui, ACTION_TITLE_PT);
        let title = ui.ctx().fonts_mut(|f| f.row_height(&font));
        (title + BLURB_ROWS as f32 * small).max(TILE_CHIP).ceil() + 2.0 * TILE_PAD
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        if !self.enabled {
            return ui
                .add_enabled_ui(false, |ui| self.enabled(true).show(ui))
                .inner;
        }
        let (t, _) = kit(ui);
        let width = self.width.unwrap_or_else(|| ui.available_width());
        let height = Self::height(ui);
        let busy = self.mark == TileMark::Busy;
        let sense = if busy { Sense::hover() } else { Sense::click() };
        let (rect, response) = ui.allocate_exact_size(vec2(width, height), sense);
        response.widget_info(|| {
            WidgetInfo::selected(Role::Button, ui.is_enabled(), self.selected, self.title)
        });
        if !ui.is_rect_visible(rect) {
            return response;
        }
        let (state, focused) = state_of(ui, &response);
        let tone = match (self.danger, self.tone) {
            (true, _) => t.error,
            (false, Some(tone)) => tone,
            (false, None) => t.primary,
        };
        let rest = tile_fill(&t);
        let (fill, edge) = match (self.danger, self.selected, state) {
            (true, _, State::Rest) => (mix(rest, t.error, DANGER_WASH[0]), t.card_border),
            (true, _, _) => (
                mix(rest, t.error, DANGER_WASH[1]),
                mix(t.card_border, t.error, TILE_EDGE),
            ),
            (false, true, _) => (t.primary_soft, t.primary_border),
            (false, false, State::Rest) => (rest, t.card_border),
            (false, false, State::Hovered) => (t.hover, mix(t.card_border, tone, TILE_EDGE)),
            (false, false, State::Pressed) => (t.pressed, t.primary_border),
        };
        paint_box(ui, rect, t.radius_md, fill, edge);
        let response = if ui.is_enabled() && !busy {
            response.on_hover_cursor(CursorIcon::PointingHand)
        } else {
            response
        };

        let chip = Rect::from_min_size(
            pos2(rect.left() + TILE_PAD, rect.center().y - TILE_CHIP / 2.0),
            Vec2::splat(TILE_CHIP),
        );
        let chip_tone = if ui.is_enabled() { tone } else { t.text_faint };
        IconChip::new(self.icon).tone(chip_tone).paint(ui, chip);

        let trail_w = if self.mark == TileMark::None {
            0.0
        } else {
            TRAIL + TILE_GAP
        };
        let x = chip.right() + TILE_GAP;
        let text_w = (rect.right() - TILE_PAD - trail_w - x).max(1.0);
        let title = elided(
            ui,
            self.title,
            semibold_font_sized(ui, ACTION_TITLE_PT),
            text_w,
        );
        let blurb = (!self.blurb.is_empty()).then(|| {
            let font = TextStyle::Small.resolve(ui.style());
            let mut job =
                LayoutJob::simple(self.blurb.to_owned(), font, Color32::PLACEHOLDER, text_w);
            job.wrap.max_rows = BLURB_ROWS;
            job.wrap.overflow_character = Some('…');
            ui.painter().layout_job(job)
        });
        let text_h = title.size().y + blurb.as_ref().map_or(0.0, |g| g.size().y);
        let top = (rect.center().y - text_h / 2.0).round();
        let title_h = title.size().y;
        let ink = match (ui.is_enabled(), self.danger) {
            (false, _) => t.text_faint,
            (true, true) => t.error,
            (true, false) => t.text,
        };
        ui.painter().galley(pos2(x, top), title, ink);
        if let Some(blurb) = blurb {
            ui.painter()
                .galley(pos2(x, top + title_h), blurb, t.text_dim);
        }

        let slot = Rect::from_center_size(
            pos2(rect.right() - TILE_PAD - TRAIL / 2.0, rect.center().y),
            Vec2::splat(TRAIL),
        );
        match self.mark {
            TileMark::Go => {
                let ink = if state == State::Rest {
                    t.text_faint
                } else {
                    tone
                };
                icons::CARET_RIGHT.paint(ui, slot, TRAIL, ink);
            }
            TileMark::Busy => paint_small_spinner(ui, slot, Some(tone)),
            TileMark::Done => icons::CHECK_CIRCLE.paint(ui, slot, TRAIL, t.success),
            TileMark::Warning => icons::WARNING.paint(ui, slot, TRAIL, t.warn),
            TileMark::Error => icons::ERROR.paint(ui, slot, TRAIL, t.error),
            TileMark::None => {}
        }
        if focused {
            focus_ring(ui, rect, t.radius_md, &t);
        }
        response
    }
}

impl Widget for ActionTile<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        self.show(ui)
    }
}

/// A clickable tile with `icon`, a `title` and a `blurb`.
pub fn action_tile(ui: &mut Ui, icon: impl Into<Icon>, title: &str, blurb: &str) -> Response {
    ActionTile::new(icon, title).blurb(blurb).show(ui)
}

/// A rounded chip washed in `tone`: an icon and a label.
pub fn status_chip(ui: &mut Ui, icon: impl Into<Icon>, label: &str, tone: Color32) -> Response {
    let icon = icon.into();
    let (t, m) = kit(ui);
    let galley = text_galley(ui, label, medium_font_sized(ui, STATUS_PT));
    let size = vec2(
        whole(galley.size().x + m.icon_sm + STATUS_GAP + 2.0 * STATUS_PAD_X),
        m.control_h,
    );
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    response.widget_info(|| WidgetInfo::labeled(Role::Label, ui.is_enabled(), label));
    if ui.is_rect_visible(rect) {
        let radius = (m.control_h / 2.0) as u8;
        paint_box(
            ui,
            rect,
            radius,
            mix(t.base, tone, STATUS_FILL),
            mix(t.base, tone, STATUS_EDGE),
        );
        let slot = Rect::from_min_size(
            pos2(
                rect.left() + STATUS_PAD_X,
                rect.center().y - m.icon_sm / 2.0,
            ),
            Vec2::splat(m.icon_sm),
        );
        icon.paint(ui, slot, m.icon_sm, tone);
        let pos = pos2(
            slot.right() + STATUS_GAP,
            (rect.center().y - galley.size().y / 2.0).round(),
        );
        ui.painter().galley(pos, galley, t.text);
    }
    response
}

/// Up to two initials of `name`, uppercased; a question mark for an empty name.
pub fn initials(name: &str) -> String {
    let mut words = name.split_whitespace();
    let first = words.next().and_then(|w| w.chars().next());
    let last = words.next_back().and_then(|w| w.chars().next());
    let text: String = first
        .into_iter()
        .chain(last)
        .flat_map(char::to_uppercase)
        .collect();
    if text.is_empty() {
        "?".to_owned()
    } else {
        text
    }
}

/// A hue in 0..1 that is the same for the same name on every run.
pub fn name_hue(name: &str) -> f32 {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in name.trim().to_lowercase().bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    (hash % 360) as f32 / 360.0
}

/// A colour of `hue` bright enough to read on the `base` surface.
fn hue_tone(t: &Tokens, hue: f32) -> Color32 {
    let sum = |c: Color32| c.r() as u16 + c.g() as u16 + c.b() as u16;
    let dark = sum(t.text) > sum(t.base);
    let value = if dark {
        HUE_VALUE_DARK
    } else {
        HUE_VALUE_LIGHT
    };
    Hsva::new(hue, HUE_SAT, value, 1.0).into()
}

/// A disc holding the initials of a name.
#[derive(Clone, Copy, Debug)]
#[must_use = "call `show` to draw the avatar"]
pub struct Avatar<'a> {
    name: &'a str,
    diameter: f32,
    hue_from_name: bool,
    tone: Option<Color32>,
}

impl<'a> Avatar<'a> {
    pub fn new(name: &'a str) -> Self {
        Self {
            name,
            diameter: TILE_CHIP,
            hue_from_name: false,
            tone: None,
        }
    }

    pub fn diameter(mut self, diameter: f32) -> Self {
        self.diameter = diameter.max(8.0);
        self
    }

    /// Takes a hue from the name, the same on every run, in place of the primary role.
    pub fn hue_from_name(mut self, on: bool) -> Self {
        self.hue_from_name = on;
        self
    }

    /// Colour in place of the primary role or the name hue.
    pub fn tone(mut self, tone: Color32) -> Self {
        self.tone = Some(tone);
        self
    }

    /// Fill and ink the avatar paints with.
    pub fn colors(&self, t: &Tokens) -> (Color32, Color32) {
        match (self.tone, self.hue_from_name) {
            (Some(tone), _) => (mix(t.raised, tone, AVATAR_WASH), tone),
            (None, true) => {
                let tone = hue_tone(t, name_hue(self.name));
                (mix(t.raised, tone, AVATAR_WASH), tone)
            }
            (None, false) => (t.primary_soft, t.primary),
        }
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let (t, _) = kit(ui);
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(self.diameter), Sense::hover());
        response.widget_info(|| WidgetInfo::labeled(Role::Image, ui.is_enabled(), self.name));
        if ui.is_rect_visible(rect) {
            let (fill, ink) = self.colors(&t);
            let painter = ui.painter();
            painter.circle_filled(rect.center(), self.diameter / 2.0, fill);
            let font = semibold_font_sized(ui, (self.diameter * AVATAR_TEXT).round());
            let text = text_galley(ui, &initials(self.name), font);
            painter.galley((rect.center() - text.size() / 2.0).round(), text, ink);
        }
        response
    }
}

impl Widget for Avatar<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        self.show(ui)
    }
}

/// A disc in the primary wash holding the initials of `name`.
pub fn avatar(ui: &mut Ui, name: &str, diameter: f32) -> Response {
    Avatar::new(name).diameter(diameter).show(ui)
}

/// How a yes/no check turned out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Check {
    Good,
    Warn,
    Bad,
    Unknown,
}

impl Check {
    /// `Good` for `Some(true)`, `Bad` for `Some(false)`, `Unknown` otherwise.
    pub fn from_flag(flag: Option<bool>) -> Self {
        match flag {
            Some(true) => Self::Good,
            Some(false) => Self::Bad,
            None => Self::Unknown,
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            Self::Good => icons::CHECK_CIRCLE,
            Self::Warn => icons::WARNING,
            Self::Bad => icons::ERROR,
            Self::Unknown => icons::MINUS_CIRCLE,
        }
    }

    pub fn tone(self, t: &Tokens) -> Color32 {
        match self {
            Self::Good => t.success,
            Self::Warn => t.warn,
            Self::Bad => t.error,
            Self::Unknown => t.text_faint,
        }
    }

    /// Word a screen reader hears after the row's label.
    pub fn word(self) -> &'static str {
        match self {
            Self::Good => "passed",
            Self::Warn => "warning",
            Self::Bad => "failed",
            Self::Unknown => "unknown",
        }
    }
}

/// A full-width row: a status icon, a label, and dim detail text cut at the right edge.
pub fn check_row(ui: &mut Ui, check: Check, label: &str, detail: &str) -> Response {
    let (t, m) = kit(ui);
    let width = super::fill_width(ui);
    let (rect, response) = ui.allocate_exact_size(vec2(width, m.control_h), Sense::hover());
    response.widget_info(|| {
        let text = format!("{label} ({}) {detail}", check.word());
        WidgetInfo::labeled(Role::Label, ui.is_enabled(), text.trim())
    });
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let slot = Rect::from_min_size(
        pos2(rect.left(), rect.center().y - m.icon_md / 2.0),
        Vec2::splat(m.icon_md),
    );
    check.icon().paint(ui, slot, m.icon_md, check.tone(&t));
    let body = TextStyle::Body.resolve(ui.style());
    let x = slot.right() + m.gap.x;
    let label = elided(ui, label, body, (rect.right() - x).max(1.0));
    let label_w = label.size().x;
    ui.painter().galley(
        pos2(x, (rect.center().y - label.size().y / 2.0).round()),
        label,
        t.text,
    );
    let room = rect.right() - (x + label_w + m.gap.x);
    if !detail.is_empty() && room > m.gap.x {
        let small = TextStyle::Small.resolve(ui.style());
        let detail = elided(ui, detail, small, room);
        let pos = pos2(
            rect.right() - detail.size().x,
            (rect.center().y - detail.size().y / 2.0).round(),
        );
        ui.painter().galley(pos, detail, t.text_dim);
    }
    response
}

/// Columns and tile width that fit `available` with tiles at least `min_w` wide and `gap` between.
pub fn tile_columns(available: f32, min_w: f32, gap: f32) -> (usize, f32) {
    let columns = (((available + gap) / (min_w + gap)).floor() as usize).max(1);
    let width = ((available - gap * (columns - 1) as f32) / columns as f32).floor();
    (columns, width.max(1.0))
}

/// Lays `count` tiles out in rows; `add` draws tile `index` at the given width.
pub fn tile_grid(ui: &mut Ui, min_w: f32, count: usize, mut add: impl FnMut(&mut Ui, usize, f32)) {
    let m = Metrics::get(ui.ctx());
    let (columns, width) = tile_columns(ui.available_width(), min_w, m.gap.x);
    let mut index = 0;
    while index < count {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = m.gap;
            for _ in 0..columns {
                if index >= count {
                    break;
                }
                add(ui, index, width);
                index += 1;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials_take_the_first_and_last_word() {
        assert_eq!(initials("Jane Doe"), "JD");
        assert_eq!(initials("jane van doe"), "JD");
        assert_eq!(initials("Cher"), "C");
        assert_eq!(initials("  "), "?");
    }

    #[test]
    fn tiles_fill_the_row_and_never_drop_below_one_column() {
        assert_eq!(tile_columns(1000.0, 240.0, 10.0), (4, 242.0));
        assert_eq!(tile_columns(500.0, 240.0, 10.0), (2, 245.0));
        assert_eq!(tile_columns(100.0, 240.0, 10.0), (1, 100.0));
    }

    #[test]
    fn a_flag_maps_to_its_check() {
        assert_eq!(Check::from_flag(Some(true)), Check::Good);
        assert_eq!(Check::from_flag(Some(false)), Check::Bad);
        assert_eq!(Check::from_flag(None), Check::Unknown);
    }

    #[test]
    fn a_name_hue_is_stable_and_spreads_names_apart() {
        assert_eq!(name_hue("workstation-01"), name_hue("  Workstation-01 "));
        assert_ne!(name_hue("alpha"), name_hue("beta"));
        let hue = name_hue("gamma");
        assert!((0.0..1.0).contains(&hue));
        let t = Tokens::derive(&egui::Style::default(), Color32::GREEN, Color32::RED);
        let a = Avatar::new("alpha").hue_from_name(true).colors(&t);
        let b = Avatar::new("beta").hue_from_name(true).colors(&t);
        assert_ne!(a, b);
        assert_eq!(Avatar::new("alpha").colors(&t), (t.primary_soft, t.primary));
    }
}
