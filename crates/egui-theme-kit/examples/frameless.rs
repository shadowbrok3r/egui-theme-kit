//! An app with the app-drawn title bar: `cargo run -p egui-theme-kit --example frameless --features title-bar`.
//!
//! Pass `--system-titlebar` (or set `EGUI_THEME_KIT_SYSTEM_TITLEBAR=1`) to keep the system title bar
//! for one launch; View > Custom title bar switches live and is remembered between launches.

use std::sync::Arc;

use eframe::egui::{self, CentralPanel, Panel, RichText, class::HasClasses as _};
use egui_theme_kit::{
    KitConfig,
    chrome::{self, Chrome},
    colors,
    controls::{self, IconBox, MenuButton, icons},
    debug_panel, persist,
    picker::{self, PickerState},
    store::JsonDirStore,
    title_bar::{Side, TitleBar, window_edges, windows::client_area},
    widget_theme,
};

const APP_ID: &str = "egui-theme-kit-frameless-example";

fn main() -> eframe::Result {
    let chrome = chrome::resolve_from_process(APP_ID);
    eprintln!("frameless example: {chrome:?}");
    let viewport = chrome::viewport(
        egui::ViewportBuilder::default()
            .with_inner_size([960.0, 640.0])
            .with_app_id(APP_ID)
            .with_title("Frameless example"),
        &chrome,
    );
    eframe::run_native(
        "Frameless example",
        eframe::NativeOptions {
            viewport,
            ..Default::default()
        },
        Box::new(move |cc| {
            if !client_area::install(cc) {
                eprintln!("frameless example: the client-area subclass was not installed");
            }
            let store: Arc<JsonDirStore> = Arc::new(open_store());
            egui_theme_kit::init(cc, KitConfig::default());
            persist::apply_startup_from_store(&cc.egui_ctx, &*store, None);
            debug_panel::set_store(&cc.egui_ctx, store.clone());
            Ok(Box::new(App::new(chrome, store)))
        }),
    )
}

fn open_store() -> JsonDirStore {
    let dir =
        JsonDirStore::default_dir(APP_ID).unwrap_or_else(|| std::env::temp_dir().join(APP_ID));
    JsonDirStore::open(dir).expect("open the theme directory")
}

struct App {
    chrome: Chrome,
    store: Arc<JsonDirStore>,
    picker: PickerState,
    note: String,
    notes_saved: u32,
    save_error: Option<String>,
    about_open: bool,
}

impl App {
    fn new(chrome: Chrome, store: Arc<JsonDirStore>) -> Self {
        Self {
            chrome,
            store,
            picker: PickerState::default(),
            note: "Type here".to_owned(),
            notes_saved: 0,
            save_error: None,
            about_open: false,
        }
    }

    fn title_bar(&mut self, ui: &mut egui::Ui) {
        let frameless = self.chrome.frameless;
        let mut bar = TitleBar::new(
            "example.title_bar",
            if frameless { "Frameless example" } else { "" },
        )
        .frameless(frameless);
        if frameless {
            bar = bar.mark(icons::GRID);
        }
        let _ = bar.show(ui, |ui, side| match side {
            Side::Left => self.menus(ui),
            Side::Right => self.status(ui),
        });
    }

    fn menus(&mut self, ui: &mut egui::Ui) {
        let _ = MenuButton::new("File").show(ui, |ui| {
            if controls::menu_item(ui, icons::FILE, "Save note").clicked() {
                self.notes_saved += 1;
            }
            if controls::menu_item(ui, icons::FOLDER, "Open").clicked() {
                self.note = "Opened".to_owned();
            }
            controls::menu_separator(ui);
            if controls::menu_item(ui, icons::X, "Quit").clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
        let _ = MenuButton::new("View").keep_open(true).show(ui, |ui| {
            let _ = chrome::toggle_row(ui, &mut self.chrome, "Custom title bar");
            controls::menu_separator(ui);
            if controls::menu_item(ui, icons::GEAR, "Theme settings").clicked() {
                self.picker.open = true;
            }
        });
        // A stock egui menu button sits in the slot the same way, with the kit's ghost class.
        let help = egui::Button::new("Help").with_class(widget_theme::GHOST);
        egui::containers::menu::MenuButton::from_button(help).ui(ui, |ui| {
            if ui.button("About").clicked() {
                self.about_open = true;
            }
        });
    }

    fn status(&mut self, ui: &mut egui::Ui) {
        if controls::icon_button(
            ui,
            icons::GEAR,
            IconBox::Md,
            self.picker.open,
            "Theme settings",
        )
        .clicked()
        {
            self.picker.open = !self.picker.open;
        }
        let label = if self.chrome.frameless {
            "Custom bar"
        } else {
            "System bar"
        };
        let _ = controls::status_label(ui, icons::CHECK_CIRCLE, label, colors::success(ui));
    }

    fn content(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        controls::heading(ui, icons::GRID, "Frameless window");
        ui.add_space(4.0);
        ui.label(RichText::new("The strip above is drawn by the app: drag it to move, double-click to maximize, use the edges to resize.").weak());
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if ui
                .add(egui_theme_kit::primary_button("Save note"))
                .clicked()
            {
                self.notes_saved += 1;
            }
            ui.label(format!("Saved {} times", self.notes_saved));
        });
        ui.add_space(8.0);
        ui.add(egui::TextEdit::singleline(&mut self.note).desired_width(280.0));
        ui.add_space(12.0);
        let info = ui.ctx().input(|i| {
            let v = i.viewport();
            format!(
                "inner {:?}  outer {:?}  scale {:.2}  maximized {:?}  fullscreen {:?}",
                v.inner_rect.map(|r| r.size()),
                v.outer_rect.map(|r| r.min),
                i.pixels_per_point,
                v.maximized,
                v.fullscreen,
            )
        });
        ui.label(RichText::new(info).monospace().weak());
        if let Some(error) = &self.save_error {
            ui.add_space(8.0);
            ui.colored_label(colors::error(ui), error);
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.title_bar(ui);
        Panel::bottom("example.status").show(ui, |ui| {
            ui.label(format!("Window chrome: {:?}", self.chrome));
        });
        CentralPanel::default().show(ui, |ui| self.content(ui));
        picker::window(&ctx, &mut self.picker, &*self.store);
        egui::Window::new("About")
            .open(&mut self.about_open)
            .show(&ctx, |ui| {
                ui.label("A window inside the frameless window.");
            });
        if let Some(error) = chrome::take_save_error(&ctx) {
            self.save_error = Some(format!("Couldn't save the title bar setting: {error}"));
        }
        if self.chrome.frameless {
            window_edges(ui);
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.store.flush();
    }
}
