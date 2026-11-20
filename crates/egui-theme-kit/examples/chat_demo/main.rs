//! The kit chat widgets in one window: app bar, session list and title card, transcript and composer.

mod demo;

use eframe::egui::{self, Ui};
use egui_theme_kit::chat::UnixMs;
use egui_theme_kit::{KitConfig, PresetStyles, glass};

struct App {
    demo: demo::ChatDemo,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        egui_theme_kit::init(cc, KitConfig::default());
        let preset = PresetStyles::Graphite;
        egui_theme_kit::apply_preset(&cc.egui_ctx, preset);
        Self {
            demo: demo::ChatDemo::new(UnixMs::now().unwrap_or_default(), preset),
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        glass::poll_outcome(ctx);
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        if let Some(now) = UnixMs::now() {
            self.demo.set_now(now);
        }
        self.demo.show(ui);
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs(1));
    }
}

fn main() -> eframe::Result {
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1200.0, 820.0]),
        ..Default::default()
    };
    eframe::run_native(
        "egui-theme-kit chat demo",
        native,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}
