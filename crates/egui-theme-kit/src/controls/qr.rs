//! QR codes painted as dark modules on a light tile whatever the theme.

use std::sync::Arc;

use egui::{
    Color32, Id, Rect, Response, Role, Sense, TextStyle, Ui, Vec2, Widget, WidgetInfo, pos2, vec2,
};
use qrcode::{Color as Module, EcLevel};

use super::{elided, icons, kit, paint_box};

/// Default side of a QR code tile.
pub const QR_SIDE: f32 = 160.0;
/// Light modules the code keeps clear around itself on every side.
pub const QUIET_ZONE: usize = 4;
/// Gap between the tile and its caption.
const CAPTION_GAP: f32 = 6.0;
/// Corner radius of the light tile.
const TILE_RADIUS: u8 = 4;
/// Size of the icon on a placeholder tile.
const PLACEHOLDER_ICON: f32 = 24.0;
/// Dark and light module colours.
const DARK: Color32 = Color32::BLACK;
const LIGHT: Color32 = Color32::WHITE;

/// How much damage a QR code survives; higher levels hold less data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum QrEcc {
    /// About 7 percent.
    Low,
    /// About 15 percent.
    #[default]
    Medium,
    /// About 25 percent.
    Quartile,
    /// About 30 percent.
    High,
}

impl QrEcc {
    fn level(self) -> EcLevel {
        match self {
            Self::Low => EcLevel::L,
            Self::Medium => EcLevel::M,
            Self::Quartile => EcLevel::Q,
            Self::High => EcLevel::H,
        }
    }
}

/// The square grid of modules a string encodes to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QrMatrix {
    width: usize,
    dark: Vec<bool>,
}

impl QrMatrix {
    /// Encodes `data`, or `None` when it is too long for any QR version at `ecc`.
    pub fn encode(data: &str, ecc: QrEcc) -> Option<Self> {
        let code =
            qrcode::QrCode::with_error_correction_level(data.as_bytes(), ecc.level()).ok()?;
        let width = code.width();
        let dark = code
            .to_colors()
            .into_iter()
            .map(|m| m == Module::Dark)
            .collect();
        Some(Self { width, dark })
    }

    /// Modules per side, without the quiet zone.
    pub fn width(&self) -> usize {
        self.width
    }

    /// Whether the module at column `x`, row `y` is dark; false outside the grid.
    pub fn is_dark(&self, x: usize, y: usize) -> bool {
        x < self.width && y < self.width && self.dark[y * self.width + x]
    }

    /// Runs of dark modules as (row, first column, length).
    pub fn dark_runs(&self) -> Vec<(usize, usize, usize)> {
        let mut runs = Vec::new();
        for y in 0..self.width {
            let mut x = 0;
            while x < self.width {
                if self.is_dark(x, y) {
                    let start = x;
                    while x < self.width && self.is_dark(x, y) {
                        x += 1;
                    }
                    runs.push((y, start, x - start));
                } else {
                    x += 1;
                }
            }
        }
        runs
    }
}

/// The matrix for `data`, encoded once and kept until the data changes.
fn cached(ui: &Ui, id: Id, data: &str, ecc: QrEcc) -> Option<Arc<QrMatrix>> {
    let key = Id::unique((data, ecc)).value();
    if let Some((at, matrix)) = ui.data(|d| d.get_temp::<(u64, Option<Arc<QrMatrix>>)>(id))
        && at == key
    {
        return matrix;
    }
    let matrix = QrMatrix::encode(data, ecc).map(Arc::new);
    ui.data_mut(|d| d.insert_temp(id, (key, matrix.clone())));
    matrix
}

/// A QR code for a string, with an optional caption under it.
#[must_use = "call `show` to draw the code"]
pub struct QrCode<'a> {
    data: &'a str,
    side: f32,
    quiet: usize,
    ecc: QrEcc,
    caption: &'a str,
    label: &'a str,
    error_text: &'a str,
}

impl<'a> QrCode<'a> {
    pub fn new(data: &'a str) -> Self {
        Self {
            data,
            side: QR_SIDE,
            quiet: QUIET_ZONE,
            ecc: QrEcc::Medium,
            caption: "",
            label: "QR code",
            error_text: "Too long for a QR code",
        }
    }

    /// Side of the tile, quiet zone included.
    pub fn size(mut self, side: f32) -> Self {
        self.side = side.max(16.0);
        self
    }

    /// Light modules around the code; four by default.
    pub fn quiet_zone(mut self, modules: usize) -> Self {
        self.quiet = modules;
        self
    }

    pub fn ecc(mut self, ecc: QrEcc) -> Self {
        self.ecc = ecc;
        self
    }

    /// Dim text under the tile.
    pub fn caption(mut self, caption: &'a str) -> Self {
        self.caption = caption;
        self
    }

    /// Accessible name; "QR code" by default.
    pub fn label(mut self, label: &'a str) -> Self {
        self.label = label;
        self
    }

    /// Text a placeholder tile shows when the data does not fit.
    pub fn error_text(mut self, text: &'a str) -> Self {
        self.error_text = text;
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let (t, _) = kit(ui);
        let small = TextStyle::Small.resolve(ui.style());
        let caption =
            (!self.caption.is_empty()).then(|| elided(ui, self.caption, small.clone(), self.side));
        let caption_h = caption.as_ref().map_or(0.0, |g| CAPTION_GAP + g.size().y);
        let (rect, response) =
            ui.allocate_exact_size(vec2(self.side, self.side + caption_h), Sense::hover());
        let matrix = cached(ui, response.id, self.data, self.ecc);
        response.widget_info(|| {
            let label = if matrix.is_some() {
                self.label.to_owned()
            } else {
                format!("{}: {}", self.label, self.error_text)
            };
            WidgetInfo::labeled(Role::Image, ui.is_enabled(), label)
        });
        if !ui.is_rect_visible(rect) {
            return response;
        }
        let tile = Rect::from_min_size(rect.min, Vec2::splat(self.side));
        match &matrix {
            Some(matrix) => self.paint_matrix(ui, tile, matrix),
            None => {
                paint_box(ui, tile, t.radius_sm, t.field, t.error);
                let slot = Rect::from_center_size(
                    tile.center() - vec2(0.0, PLACEHOLDER_ICON / 2.0),
                    Vec2::splat(PLACEHOLDER_ICON),
                );
                icons::ERROR.paint(ui, slot, PLACEHOLDER_ICON, t.error);
                let text = elided(ui, self.error_text, small, tile.width() - 16.0);
                let pos = pos2(
                    (tile.center().x - text.size().x / 2.0).round(),
                    (slot.bottom() + CAPTION_GAP).round(),
                );
                ui.painter().galley(pos, text, t.text_dim);
            }
        }
        if let Some(caption) = caption {
            let pos = pos2(
                (rect.center().x - caption.size().x / 2.0).round(),
                tile.bottom() + CAPTION_GAP,
            );
            ui.painter().galley(pos, caption, t.text_dim);
        }
        response
    }

    /// Paints the light tile and the dark modules, each module a whole number of pixels.
    fn paint_matrix(&self, ui: &Ui, tile: Rect, matrix: &QrMatrix) {
        let ppp = ui.pixels_per_point();
        let modules = (matrix.width() + 2 * self.quiet) as f32;
        let module_px = (tile.width() * ppp / modules).floor().max(1.0);
        let module = module_px / ppp;
        let drawn = module * modules;
        let origin = pos2(
            ((tile.center().x - drawn / 2.0) * ppp).round() / ppp,
            ((tile.center().y - drawn / 2.0) * ppp).round() / ppp,
        );
        let painter = ui.painter();
        painter.rect_filled(tile, TILE_RADIUS, LIGHT);
        let quiet = self.quiet as f32 * module;
        for (y, x, len) in matrix.dark_runs() {
            let min = origin + vec2(quiet + x as f32 * module, quiet + y as f32 * module);
            painter.rect_filled(
                Rect::from_min_size(min, vec2(len as f32 * module, module)),
                0.0,
                DARK,
            );
        }
    }
}

impl Widget for QrCode<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        self.show(ui)
    }
}

/// A QR code for `data`, `side` points square.
pub fn qr_code(ui: &mut Ui, data: &str, side: f32) -> Response {
    QrCode::new(data).size(side).show(ui)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_string_encodes_and_an_oversized_one_does_not() {
        let matrix = QrMatrix::encode("pair:ABCD-EFGH", QrEcc::Medium).expect("encodes");
        assert_eq!(matrix.width(), 21);
        assert!(matrix.is_dark(0, 0), "finder pattern corner");
        assert!(!matrix.is_dark(99, 0));
        let huge = "x".repeat(5000);
        assert!(QrMatrix::encode(&huge, QrEcc::High).is_none());
    }

    #[test]
    fn dark_runs_cover_every_dark_module_once() {
        let matrix = QrMatrix::encode("https://example.com", QrEcc::Low).expect("encodes");
        let from_runs: usize = matrix.dark_runs().iter().map(|r| r.2).sum();
        let counted = (0..matrix.width())
            .flat_map(|y| (0..matrix.width()).map(move |x| (x, y)))
            .filter(|(x, y)| matrix.is_dark(*x, *y))
            .count();
        assert_eq!(from_runs, counted);
    }
}
