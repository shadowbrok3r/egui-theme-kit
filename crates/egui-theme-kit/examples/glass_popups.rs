//! Popups over dense text under a glass preset: `cargo run --example glass_popups -- --preset frost`.

use eframe::egui::{self, CentralPanel, ComboBox, Context, Ui, Window};
use egui_theme_kit::{KitConfig, glass};

const LINE: &str =
    "The quick brown fox jumps over the lazy dog while the five boxing wizards jump quickly";

struct App {
    mode: usize,
    inner_mode: usize,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut cfg = KitConfig::default().parse_cli(false);
        if let Some(preset) = preset_arg() {
            cfg = cfg.default_preset(preset);
        }
        egui_theme_kit::init(cc, cfg);
        Self {
            mode: 1,
            inner_mode: 0,
        }
    }
}

fn preset_arg() -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--preset" {
            return args.next();
        }
        if let Some(value) = arg.strip_prefix("--preset=") {
            return Some(value.to_owned());
        }
    }
    None
}

fn modes(ui: &mut Ui, id: &str, mode: &mut usize) {
    ComboBox::from_id_salt(id)
        .selected_text(["Alpha", "Beta", "Gamma", "Delta"][*mode])
        .show_ui(ui, |ui| {
            for (i, name) in ["Alpha", "Beta", "Gamma", "Delta"].into_iter().enumerate() {
                ui.selectable_value(mode, i, name);
            }
        });
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        glass::poll_outcome(ctx);
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        glass::frost_open_windows(&ctx);

        CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.menu_button("Menu", |ui| {
                    let _ = ui.button("First entry");
                    let _ = ui.button("Second entry");
                    ui.menu_button("Submenu", |ui| {
                        let _ = ui.button("Nested entry");
                    });
                });
                modes(ui, "combo", &mut self.mode);
                ui.button("Hover me")
                    .on_hover_text("A tooltip over the text behind it");
            });
            ui.separator();
            for i in 0..34 {
                ui.label(format!("{i:02} {LINE}"));
            }
        });

        Window::new("Floating window")
            .default_pos([520.0, 140.0])
            .show(&ctx, |ui| {
                for i in 0..6 {
                    ui.label(format!("{i} window text behind the popup {i}"));
                }
                modes(ui, "inner", &mut self.inner_mode);
                for i in 0..6 {
                    ui.label(format!("{i} window text under the combo box {i}"));
                }
            });
    }

    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        if let Some(gl) = gl {
            glass::shutdown(gl);
        }
    }
}

fn main() -> eframe::Result {
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1000.0, 720.0]),
        persist_window: false,
        ..Default::default()
    };
    eframe::run_native(
        "glass popups",
        native,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}
