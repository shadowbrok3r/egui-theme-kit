//! Gallery of the bundled Phosphor Regular pack.
//!
//! ```text
//! cargo run -p egui-icon-pack --example gallery --features phosphor-regular
//! cargo run -p egui-icon-pack --example gallery --features phosphor-regular -- --size 1100x720 --light
//! ```

use eframe::egui;
use egui_icon_pack::phosphor_regular::{self as svg, PACK};
use egui_icon_pack::{gallery, widgets};

struct App {
    light: bool,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("egui_icon_pack.example.bar").show(ui, |ui| {
            ui.horizontal(|ui| {
                widgets::heading(ui, svg::SQUARES_FOUR, "Phosphor Regular");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (icon, tip) = if self.light {
                        (svg::MOON, "Dark theme")
                    } else {
                        (svg::SUN, "Light theme")
                    };
                    if widgets::icon_button(ui, icon, tip).clicked() {
                        self.light = !self.light;
                        let theme = if self.light {
                            egui::Theme::Light
                        } else {
                            egui::Theme::Dark
                        };
                        ui.ctx().set_theme(theme);
                    }
                });
            });
        });
        egui::CentralPanel::default().show(ui, |ui| gallery::show(ui, &PACK));
    }
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    let size = args
        .iter()
        .position(|a| a == "--size")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.split_once('x'))
        .and_then(|(w, h)| Some([w.parse::<f32>().ok()?, h.parse::<f32>().ok()?]))
        .unwrap_or([960.0, 680.0]);
    let light = args.iter().any(|a| a == "--light");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size(size),
        ..Default::default()
    };
    eframe::run_native(
        "Icon pack gallery",
        options,
        Box::new(move |cc| {
            let mut fonts = egui::FontDefinitions::default();
            egui_icon_pack::install_fonts(&mut fonts);
            cc.egui_ctx.set_fonts(fonts);
            egui_icon_pack::install(&cc.egui_ctx);
            cc.egui_ctx.set_theme(if light {
                egui::Theme::Light
            } else {
                egui::Theme::Dark
            });
            Ok(Box::new(App { light }))
        }),
    )
}
