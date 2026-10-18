//! A theme store that opens after the window: `cargo run --example late_store -- --delay 3`.
//!
//! The backend "database" becomes available after the delay and holds a Nebula theme. The kit applies it
//! on its own unless the theme was changed first with the buttons.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui::{self, CentralPanel, Context, RichText, Ui};
use egui_theme_kit::{
    KitConfig, debug_panel,
    presets::{self, PresetStyles, apply_preset},
    saved::SavedTheme,
    startup::{self, StorePoll},
    store::{CachedStore, StoreOp, StoreSnapshot, ThemeBackend, ThemeStore},
};

/// Rows of the pretend database, shared with the thread that opens it.
type Rows = Arc<Mutex<StoreSnapshot>>;

struct Backend(Rows);

impl ThemeBackend for Backend {
    fn load(&mut self) -> Result<StoreSnapshot, String> {
        Ok(self.0.lock().unwrap().clone())
    }

    fn apply(&mut self, op: &StoreOp) -> Result<(), String> {
        if let StoreOp::SaveActive(theme) = op {
            self.0.lock().unwrap().active = Some(theme.clone());
        }
        Ok(())
    }
}

struct App {
    store: Arc<CachedStore>,
    rows: Rows,
    log: Vec<String>,
    last: Option<StorePoll>,
}

fn delay_arg() -> f32 {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--delay" {
            return args.next().and_then(|v| v.parse().ok()).unwrap_or(3.0);
        }
    }
    3.0
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let rows: Rows = Arc::default();
        let store = Arc::new(CachedStore::open_lazy(Backend(rows.clone())));
        egui_theme_kit::init(cc, KitConfig::new().parse_cli(false).store(store.clone()));
        let delay = Duration::from_secs_f32(delay_arg());
        let (opened, shared) = (store.clone(), rows.clone());
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            let scratch = Context::default();
            apply_preset(&scratch, PresetStyles::Nebula);
            shared.lock().unwrap().active = Some(SavedTheme::current(&scratch));
            opened.reload();
        });
        Self {
            store,
            rows,
            log: vec![format!("window open, store opens in {delay:?}")],
            last: None,
        }
    }

    fn status(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let ready = self.store.is_ready();
        ui.label(RichText::new(format!("store ready: {ready}")).strong());
        ui.label(format!(
            "theme: {}",
            presets::installed_preset(&ctx).as_str()
        ));
        ui.label(format!("epoch: {}", startup::theme_epoch(&ctx)));
        ui.label(format!(
            "stored active: {}",
            self.store
                .load_active()
                .map_or("none".to_owned(), |t| preset_name(&t))
        ));
        ui.label(format!("database holds: {}", {
            let rows = self.rows.lock().unwrap();
            rows.active.as_ref().map_or("none".to_owned(), preset_name)
        }));
    }
}

fn preset_name(theme: &SavedTheme) -> String {
    let ctx = Context::default();
    presets::apply_saved_theme(&ctx, theme.clone());
    presets::installed_preset(&ctx).as_str().to_owned()
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        CentralPanel::default().show(ui, |ui| {
            ui.heading("Late theme store");
            self.status(ui);
            ui.separator();
            if ui.button("Choose Noir now").clicked() {
                apply_preset(&ctx, PresetStyles::Noir);
                self.log.push("the user chose Noir".to_owned());
            }
            if ui.button("Poll now").clicked() {
                let outcome = startup::poll_store(&ctx);
                self.log.push(format!("poll_store: {outcome:?}"));
                self.last = Some(outcome);
            }
            ui.separator();
            for line in &self.log {
                ui.label(line);
            }
        });
        debug_panel::show(&ctx);
    }
}

fn main() -> eframe::Result {
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([520.0, 360.0]),
        persist_window: false,
        ..Default::default()
    };
    eframe::run_native(
        "late store",
        native,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}
