//! The preset gallery: grouped cards with a live miniature of each preset.

use std::collections::HashMap;

use eframe::egui::{
    Align2, Context, CornerRadius, FontId, Rect, RichText, Role, Sense, Stroke, StrokeKind,
    TextEdit, Ui, WidgetInfo, pos2, vec2,
};

use super::{
    label,
    preview::{self, PREVIEW_HEIGHT, Preview},
    state::{Origin, PickerState},
};
use crate::{
    fonts,
    glass::GlassParams,
    presets::{self, PresetDef},
    store::{StartupTheme, ThemeStore},
    tokens::Tokens,
};

/// Narrowest a card gets before the grid drops a column.
const MIN_CARD_W: f32 = 176.0;
/// Height of the name row under a miniature.
const LABEL_H: f32 = 22.0;
const NAME_PT: f32 = 12.0;
const TAG_PT: f32 = 10.0;

/// Search and miniature caches for an embedded preset browser.
///
/// This is the same gallery used by the full theme picker. Selection is returned
/// to the host, so an app can keep its own persistence, fonts and touch metrics.
#[derive(Clone, Default)]
pub struct PresetGallery {
    search: String,
    card_data: HashMap<&'static str, (GlassParams, f32)>,
}

impl PresetGallery {
    /// Draw previews without changing the live theme. Apply and save the returned
    /// preset through the host's appearance settings. The host owns scrolling.
    pub fn show(&mut self, ui: &mut Ui, selected: Option<&str>) -> Option<PresetDef> {
        search(ui, &mut self.search, "Search presets");
        let live = Tokens::get(ui.ctx());
        cards(
            ui,
            &self.search,
            &mut self.card_data,
            &live,
            |def| {
                if selected == Some(def.id) {
                    Cue::Active
                } else {
                    Cue::None
                }
            },
            None,
        )
        .map(|(def, _)| def)
    }
}

pub(super) fn show(ui: &mut Ui, state: &mut PickerState, store: &dyn ThemeStore) {
    let ctx = ui.ctx().clone();
    let hint = label(state, "search", "Search presets");
    search(ui, &mut state.search, &hint);
    let live = Tokens::get(&ctx);
    let installed = presets::installed_preset_id(&ctx);
    let origin = state.origin().cloned();
    let apply = label(state, "apply", "Apply");
    let startup = label(state, "startup", "Start the app with this");
    let picked = cards(
        ui,
        &state.search,
        &mut state.card_data,
        &live,
        |def| {
            if installed == Some(def.id) {
                Cue::Active
            } else if matches!(&origin, Some(Origin::Preset(id)) if *id == def.id) {
                Cue::Edited
            } else {
                Cue::None
            }
        },
        Some((&apply, &startup)),
    );
    match picked {
        Some((def, Pick::Apply)) => {
            state.apply_preset(&ctx, def);
        }
        Some((def, Pick::Startup)) => {
            state.set_startup(store, StartupTheme::Preset(def.id.to_owned()));
        }
        None => {}
    }
}

fn search(ui: &mut Ui, search: &mut String, hint: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.add(
            TextEdit::singleline(search)
                .hint_text(hint)
                .desired_width(ui.available_width().min(280.0)),
        );
        if !search.is_empty() && ui.small_button("Clear").clicked() {
            search.clear();
        }
    });
    ui.add_space(ui.spacing().item_spacing.y);
}

fn cards(
    ui: &mut Ui,
    search: &str,
    card_data: &mut HashMap<&'static str, (GlassParams, f32)>,
    live: &Tokens,
    cue: impl Fn(&PresetDef) -> Cue,
    menu: Option<(&str, &str)>,
) -> Option<(PresetDef, Pick)> {
    let query = search.trim().to_lowercase();
    let mut any = false;
    let mut picked = None;
    for group in presets::grouped_presets() {
        let defs: Vec<PresetDef> = group
            .presets
            .into_iter()
            .filter(|def| matches(def, &query))
            .collect();
        if defs.is_empty() {
            continue;
        }
        any = true;
        ui.label(RichText::new(group.title).weak().small());
        let columns = columns(ui);
        let width = card_width(ui, columns);
        for row in defs.chunks(columns) {
            ui.horizontal(|ui| {
                for def in row {
                    if let Some(pick) = card(ui, card_data, def, width, cue(def), live, menu) {
                        picked = Some((*def, pick));
                    }
                }
            });
        }
        ui.add_space(ui.spacing().item_spacing.y);
    }
    if !any {
        ui.label(RichText::new(format!("No presets match \"{}\".", search.trim())).weak());
    }
    picked
}

pub(super) fn matches(def: &PresetDef, query: &str) -> bool {
    query.is_empty()
        || [def.name, def.id, def.group, def.description]
            .iter()
            .any(|text| text.to_lowercase().contains(query))
}

/// Height of the name row, grown with the text size.
fn label_height(ctx: &Context) -> f32 {
    LABEL_H.max(NAME_PT * fonts::text_scale(ctx).factor() + 10.0)
}

fn columns(ui: &Ui) -> usize {
    let gap = ui.spacing().item_spacing.x;
    (((ui.available_width() + gap) / (MIN_CARD_W + gap)).floor() as usize).max(1)
}

fn card_width(ui: &Ui, columns: usize) -> f32 {
    let gap = ui.spacing().item_spacing.x;
    ((ui.available_width() - gap * (columns - 1) as f32) / columns as f32)
        .floor()
        .max(1.0)
}

/// What a card says about its preset's relation to the live theme.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Cue {
    None,
    /// The live theme is exactly this preset.
    Active,
    /// The live theme started from this preset and was edited.
    Edited,
}

enum Pick {
    Apply,
    Startup,
}

/// One preset card; returns what the user chose on it.
fn card(
    ui: &mut Ui,
    card_data: &mut HashMap<&'static str, (GlassParams, f32)>,
    def: &PresetDef,
    width: f32,
    cue: Cue,
    live: &Tokens,
    menu: Option<(&str, &str)>,
) -> Option<Pick> {
    let ctx = ui.ctx().clone();
    let label_h = label_height(&ctx);
    let size = vec2(width, PREVIEW_HEIGHT + label_h);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, def.name));

    let tokens = presets::preset_tokens(&ctx, def);
    let (glass, opacity) = *card_data.entry(def.id).or_insert_with(|| {
        let theme = def.theme();
        (def.glass(), preview::window_opacity(&theme))
    });
    let mini = Rect::from_min_size(rect.min, vec2(width, PREVIEW_HEIGHT));
    if ui.is_rect_visible(rect) {
        preview::paint(ui, mini, &Preview::new(&tokens, glass, opacity, def.name));
        paint_frame(ui, mini, &response, cue, &tokens, live);
        paint_label(ui, rect, label_h, def, cue, live);
    }

    let response = response.on_hover_text(def.description);
    let mut pick = response.clicked().then_some(Pick::Apply);
    if let Some((apply, startup)) = menu {
        response.context_menu(|ui| {
            if ui.button(apply).clicked() {
                pick = Some(Pick::Apply);
                ui.close();
            }
            if ui.button(startup).clicked() {
                pick = Some(Pick::Startup);
                ui.close();
            }
        });
    }
    pick
}

fn paint_frame(
    ui: &Ui,
    mini: Rect,
    response: &eframe::egui::Response,
    cue: Cue,
    tokens: &Tokens,
    live: &Tokens,
) {
    let radius = CornerRadius::from(tokens.radius_md.min(10));
    let stroke = match (cue, response.hovered()) {
        (Cue::Active, _) => Stroke::new(2.0, live.primary),
        (Cue::Edited, _) => Stroke::new(1.5, live.primary.gamma_multiply(0.6)),
        (Cue::None, true) => Stroke::new(1.5, live.text_dim),
        (Cue::None, false) => Stroke::new(1.0, live.card_border),
    };
    ui.painter()
        .rect_stroke(mini, radius, stroke, StrokeKind::Inside);
}

fn paint_label(ui: &Ui, rect: Rect, label_h: f32, def: &PresetDef, cue: Cue, live: &Tokens) {
    let y = rect.bottom() - label_h / 2.0 + 2.0;
    let name_color = live.text;
    let painter = ui.painter();
    painter.text(
        pos2(rect.left() + 3.0, y),
        Align2::LEFT_CENTER,
        def.name,
        fonts::semibold(NAME_PT),
        name_color,
    );
    let tag = match cue {
        Cue::Active => Some(("Active", live.primary)),
        Cue::Edited => Some(("Edited", live.text_dim)),
        Cue::None => None,
    };
    if let Some((text, color)) = tag {
        painter.text(
            pos2(rect.right() - 3.0, y),
            Align2::RIGHT_CENTER,
            text,
            FontId::proportional(TAG_PT),
            color,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{CentralPanel, Event, FullOutput, PointerButton, Pos2, RawInput, Shape};

    fn draw(
        ctx: &Context,
        gallery: &mut PresetGallery,
        width: f32,
        events: Vec<Event>,
    ) -> (FullOutput, Option<PresetDef>, Rect) {
        let mut picked = None;
        let mut bounds = Rect::NOTHING;
        let output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 4000.0))),
                events,
                ..Default::default()
            },
            |ui| {
                CentralPanel::default().show(ui, |ui| {
                    picked = gallery.show(ui, Some("studio"));
                    bounds = ui.min_rect();
                });
            },
        );
        (output, picked, bounds)
    }

    fn text_center(shape: &Shape, name: &str) -> Option<Pos2> {
        match shape {
            Shape::Text(text) if text.galley.job.text == name => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            Shape::Vec(shapes) => shapes.iter().find_map(|shape| text_center(shape, name)),
            _ => None,
        }
    }

    #[test]
    fn embedded_gallery_reflows_and_searches_without_overflow() {
        for width in [240.0, 320.0, 412.0, 800.0] {
            let ctx = Context::default();
            crate::appearance::ThemeConfig {
                text_scale: 1.3,
                ..Default::default()
            }
            .apply(&ctx, crate::appearance::Density::Touch);
            let mut gallery = PresetGallery::default();
            for _ in 0..2 {
                let (out, _, bounds) = draw(&ctx, &mut gallery, width, vec![]);
                assert!(bounds.right() <= width + 1.0, "{width}: {bounds:?}");
                out.drop_without_applying_deltas();
            }
            assert_eq!(gallery.card_data.len(), presets::builtin_presets().len());
            gallery.search = "Tokyo Night".into();
            let (out, _, bounds) = draw(&ctx, &mut gallery, width, vec![]);
            assert!(bounds.right() <= width + 1.0);
            assert!(
                out.shapes
                    .iter()
                    .any(|shape| text_center(&shape.shape, "Tokyo Night Storm").is_some())
            );
            assert!(
                !out.shapes
                    .iter()
                    .any(|shape| text_center(&shape.shape, "Graphite").is_some())
            );
            out.drop_without_applying_deltas();
        }
    }

    #[test]
    fn tapping_a_preview_returns_the_preset_without_replacing_host_settings() {
        let ctx = Context::default();
        let mut config = crate::appearance::ThemeConfig::default();
        config.control_height = Some(44.0);
        config.select_preset("studio");
        config.apply(&ctx, crate::appearance::Density::Touch);
        let before = ctx.global_style();
        let tokens = Tokens::get(&ctx);
        let mut gallery = PresetGallery {
            search: "Tokyo Night Storm".into(),
            ..Default::default()
        };
        for _ in 0..2 {
            draw(&ctx, &mut gallery, 320.0, vec![])
                .0
                .drop_without_applying_deltas();
        }
        let (out, _, _) = draw(&ctx, &mut gallery, 320.0, vec![]);
        let point = out
            .shapes
            .iter()
            .rev()
            .find_map(|shape| text_center(&shape.shape, "Tokyo Night Storm"))
            .expect("preview is visible");
        out.drop_without_applying_deltas();
        draw(&ctx, &mut gallery, 320.0, vec![Event::PointerMoved(point)])
            .0
            .drop_without_applying_deltas();
        let press = Event::PointerButton {
            pos: point,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        };
        draw(
            &ctx,
            &mut gallery,
            320.0,
            vec![Event::PointerMoved(point), press],
        )
        .0
        .drop_without_applying_deltas();
        let release = Event::PointerButton {
            pos: point,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        };
        let (out, picked, _) = draw(&ctx, &mut gallery, 320.0, vec![release]);
        out.drop_without_applying_deltas();
        assert_eq!(picked.map(|p| p.id), Some("tokyo-night-storm"));
        assert_eq!(*before, *ctx.global_style());
        assert_eq!(tokens, Tokens::get(&ctx));
        assert_eq!(ctx.global_style().spacing.interact_size.y, 44.0);
    }
}
