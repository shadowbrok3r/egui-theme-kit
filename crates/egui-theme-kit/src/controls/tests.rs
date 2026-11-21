use super::*;
use crate::fonts::{self, TextScale};
use crate::metrics::{self, Density};
use crate::presets::{MATCHABLE_PRESETS, apply_preset, bootstrap_startup_theme, set_density};
use egui::{
    CentralPanel, Context, Event, FontDefinitions, Modifiers, PointerButton, Pos2, RawInput, Shape,
    Style, Vec2, pos2, vec2,
};

fn context(density: Density) -> Context {
    let ctx = Context::default();
    let mut definitions = FontDefinitions::default();
    fonts::install(&mut definitions);
    ctx.set_fonts(definitions);
    metrics::set_density(&ctx, density);
    metrics::install_overlaid_style(&ctx, Style::default());
    ctx
}

fn themed_context() -> Context {
    let ctx = Context::default();
    ctx.set_fonts(fonts::definitions(TextScale::Normal));
    bootstrap_startup_theme(&ctx);
    ctx
}

fn input(events: Vec<Event>) -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 600.0))),
        events,
        ..Default::default()
    }
}

/// Runs `f` for `passes` frames and returns what its last pass produced.
fn run<R>(ctx: &Context, passes: usize, mut f: impl FnMut(&mut Ui) -> R) -> R {
    let mut out = None;
    for _ in 0..passes {
        let mut full = ctx.run_ui(input(Vec::new()), |ui| {
            CentralPanel::default().show(ui, |ui| out = Some(f(ui)));
        });
        full.textures_delta.clear();
    }
    out.expect("the ui closure ran")
}

fn run_with(ctx: &Context, events: Vec<Event>, mut f: impl FnMut(&mut Ui)) {
    let mut full = ctx.run_ui(input(events), |ui| {
        CentralPanel::default().show(ui, |ui| f(ui));
    });
    full.textures_delta.clear();
}

/// The first row's rect that leaves `height` or the shared centre line.
fn misaligned(rects: &[Rect], height: f32) -> Option<Rect> {
    let centre = rects.first()?.center().y;
    rects
        .iter()
        .copied()
        .find(|r| (r.height() - height).abs() > 0.01 || (r.center().y - centre).abs() > 0.01)
}

fn row(ui: &mut Ui) -> Vec<Rect> {
    let mut text = "Text".to_owned();
    let mut size = 20.0_f32;
    let mut choice = 0_usize;
    let mut on = true;
    ui.horizontal(|ui| {
        vec![
            button(ui, "Apply").rect,
            icon_button(ui, icons::GEAR, IconBox::Md, false, "Settings").rect,
            text_field(ui, &mut text, "", 120.0).rect,
            value_field(ui, &mut size, 1.0..=100.0, "px", 70.0).rect,
            dropdown(ui, "row", &mut choice, &[(0, "One"), (1, "Two")], 100.0).rect,
            toggle(ui, &mut on, "Live").rect,
        ]
    })
    .inner
}

#[test]
fn row_controls_share_control_height_and_centre_at_every_density() {
    for density in Density::ALL {
        let ctx = context(density);
        let rects = run(&ctx, 2, row);
        let height = Metrics::for_density(density).control_h;
        assert_eq!(rects.len(), 6);
        assert_eq!(misaligned(&rects, height), None, "{density:?}: {rects:?}");
    }
}

#[test]
fn the_alignment_check_catches_a_control_off_the_row() {
    let ctx = context(Density::Regular);
    let mut rects = run(&ctx, 2, row);
    let height = Metrics::for_density(Density::Regular).control_h;
    rects.push(rects[0].translate(vec2(0.0, 1.0)));
    assert!(misaligned(&rects, height).is_some());
    rects.pop();
    rects.push(Rect::from_min_size(rects[0].min, vec2(30.0, height + 2.0)));
    assert!(misaligned(&rects, height).is_some());
}

#[test]
fn prominent_buttons_take_the_button_height() {
    for density in Density::ALL {
        let ctx = context(density);
        let m = Metrics::for_density(density);
        let heights = run(&ctx, 1, |ui| {
            [
                primary_button(ui, "Save").rect.height(),
                secondary_button(ui, "Cancel").rect.height(),
                danger_button(ui, "Delete").rect.height(),
                ghost_button(ui, "More").rect.height(),
            ]
        });
        assert_eq!(heights, [m.button_h, m.button_h, m.button_h, m.control_h]);
    }
}

#[test]
fn every_button_kind_takes_its_height_and_a_text_button_wraps_its_label() {
    for density in Density::ALL {
        let ctx = context(density);
        let m = Metrics::for_density(density);
        let rects = run(&ctx, 2, |ui| {
            ButtonKind::ALL.map(|kind| {
                let plain = TextButton::new("Go").kind(kind).show(ui).rect;
                let tall = TextButton::new("Go")
                    .kind(kind)
                    .prominent(true)
                    .show(ui)
                    .rect;
                (plain, tall)
            })
        });
        for (kind, (plain, tall)) in ButtonKind::ALL.into_iter().zip(rects) {
            assert_eq!(plain.height(), m.control_h, "{kind:?} {density:?}");
            assert_eq!(tall.height(), m.button_h, "{kind:?} {density:?}");
            assert!(plain.width() >= 2.0 * m.pad.x, "{kind:?} {density:?}");
        }
    }
}

#[test]
fn icon_boxes_are_square_and_md_matches_the_control_height_at_every_density() {
    for density in Density::ALL {
        let ctx = context(density);
        let m = Metrics::for_density(density);
        let sizes = run(&ctx, 1, |ui| {
            [IconBox::Sm, IconBox::Md, IconBox::Lg, IconBox::Tool]
                .map(|b| icon_button(ui, icons::GEAR, b, false, "").rect.size())
        });
        assert_eq!(sizes[1], Vec2::splat(m.control_h), "{density:?}");
        assert_eq!(sizes[2], Vec2::splat(m.button_h), "{density:?}");
        for size in sizes {
            assert_eq!(size.x, size.y, "{density:?}");
            assert_eq!(size.x, size.x.round(), "{density:?}");
        }
    }
}

#[test]
fn bars_hold_their_height_whatever_they_contain() {
    let ctx = context(Density::Regular);
    let m = Metrics::for_density(Density::Regular);
    for kind in [
        BarKind::Tool,
        BarKind::Header,
        BarKind::Status,
        BarKind::Footer,
    ] {
        let (bar_rect, inner) = run(&ctx, 1, |ui| {
            let top = ui.cursor().top();
            let inner = bar(ui, kind, |ui| {
                icon_button(ui, icons::GEAR, IconBox::Md, false, "").rect
            });
            (
                Rect::from_min_max(
                    pos2(0.0, top),
                    pos2(1.0, ui.cursor().top() - ui.spacing().item_spacing.y),
                ),
                inner,
            )
        });
        assert_eq!(bar_rect.height(), kind.height(&m), "{kind:?}");
        assert!(
            (inner.center().y - bar_rect.center().y).abs() < 0.01,
            "{kind:?} row is off centre"
        );
    }
}

#[test]
fn dialog_buttons_put_the_default_answer_first_and_cancel_last() {
    let buttons = [
        (Role::Cancel, "Cancel"),
        (Role::Secondary, "Don't save"),
        (Role::Primary, "Save"),
    ];
    assert_eq!(layout::dialog_order(&buttons), [2, 1, 0]);
    let danger = [(Role::Cancel, "Keep"), (Role::Danger, "Delete")];
    assert_eq!(layout::dialog_order(&danger), [1, 0]);
}

#[test]
fn a_toggle_flips_when_clicked() {
    let ctx = context(Density::Regular);
    let mut on = false;
    let mut at = Rect::NOTHING;
    run_with(&ctx, Vec::new(), |ui| at = toggle(ui, &mut on, "Live").rect);
    let p = at.center();
    let press = |pressed| Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    run_with(&ctx, vec![Event::PointerMoved(p), press(true)], |ui| {
        let _ = toggle(ui, &mut on, "Live");
    });
    run_with(&ctx, vec![press(false)], |ui| {
        let _ = toggle(ui, &mut on, "Live");
    });
    assert!(on);
}

#[test]
fn a_preview_state_is_scoped_to_its_closure() {
    let ctx = context(Density::Regular);
    run(&ctx, 1, |ui| {
        let response = button(ui, "x");
        assert_eq!(state_of(ui, &response), (State::Rest, false));
        with_preview(ui, Preview::Pressed, |ui| {
            assert_eq!(state_of(ui, &response), (State::Pressed, false));
            with_preview(ui, Preview::Focused, |ui| assert!(preview_focused(ui)));
            assert!(!preview_focused(ui));
        });
        assert_eq!(state_of(ui, &response), (State::Rest, false));
    });
}

#[test]
fn the_gallery_draws_at_every_density() {
    for density in Density::ALL {
        let ctx = context(density);
        run(&ctx, 2, gallery::show);
    }
}

#[test]
fn the_gallery_draws_under_every_preset_and_density() {
    let ctx = themed_context();
    for preset in MATCHABLE_PRESETS {
        apply_preset(&ctx, preset);
        for density in Density::ALL {
            set_density(&ctx, density);
            run(&ctx, 2, |ui| {
                gallery::show_with(ui, &[("glyph", Icon::glyph("A"))])
            });
        }
    }
}

#[test]
fn the_kit_draws_without_the_weight_families() {
    let ctx = Context::default();
    metrics::install_overlaid_style(&ctx, Style::default());
    run(&ctx, 1, gallery::show);
}

fn click_at(ctx: &Context, p: Pos2, mut f: impl FnMut(&mut Ui)) {
    let press = |pressed| Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    run_with(ctx, vec![Event::PointerMoved(p), press(true)], &mut f);
    run_with(ctx, vec![press(false)], &mut f);
}

#[test]
fn a_closed_section_stays_closed_until_clicked_and_a_forced_state_wins() {
    let ctx = context(Density::Regular);
    let mut header = Rect::NOTHING;
    let mut opened = false;
    run_with(&ctx, Vec::new(), |ui| {
        let top = ui.cursor().top();
        opened = Section::new("s", "Registers")
            .default_open(false)
            .header(ui);
        header = Rect::from_min_max(pos2(0.0, top), pos2(100.0, ui.cursor().top()));
    });
    assert!(!opened);
    click_at(&ctx, header.center(), |ui| {
        opened = Section::new("s", "Registers")
            .default_open(false)
            .header(ui);
    });
    run_with(&ctx, Vec::new(), |ui| {
        opened = Section::new("s", "Registers")
            .default_open(false)
            .header(ui);
    });
    assert!(opened, "a click opens it");
    run_with(&ctx, Vec::new(), |ui| {
        opened = Section::new("s", "Registers").open(Some(false)).header(ui);
    });
    assert!(!opened, "a forced state wins");
}

#[test]
fn a_section_stores_its_state_where_a_collapsing_header_would() {
    let ctx = context(Density::Regular);
    let id = run(&ctx, 1, |ui| {
        let _ = Section::new("salt", "T").default_open(false).header(ui);
        ui.make_persistent_id(egui::IdSalt::new("salt"))
    });
    let stored = egui::collapsing_header::CollapsingState::load(&ctx, id);
    assert!(stored.is_some_and(|s| !s.is_open()));
}

#[test]
fn a_segment_click_picks_its_value() {
    let ctx = context(Density::Regular);
    let mut value = 0_u8;
    let segments = [Segment::new(0_u8, "One"), Segment::new(1, "Two")];
    let mut rect = Rect::NOTHING;
    run_with(&ctx, Vec::new(), |ui| {
        rect = segmented(ui, &mut value, &segments).rect;
    });
    assert_eq!(
        rect.height(),
        Metrics::for_density(Density::Regular).control_h
    );
    let right = pos2(rect.right() - 4.0, rect.center().y);
    let mut changed = false;
    click_at(&ctx, right, |ui| {
        changed |= segmented(ui, &mut value, &segments).changed();
    });
    assert_eq!(value, 1);
    assert!(changed);
}

#[test]
fn a_slider_click_sets_the_value_under_the_pointer() {
    let ctx = context(Density::Regular);
    let mut value = 0.0_f32;
    let mut rect = Rect::NOTHING;
    run_with(&ctx, Vec::new(), |ui| {
        rect = Slider::new(&mut value, 0.0..=10.0)
            .width(200.0)
            .step(1.0)
            .show(ui)
            .rect;
    });
    click_at(&ctx, rect.right_center() - vec2(7.0, 0.0), |ui| {
        let _ = Slider::new(&mut value, 0.0..=10.0)
            .width(200.0)
            .step(1.0)
            .show(ui);
    });
    assert_eq!(value, 10.0);
}

#[test]
fn a_disabled_button_ignores_clicks() {
    let ctx = context(Density::Regular);
    let mut rect = Rect::NOTHING;
    let mut clicked = false;
    run_with(&ctx, Vec::new(), |ui| {
        rect = TextButton::new("Go").enabled(false).show(ui).rect;
    });
    click_at(&ctx, rect.center(), |ui| {
        clicked |= TextButton::new("Go").enabled(false).show(ui).clicked();
    });
    assert!(!clicked);
    run_with(&ctx, Vec::new(), |ui| {
        let _ = TextButton::new("Go").show(ui);
    });
    click_at(&ctx, rect.center(), |ui| {
        clicked |= TextButton::new("Go").show(ui).clicked();
    });
    assert!(clicked, "the enabled button at the same spot clicks");
}

#[test]
fn row_kit_additions_share_the_control_height() {
    for density in Density::ALL {
        let ctx = context(density);
        let m = Metrics::for_density(density);
        let heights = run(&ctx, 2, |ui| {
            let mut v = 1.0_f32;
            let mut seg = 0_u8;
            let mut pick = 9_u8;
            ui.horizontal(|ui| {
                [
                    ProgressBar::new(0.5).width(80.0).show(ui).rect.height(),
                    Slider::new(&mut v, 0.0..=2.0).show(ui).rect.height(),
                    segmented(ui, &mut seg, &[Segment::new(0, "A")])
                        .rect
                        .height(),
                    Pill::new("P")
                        .tone(Color32::RED)
                        .selected(true)
                        .show(ui)
                        .rect
                        .height(),
                    Dropdown::new("d", &mut pick, &[(0, "x")])
                        .placeholder("Pick")
                        .show(ui)
                        .rect
                        .height(),
                ]
            })
            .inner
        });
        assert!(
            heights.iter().all(|h| *h == m.control_h),
            "{density:?}: {heights:?}"
        );
    }
}

#[test]
fn list_rows_menu_rows_and_headers_take_their_metrics_height() {
    for density in Density::ALL {
        let ctx = context(density);
        let m = Metrics::for_density(density);
        let heights = run(&ctx, 2, |ui| {
            [
                ListItem::new("Row")
                    .icon(icons::FILE)
                    .show(ui)
                    .rect
                    .height(),
                menu_body(ui, |ui| MenuItem::new("Item").show(ui).rect.height()),
                fold_header(ui, true, icons::FOLDER, "Header").rect.height(),
                checkbox(ui, &mut true, "Check").rect.height(),
                toggle(ui, &mut true, "Switch").rect.height(),
            ]
        });
        assert_eq!(
            heights,
            [m.row_h, m.row_h, m.header_h, m.control_h, m.control_h],
            "{density:?}"
        );
    }
}

#[test]
fn a_text_area_grows_with_its_rows() {
    let ctx = context(Density::Regular);
    let (two, six) = run(&ctx, 1, |ui| {
        let mut a = String::new();
        let mut b = String::new();
        (
            TextArea::new(&mut a).rows(2).width(300.0).show(ui).rect,
            TextArea::new(&mut b).rows(6).width(300.0).show(ui).rect,
        )
    });
    assert_eq!(two.width(), 300.0);
    assert!(six.height() > two.height() + 3.0 * 12.0, "{two:?} {six:?}");
}

#[test]
fn a_value_field_formatter_shows_its_text() {
    let ctx = context(Density::Regular);
    let mut threads = 0_u32;
    let mut full = ctx.run_ui(input(Vec::new()), |ui| {
        let _ = ValueField::new(&mut threads, 0..=64)
            .formatter(|v| {
                if v == 0.0 {
                    "auto".into()
                } else {
                    format!("{v}")
                }
            })
            .show(ui);
    });
    full.textures_delta.clear();
    let texts: Vec<String> = full
        .shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
            _ => None,
        })
        .collect();
    assert!(texts.iter().any(|t| t == "auto"), "{texts:?}");
}

#[test]
fn an_icon_window_keeps_the_plain_title_id() {
    let ctx = context(Density::Regular);
    let mut full = ctx.run_ui(input(Vec::new()), |ui| {
        icon_window(ui.ctx(), icons::GEAR, "Settings").show(ui.ctx(), |_| {});
        icon_window(ui.ctx(), Icon::glyph("G"), "Glyph").show(ui.ctx(), |_| {});
        egui::Window::new("Plain").show(ui.ctx(), |_| {});
    });
    full.textures_delta.clear();
    let known = |id| ctx.memory(|m| m.area_rect(id).is_some());
    assert!(known(window_id("Plain")), "window_id matches Window::new");
    for title in ["Settings", "Glyph"] {
        assert!(known(window_id(title)), "{title} keeps the plain id");
    }
}

#[test]
fn a_tab_title_leads_with_a_glyph_only() {
    let glyph = tab_title(Icon::glyph("G"), "Tab");
    let painted = tab_title(icons::GEAR, "Tab");
    let none = tab_title(None, "Tab");
    assert_eq!(glyph.text(), "GTab");
    assert_eq!(painted.text(), "Tab");
    assert_eq!(none.text(), "Tab");
}

#[test]
fn a_tree_node_toggles_from_its_caret() {
    let ctx = context(Density::Regular);
    let mut row = Rect::NOTHING;
    let mut shown = false;
    let draw = |ui: &mut Ui, row: &mut Rect, shown: &mut bool| {
        let (r, inner) = ListItem::new("C:").show_node(ui, "c", false, |_| ());
        *row = r.rect;
        *shown = inner.is_some();
    };
    run_with(&ctx, Vec::new(), |ui| draw(ui, &mut row, &mut shown));
    assert!(!shown);
    let caret = row.left_center() + vec2(12.0, 0.0);
    click_at(&ctx, caret, |ui| draw(ui, &mut row, &mut shown));
    run_with(&ctx, Vec::new(), |ui| draw(ui, &mut row, &mut shown));
    assert!(shown, "a caret click opens the node");
}

#[test]
fn one_tab_press_focuses_the_text_inside_a_field() {
    let ctx = context(Density::Regular);
    let mut text = String::new();
    let mut draw = |ui: &mut Ui| {
        let _ = TextField::new(&mut text).id_salt("tab.field").show(ui);
    };
    run_with(&ctx, Vec::new(), &mut draw);
    let tab = Event::Key {
        key: egui::Key::Tab,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    run_with(&ctx, vec![tab], &mut draw);
    run_with(&ctx, Vec::new(), &mut draw);
    let focused = ctx.memory(|m| m.has_focus(egui::Id::unique("tab.field")));
    assert!(focused, "the first Tab lands on the text, not the box");
}

#[test]
fn disabled_menu_rows_and_dialog_buttons_ignore_clicks() {
    let ctx = context(Density::Regular);
    let mut rects = (Rect::NOTHING, Rect::NOTHING);
    let mut hits = (false, None);
    let draw = |ui: &mut Ui, rects: &mut (Rect, Rect), hits: &mut (bool, Option<usize>)| {
        let row = MenuItem::new("Archive").enabled(false).show(ui);
        rects.0 = row.rect;
        hits.0 |= row.clicked();
        let buttons = [(Role::Primary, "Apply")];
        let top = ui.cursor().top();
        hits.1 = hits.1.or(dialog_buttons_enabled(ui, &buttons, &[false]));
        rects.1 = Rect::from_min_max(
            pos2(ui.max_rect().right() - 40.0, top),
            pos2(ui.max_rect().right() - 20.0, top + 20.0),
        );
    };
    run_with(&ctx, Vec::new(), |ui| draw(ui, &mut rects, &mut hits));
    let (row, button) = rects;
    click_at(&ctx, row.center(), |ui| draw(ui, &mut rects, &mut hits));
    click_at(&ctx, button.center(), |ui| draw(ui, &mut rects, &mut hits));
    assert_eq!(hits, (false, None));
}

#[test]
fn a_section_reports_its_header_and_takes_an_exact_id() {
    let ctx = context(Density::Regular);
    let id = egui::Id::unique("exact.section");
    let mut header = Rect::NOTHING;
    let mut clicked = false;
    let draw = |ui: &mut Ui, header: &mut Rect, clicked: &mut bool| {
        let (response, _) = Section::new("unused", "Inventory")
            .id(id)
            .suffix("(3)", None)
            .trailing(|ui| {
                let _ = badge(ui, "1 running", Color32::YELLOW);
            })
            .default_open(false)
            .show_with_header(ui, |_| ());
        *header = response.rect;
        *clicked |= response.clicked();
    };
    run_with(&ctx, Vec::new(), |ui| draw(ui, &mut header, &mut clicked));
    assert_eq!(
        header.height(),
        Metrics::for_density(Density::Regular).header_h
    );
    click_at(&ctx, header.left_center() + vec2(30.0, 0.0), |ui| {
        draw(ui, &mut header, &mut clicked)
    });
    assert!(clicked);
    let stored = egui::collapsing_header::CollapsingState::load(&ctx, id);
    assert!(
        stored.is_some_and(|s| s.is_open()),
        "the click opened the exact id"
    );
}

#[test]
fn a_loading_row_sits_on_one_line() {
    let ctx = context(Density::Regular);
    let (row, line) = run(&ctx, 1, |ui| {
        (
            loading_row(ui, "Loading drivers").rect,
            ui.text_style_height(&egui::TextStyle::Body),
        )
    });
    assert!(
        (row.height() - line.max(SpinnerSize::Sm.side())).abs() < 0.5,
        "{row:?} {line}"
    );
    assert_eq!(row.width(), row.width().round());
}

#[test]
fn a_menu_without_a_set_width_fits_its_rows() {
    let ctx = context(Density::Regular);
    let trigger = std::cell::Cell::new(Rect::NOTHING);
    let mut rows = Vec::new();
    let mut draw = |ui: &mut Ui| {
        let (response, _) = MenuButton::new("View").show(ui, |ui| {
            rows.clear();
            rows.push(menu_item(ui, icons::GEAR, "Settings").rect.width());
            menu_separator(ui);
            rows.push(menu_check_item(ui, None, "Wrap lines", true).rect.width());
        });
        trigger.set(response.rect);
    };
    run_with(&ctx, Vec::new(), &mut draw);
    click_at(&ctx, trigger.get().center(), &mut draw);
    for _ in 0..3 {
        run_with(&ctx, Vec::new(), &mut draw);
    }
    assert_eq!(rows.len(), 2, "the menu opened");
    for width in rows {
        assert!(
            (width - layout::MENU_MIN_W).abs() < 1.0,
            "menu row is {width} wide"
        );
    }
}

#[test]
fn a_checkbox_row_flips_from_its_label_and_holds_the_control_height() {
    let ctx = context(Density::Comfortable);
    let mut on = false;
    let mut row = Rect::NOTHING;
    run_with(&ctx, Vec::new(), |ui| {
        row = checkbox(ui, &mut on, "Run on startup").rect
    });
    assert_eq!(
        row.height(),
        Metrics::for_density(Density::Comfortable).control_h
    );
    click_at(&ctx, row.right_center() - vec2(4.0, 0.0), |ui| {
        let _ = checkbox(ui, &mut on, "Run on startup");
    });
    assert!(on);
}

#[test]
fn an_icon_link_reports_the_width_it_allocates() {
    let ctx = context(Density::Regular);
    for (text, small) in [("Link", true), ("customer:abc123", false)] {
        let (reported, drawn) = run(&ctx, 2, |ui| {
            let link = IconLink::new(text).icon(icons::PLUS).small(small);
            (link.width(ui), link.show(ui).rect.width())
        });
        assert_eq!(reported, drawn, "{text}");
    }
}

/// Every shape the last of `passes` frames of `f` painted on the root Ui, flattened.
fn painted(ctx: &Context, passes: usize, mut f: impl FnMut(&mut Ui)) -> Vec<Shape> {
    fn flatten(shape: Shape, out: &mut Vec<Shape>) {
        match shape {
            Shape::Vec(shapes) => shapes.into_iter().for_each(|s| flatten(s, out)),
            other => out.push(other),
        }
    }
    let mut shapes = Vec::new();
    for _ in 0..passes {
        let mut full = ctx.run_ui(input(Vec::new()), |ui| f(ui));
        full.textures_delta.clear();
        shapes.clear();
        for clipped in full.shapes {
            flatten(clipped.shape, &mut shapes);
        }
    }
    shapes
}

/// Swatch colours no theme paints.
const PROBES: [Color32; 7] = [
    Color32::from_rgb(1, 2, 3),
    Color32::from_rgb(4, 5, 6),
    Color32::from_rgb(7, 8, 9),
    Color32::from_rgb(10, 11, 12),
    Color32::from_rgb(13, 14, 15),
    Color32::from_rgb(16, 17, 18),
    Color32::from_rgb(19, 20, 21),
];

#[test]
fn swatches_keep_the_menu_row_height_and_sit_before_the_check_at_every_density() {
    for density in Density::ALL {
        let ctx = context(density);
        let m = Metrics::for_density(density);
        let mut rows = [Rect::NOTHING; 2];
        let shapes = painted(&ctx, 2, |ui| {
            menu_body(ui, |ui| {
                for (i, checked) in [true, false].into_iter().enumerate() {
                    rows[i] = MenuItem::new("Palette")
                        .swatches(&PROBES)
                        .checked(checked)
                        .show(ui)
                        .rect;
                }
            });
        });
        let chips: Vec<Rect> = shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Rect(r) if PROBES.contains(&r.fill) => Some(r.rect),
                _ => None,
            })
            .collect();
        assert_eq!(chips.len(), 2 * PROBES.len(), "{density:?}");
        for (row, strip) in rows.iter().zip(chips.chunks(PROBES.len())) {
            assert_eq!(row.height(), m.row_h, "{density:?}");
            let check_left = row.right() - m.pad.x - m.icon_sm;
            for chip in strip {
                assert!(
                    row.contains_rect(*chip),
                    "{density:?}: {chip:?} leaves {row:?}"
                );
                assert!(
                    (chip.center().y - row.center().y).abs() <= 0.5,
                    "{density:?}"
                );
                assert!(chip.right() + m.gap.x <= check_left + 0.01, "{density:?}");
            }
            for pair in strip.windows(2) {
                assert!(
                    pair[0].right() < pair[1].left(),
                    "{density:?}: chips overlap"
                );
            }
        }
        assert_eq!(
            chips[0].left(),
            chips[PROBES.len()].left(),
            "{density:?}: checked and unchecked strips misalign"
        );
    }
}

#[test]
fn swatch_outlines_stand_out_from_the_menu_and_hover_fills() {
    let ctx = context(Density::Regular);
    let dark = Tokens::get(&ctx);
    let light_style = Style {
        visuals: egui::Visuals::light(),
        ..Style::default()
    };
    let light = Tokens::derive(&light_style, dark.success, dark.secondary);
    let flush = Tokens {
        separator: dark.window,
        hover: dark.sunken,
        ..dark
    };
    let raised_hover = Tokens {
        hover: mix(dark.window, dark.text, 0.3),
        ..dark
    };
    let cases = [
        ("dark", dark),
        ("light", light),
        ("separator on the menu fill", flush),
        ("bright hover", raised_hover),
    ];
    for (name, t) in cases {
        Tokens::publish(&ctx, t);
        let shapes = painted(&ctx, 2, |ui| {
            menu_body(ui, |ui| {
                let _ = MenuItem::new("Palette").swatches(&PROBES).show(ui);
            });
        });
        let rings: Vec<Color32> = shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Rect(r) if PROBES.contains(&r.fill) => Some(r.stroke.color),
                _ => None,
            })
            .collect();
        assert_eq!(rings.len(), PROBES.len(), "{name}");
        for ring in rings {
            for (fill_name, fill) in [("menu", t.window), ("hover", t.hover)] {
                let ratio = crate::tokens::contrast(ring, fill);
                assert!(ratio >= 1.4, "{name}: {ratio:.2} on the {fill_name} fill");
            }
        }
    }
}

#[test]
fn a_menu_of_swatched_rows_stays_narrow() {
    let ctx = context(Density::Regular);
    let trigger = std::cell::Cell::new(Rect::NOTHING);
    let mut widths = Vec::new();
    let mut draw = |ui: &mut Ui| {
        let (response, _) = MenuButton::new("Theme").show(ui, |ui| {
            widths.clear();
            widths.push(MenuItem::new("Custom").checked(false).show(ui).rect.width());
            let row = MenuItem::new("Midnight glass preset")
                .swatches(&PROBES)
                .checked(true)
                .show(ui);
            widths.push(row.rect.width());
        });
        trigger.set(response.rect);
    };
    run_with(&ctx, Vec::new(), &mut draw);
    click_at(&ctx, trigger.get().center(), &mut draw);
    for _ in 0..3 {
        run_with(&ctx, Vec::new(), &mut draw);
    }
    assert_eq!(widths.len(), 2, "the menu opened");
    assert_eq!(widths[0], widths[1], "rows share the menu width");
    assert!(
        widths[1] > layout::MENU_MIN_W,
        "the strip widened nothing: {}",
        widths[1]
    );
    assert!(widths[1] <= 300.0, "the menu is {} wide", widths[1]);
}

#[test]
fn a_marked_row_paints_a_primary_dot_in_its_icon_slot() {
    let ctx = context(Density::Regular);
    let t = Tokens::get(&ctx);
    let m = Metrics::for_density(Density::Regular);
    let mut item = Rect::NOTHING;
    let shapes = painted(&ctx, 2, |ui| {
        menu_body(ui, |ui| {
            let _ = MenuItem::new("Group").marked(true).submenu(ui, |_| ());
            item = MenuItem::new("Item")
                .icon(icons::GEAR)
                .marked(true)
                .show(ui)
                .rect;
            let _ = MenuItem::new("Plain").show(ui);
        });
    });
    let dots: Vec<Pos2> = shapes
        .iter()
        .filter_map(|s| match s {
            Shape::Circle(c) if c.fill == t.primary => Some(c.center),
            _ => None,
        })
        .collect();
    assert_eq!(dots.len(), 2, "{dots:?}");
    let slot = pos2(item.left() + m.pad.x + m.icon_md / 2.0, item.center().y);
    assert!(
        (dots[1] - slot).length() < 0.01,
        "{:?} vs {slot:?}",
        dots[1]
    );
}

#[test]
fn a_theme_preview_paints_inside_its_rect_at_every_density() {
    for density in Density::ALL {
        let ctx = context(density);
        let m = Metrics::for_density(density);
        let t = Tokens::get(&ctx);
        let mut rect = Rect::NOTHING;
        let name = "A theme whose name runs well past the title bar of the preview";
        let shapes = painted(&ctx, 2, |ui| rect = theme_preview(ui, &t, name).rect);
        assert_eq!(rect.width(), THEME_PREVIEW_W, "{density:?}");
        assert!(
            THEME_PREVIEW_W <= m.tooltip_w,
            "{density:?}: wider than a tooltip"
        );
        assert!(
            rect.height() > m.header_h + 4.0 * m.control_h,
            "{density:?}"
        );
        assert!(rect.height() < THEME_PREVIEW_W, "{density:?}");
        assert!(shapes.len() > 10, "{density:?}: {} shapes", shapes.len());
        for shape in &shapes {
            let bounds = shape.visual_bounding_rect();
            assert!(
                rect.expand(0.5).contains_rect(bounds),
                "{density:?}: {bounds:?} leaves {rect:?}"
            );
        }
    }
}

#[test]
fn a_theme_preview_paints_the_given_tokens_not_the_installed_ones() {
    let ctx = context(Density::Regular);
    let installed = Tokens::get(&ctx);
    let preset = Tokens {
        base: Color32::from_rgb(3, 60, 9),
        primary: Color32::from_rgb(250, 10, 200),
        field: Color32::from_rgb(70, 1, 70),
        ..installed
    };
    let shapes = painted(&ctx, 2, |ui| {
        let _ = theme_preview(ui, &preset, "Preset");
    });
    let fills: Vec<Color32> = shapes
        .iter()
        .filter_map(|s| match s {
            Shape::Rect(r) => Some(r.fill),
            _ => None,
        })
        .collect();
    for c in [preset.base, preset.primary, preset.field] {
        assert!(fills.contains(&c), "{c:?} missing from {fills:?}");
    }
    assert!(
        !fills.contains(&installed.base),
        "painted the installed base"
    );
}

#[test]
fn button_colours_change_with_state_and_follow_the_kind() {
    let ctx = context(Density::Regular);
    let t = Tokens::get(&ctx);
    for kind in ButtonKind::ALL {
        let [rest, hover, pressed] =
            [State::Rest, State::Hovered, State::Pressed].map(|s| button_colors(&t, kind, s));
        assert_ne!(rest.0, pressed.0, "{kind:?}: pressed looks like rest");
        match kind {
            ButtonKind::Neutral => {
                assert_eq!(rest.0, t.field);
                assert_eq!(hover.0, t.hover);
                assert_eq!(pressed.0, t.pressed);
                assert_eq!(rest.2, t.text);
            }
            ButtonKind::Primary => {
                assert_eq!(rest.0, t.primary);
                assert_ne!(hover.0, rest.0);
                assert_eq!(rest.2, t.on_primary);
            }
            ButtonKind::Danger => {
                assert_eq!(rest.2, t.error);
                assert_ne!(hover.0, rest.0);
                assert_ne!(rest.1, Color32::TRANSPARENT);
            }
            ButtonKind::Ghost => {
                assert_eq!(rest.0, Color32::TRANSPARENT);
                assert_eq!(hover.0, t.hover);
                assert_eq!(rest.2, t.text_dim);
            }
        }
    }
}

#[test]
fn a_painted_button_uses_the_colours_of_its_state() {
    let ctx = context(Density::Regular);
    let t = Tokens::get(&ctx);
    for kind in ButtonKind::ALL {
        let shapes = painted(&ctx, 2, |ui| {
            CentralPanel::default().show(ui, |ui| {
                let _ = TextButton::new("Go").kind(kind).show(ui);
            });
        });
        let (fill, _, _) = button_colors(&t, kind, State::Rest);
        let want = if fill == Color32::TRANSPARENT {
            None
        } else {
            Some(fill)
        };
        let found = shapes
            .iter()
            .any(|s| matches!(s, Shape::Rect(r) if Some(r.fill) == want));
        assert!(
            want.is_none() || found,
            "{kind:?}: rest fill {fill:?} not painted"
        );
    }
}

#[test]
fn a_glyph_icon_paints_text_and_a_painted_icon_paints_shapes() {
    let ctx = context(Density::Regular);
    let shapes = painted(&ctx, 1, |ui| {
        let rect = Rect::from_min_size(pos2(10.0, 10.0), Vec2::splat(16.0));
        Icon::glyph("G").paint(ui, rect, 16.0, Color32::WHITE);
        icons::CHECK.paint(ui, rect, 16.0, Color32::WHITE);
    });
    let has_text = shapes.iter().any(|s| matches!(s, Shape::Text(_)));
    let has_line = shapes
        .iter()
        .any(|s| matches!(s, Shape::LineSegment { .. } | Shape::Path(_)));
    assert!(has_text, "glyph paints no text");
    assert!(has_line, "painted icon paints no line");
}

#[test]
fn icons_stay_inside_their_slot_at_every_density() {
    for density in Density::ALL {
        let ctx = context(density);
        let m = Metrics::for_density(density);
        for (name, icon) in icons::ALL {
            let slot = Rect::from_min_size(pos2(20.0, 20.0), Vec2::splat(m.icon_md));
            let shapes = painted(&ctx, 1, |ui| {
                icon.paint(ui, slot, m.icon_md, Color32::WHITE)
            });
            for shape in &shapes {
                let bounds = shape.visual_bounding_rect();
                assert!(
                    slot.expand(1.5).contains_rect(bounds),
                    "{name} at {density:?}: {bounds:?} leaves {slot:?}"
                );
            }
        }
    }
}

#[test]
fn glyph_strings_convert_into_icons_through_the_builders() {
    let ctx = context(Density::Regular);
    run(&ctx, 2, |ui| {
        let _ = icon_button(ui, "G", IconBox::Md, false, "Glyph");
        let _ = icon_text_button(ui, "G", "Glyph");
        let _ = TextButton::new("Glyph").icon("G").show(ui);
        let _ = IconButton::new("G", IconBox::Lg).selected(true).show(ui);
        let _ = IconLabel::new("G", "Glyph").show(ui);
        let _ = status_label(ui, "G", "Glyph", Color32::GREEN);
        let _ = TextField::new(&mut String::new()).icon("G").show(ui);
        let _ = MenuButton::new("Glyph").icon("G").show(ui, |_| ());
        let _ = ListItem::new("Glyph")
            .icon(Icon::glyph("G"))
            .action("G", "tip")
            .show(ui);
        let _ = Segment::new(0, "Glyph").icon("G");
        let _ = empty_state(ui, "G", "Empty", "", &[]);
    });
}

#[test]
fn sides_put_the_right_content_at_the_right_edge() {
    let ctx = context(Density::Regular);
    let (left, right, panel) = run(&ctx, 2, |ui| {
        let width = ui.available_width();
        let origin = ui.cursor().left();
        let (l, r) = sides(
            ui,
            |ui| button(ui, "Left").rect,
            |ui| button(ui, "Right").rect,
        );
        (l, r, origin..=origin + width)
    });
    assert!((left.left() - panel.start()).abs() < 1.0, "{left:?}");
    assert!(
        (right.right() - panel.end()).abs() < 1.0,
        "{right:?} in {panel:?}"
    );
    assert!(left.right() < right.left());
}

#[test]
fn form_rows_align_labels_and_controls_on_the_control_height_at_every_density() {
    for density in Density::ALL {
        let ctx = context(density);
        let m = Metrics::for_density(density);
        let rows = run(&ctx, 3, |ui| {
            let mut text = String::new();
            let mut size = 1.0_f32;
            form(ui, "form", |ui| {
                let a = form_row(ui, "Name", |ui| text_field(ui, &mut text, "", 140.0).rect);
                let b = form_row(ui, "A longer label", |ui| {
                    value_field(ui, &mut size, 0.0..=9.0, "px", 80.0).rect
                });
                [a, b]
            })
        });
        for rect in rows {
            assert_eq!(rect.height(), m.control_h, "{density:?}");
        }
        assert_eq!(
            rows[0].left(),
            rows[1].left(),
            "{density:?}: columns misalign"
        );
        assert!(rows[1].top() > rows[0].top(), "{density:?}");
    }
}

#[test]
fn striped_rows_paint_a_band_only_at_rest() {
    let ctx = context(Density::Regular);
    let t = Tokens::get(&ctx);
    let band = mix(t.base, t.text, 0.045);
    let fills = |striped: bool| {
        painted(&ctx, 2, |ui| {
            let _ = ListItem::new("Row").striped(striped).show(ui);
        })
        .iter()
        .filter(|s| matches!(s, Shape::Rect(r) if r.fill == band))
        .count()
    };
    assert_eq!(fills(false), 0);
    assert_eq!(fills(true), 1);
}

#[test]
fn usage_meters_take_the_colour_of_their_level() {
    let ctx = context(Density::Regular);
    let colours = |fraction: f32| {
        let shapes = painted(&ctx, 2, |ui| {
            let _ = usage_meter(ui, fraction, 100.0);
        });
        shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Rect(r) => Some(r.fill),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let (good, warn, critical) = (colours(0.3), colours(0.75), colours(0.95));
    let level = |pct: f32| {
        let mut out = Color32::TRANSPARENT;
        run(&ctx, 1, |ui| out = crate::colors::usage_level(ui, pct));
        out
    };
    assert!(good.contains(&level(30.0)));
    assert!(warn.contains(&level(75.0)));
    assert!(critical.contains(&level(95.0)));
    assert_ne!(level(30.0), level(95.0));
}

/// Paints a square filled with the colour it is given.
fn swatch_icon(painter: &egui::Painter, rect: Rect, color: Color32) {
    painter.rect_filled(rect, 0.0, color);
}

/// Fill colours of the rects and the colours of the text one frame of `f` paints.
fn painted_colours(ctx: &Context, mut f: impl FnMut(&mut Ui)) -> (Vec<Color32>, Vec<Color32>) {
    let mut full = ctx.run_ui(input(Vec::new()), |ui| {
        CentralPanel::default().show(ui, |ui| f(ui));
    });
    full.textures_delta.clear();
    let (mut fills, mut texts) = (Vec::new(), Vec::new());
    for clipped in &full.shapes {
        match &clipped.shape {
            Shape::Rect(rect) => fills.push(rect.fill),
            Shape::Text(text) => texts.push(text.fallback_color),
            _ => {}
        }
    }
    (fills, texts)
}

#[test]
fn an_icon_tint_colours_the_icon_of_a_text_button_and_not_its_label() {
    let ctx = context(Density::Regular);
    let tint = Color32::from_rgb(1, 2, 3);
    let (fills, texts) = painted_colours(&ctx, |ui| {
        let _ = TextButton::new("Undo")
            .icon(Icon::painted(swatch_icon))
            .icon_tint(tint)
            .show(ui);
    });
    assert!(fills.contains(&tint), "{fills:?}");
    assert!(!texts.is_empty() && !texts.contains(&tint), "{texts:?}");
}

#[test]
fn an_icon_tone_colours_a_menu_icon_unless_the_row_is_a_danger_row() {
    let ctx = context(Density::Regular);
    let tone = Color32::from_rgb(4, 5, 6);
    let (fills, texts) = painted_colours(&ctx, |ui| {
        let _ = MenuItem::new("Undo")
            .icon(Icon::painted(swatch_icon))
            .icon_tone(tone)
            .show(ui);
    });
    assert!(fills.contains(&tone), "{fills:?}");
    assert!(!texts.contains(&tone), "{texts:?}");
    let (fills, _) = painted_colours(&ctx, |ui| {
        let _ = MenuItem::new("Delete")
            .icon(Icon::painted(swatch_icon))
            .icon_tone(tone)
            .danger(true)
            .show(ui);
    });
    assert!(!fills.contains(&tone), "{fills:?}");
}

fn key(key: egui::Key) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

/// Every text one frame of `f` painted, flattened out of nested shapes.
fn painted_texts(shapes: &[Shape]) -> Vec<String> {
    shapes
        .iter()
        .filter_map(|s| match s {
            Shape::Text(text) => Some(text.galley.text().to_owned()),
            _ => None,
        })
        .collect()
}

#[test]
fn dashboard_widgets_take_their_sizes_at_every_density() {
    for density in Density::ALL {
        let ctx = context(density);
        let m = Metrics::for_density(density);
        let values = [1.0, 3.0, 2.0, 5.0];
        let (sizes, action_h) = run(&ctx, 2, |ui| {
            let sizes = [
                RingGauge::new(0.5, "50%")
                    .diameter(64.0)
                    .show(ui)
                    .rect
                    .size(),
                Sparkline::new(&values, vec2(80.0, 24.0))
                    .show(ui)
                    .rect
                    .size(),
                StatTile::new(icons::GEAR, "12", "Label")
                    .delta("+3", Trend::Up)
                    .sparkline(&values)
                    .width(260.0)
                    .show(ui)
                    .rect
                    .size(),
                ActionTile::new(icons::GEAR, "Title")
                    .blurb("Blurb")
                    .width(260.0)
                    .show(ui)
                    .rect
                    .size(),
                status_chip(ui, icons::CHECK, "Online", Color32::GREEN)
                    .rect
                    .size(),
                IconChip::new(icons::GEAR).side(30.0).show(ui).rect.size(),
                avatar(ui, "Dana Whitfield", 28.0).rect.size(),
                LineChart::new(&[Series::new("a", &values)])
                    .width(300.0)
                    .height(100.0)
                    .show(ui)
                    .rect
                    .size(),
            ];
            (sizes, ActionTile::height(ui))
        });
        assert_eq!(sizes[0], Vec2::splat(64.0), "{density:?}");
        assert_eq!(sizes[1], vec2(80.0, 24.0), "{density:?}");
        assert_eq!(sizes[2], vec2(260.0, StatTile::height()), "{density:?}");
        assert_eq!(sizes[3], vec2(260.0, action_h), "{density:?}");
        assert_eq!(sizes[4].y, m.control_h, "{density:?}");
        assert_eq!(sizes[4].x, sizes[4].x.round(), "{density:?}");
        assert_eq!(sizes[5], Vec2::splat(30.0), "{density:?}");
        assert_eq!(sizes[6], Vec2::splat(28.0), "{density:?}");
        assert_eq!(sizes[7], vec2(300.0, 100.0), "{density:?}");
    }
}

#[test]
fn row_widgets_take_their_metrics_height_at_every_density() {
    for density in Density::ALL {
        let ctx = context(density);
        let m = Metrics::for_density(density);
        let (nav, collapsed, check, kbd_rect, dot, stepper_h, timeline) = run(&ctx, 2, |ui| {
            let nav = NavItem::new(icons::LIST, "Sessions")
                .count(4, None)
                .dot(Color32::GREEN, false)
                .show(ui)
                .rect;
            let collapsed = NavItem::new(icons::LIST, "Sessions")
                .collapsed(true)
                .count(4, None)
                .show(ui)
                .rect;
            let check = check_row(ui, Check::Good, "Reachable", "12 ms").rect;
            let kbd_rect = kbd(ui, "Ctrl+Shift+K").rect;
            let dot = StatusDot::new(Color32::GREEN).show(ui).rect;
            let top = ui.cursor().top();
            let steps = [
                Step::new("One", StepState::Done),
                Step::new("Two", StepState::Active),
            ];
            let _ = Stepper::new(&steps).width(400.0).show(ui);
            let stepper_h = ui.cursor().top() - ui.spacing().item_spacing.y - top;
            let timeline = TimelineRow::new("Started", StepState::Done)
                .width(300.0)
                .show(ui)
                .rect;
            (nav, collapsed, check, kbd_rect, dot, stepper_h, timeline)
        });
        assert_eq!(nav.height(), m.button_h + 4.0, "{density:?}");
        assert_eq!(
            collapsed.size(),
            Vec2::splat(m.button_h + 4.0),
            "{density:?}"
        );
        assert_eq!(check.height(), m.control_h, "{density:?}");
        assert_eq!(kbd_rect.height(), 18.0, "{density:?}");
        assert!(kbd_rect.width() > 3.0 * 18.0, "{density:?}: three caps");
        assert_eq!(
            dot.size(),
            Vec2::splat(StatusDot::new(Color32::GREEN).side())
        );
        assert!((stepper_h - (26.0 + 2.0 * m.pad.y).max(m.button_h)).abs() < 0.01);
        assert_eq!(timeline.width(), 300.0, "{density:?}");
        assert!(timeline.height() >= 18.0, "{density:?}");
    }
}

#[test]
fn a_code_input_lays_out_its_cells_and_squeezes_on_a_narrow_row() {
    let ctx = context(Density::Regular);
    let m = Metrics::for_density(Density::Regular);
    let (wide, narrow) = run(&ctx, 2, |ui| {
        let mut a = String::new();
        let wide = CodeInput::new(&mut a)
            .length(8)
            .group(4)
            .id_salt("wide")
            .show(ui)
            .response
            .rect;
        let mut b = String::new();
        let narrow = ui
            .allocate_ui(vec2(200.0, 60.0), |ui| {
                CodeInput::new(&mut b)
                    .length(8)
                    .id_salt("narrow")
                    .show(ui)
                    .response
                    .rect
            })
            .inner;
        (wide, narrow)
    });
    assert_eq!(wide.height(), m.button_h + 10.0);
    assert!(wide.width() > 8.0 * 20.0, "{wide:?}");
    assert!(narrow.width() <= 200.0 + 0.01, "{narrow:?}");
}

/// Draws a six-digit code input, returning its rect and output.
fn draw_code(
    ui: &mut Ui,
    code: &mut String,
    charset: CodeCharset,
    out: &mut Option<CodeInputResponse>,
) {
    *out = Some(
        CodeInput::new(code)
            .charset(charset)
            .id_salt("code")
            .show(ui),
    );
}

#[test]
fn a_code_input_filters_typing_and_backspace_moves_back() {
    let ctx = context(Density::Regular);
    let mut code = String::new();
    let mut out = None;
    run_with(&ctx, Vec::new(), |ui| {
        draw_code(ui, &mut code, CodeCharset::Digits, &mut out)
    });
    let rect = out.as_ref().map(|o| o.response.rect).expect("drawn");
    click_at(&ctx, rect.center(), |ui| {
        draw_code(ui, &mut code, CodeCharset::Digits, &mut out)
    });
    run_with(&ctx, vec![Event::Text("1a2-b3".into())], |ui| {
        draw_code(ui, &mut code, CodeCharset::Digits, &mut out)
    });
    assert_eq!(code, "123");
    assert!(out.as_ref().is_some_and(|o| o.changed && !o.completed));
    run_with(&ctx, vec![key(egui::Key::Backspace)], |ui| {
        draw_code(ui, &mut code, CodeCharset::Digits, &mut out)
    });
    assert_eq!(code, "12", "backspace removes the last cell");
    let typed = "3456789"
        .chars()
        .map(|c| Event::Text(c.to_string()))
        .collect();
    run_with(&ctx, typed, |ui| {
        draw_code(ui, &mut code, CodeCharset::Digits, &mut out)
    });
    assert_eq!(code, "123456", "typing stops at the last cell");
    let o = out.as_ref().expect("drawn");
    assert!(o.completed && o.full && o.changed);
    run_with(&ctx, Vec::new(), |ui| {
        draw_code(ui, &mut code, CodeCharset::Digits, &mut out)
    });
    let o = out.as_ref().expect("drawn");
    assert!(o.full && !o.completed && !o.changed, "completed fires once");
}

#[test]
fn a_pasted_code_fills_every_cell_and_letters_are_uppercased() {
    let ctx = context(Density::Regular);
    let mut code = "9".to_owned();
    let mut out = None;
    let charset = CodeCharset::Unambiguous;
    run_with(&ctx, Vec::new(), |ui| {
        draw_code(ui, &mut code, charset, &mut out)
    });
    let rect = out.as_ref().map(|o| o.response.rect).expect("drawn");
    click_at(&ctx, rect.center(), |ui| {
        draw_code(ui, &mut code, charset, &mut out)
    });
    run_with(&ctx, vec![Event::Paste("ab-cd ef".into())], |ui| {
        draw_code(ui, &mut code, charset, &mut out)
    });
    assert_eq!(code, "ABCDEF", "a whole pasted code replaces the cells");
    assert!(out.as_ref().is_some_and(|o| o.completed));
    let mut code = String::new();
    run_with(&ctx, vec![Event::Text("o0x1il".into())], |ui| {
        draw_code(ui, &mut code, charset, &mut out)
    });
    assert_eq!(code, "X", "look-alikes are refused");
}

/// Commands for the palette tests, in sections.
fn palette_items() -> Vec<PaletteItem<'static, u8>> {
    vec![
        PaletteItem::new(0, "Reset view").section("View"),
        PaletteItem::new(1, "Show settings").section("View"),
        PaletteItem::new(2, "Settings")
            .section("App")
            .shortcut("Ctrl+,"),
        PaletteItem::new(3, "New session")
            .icon(icons::PLUS)
            .section("App")
            .keywords(&["create"]),
        PaletteItem::new(4, "Close").section("App"),
    ]
}

#[test]
fn palette_results_rank_prefixes_and_word_starts_first() {
    let items = palette_items();
    let ranked = palette::entries(&palette::rows(&items, "set"));
    assert_eq!(ranked, [2, 1, 0]);
    let grouped = palette::entries(&palette::rows(&items, ""));
    assert_eq!(grouped, [0, 1, 2, 3, 4]);
    assert_eq!(palette::entries(&palette::rows(&items, "create")), [3]);
    let highlight = palette::rows(&items, "nse");
    assert!(matches!(
        &highlight[0],
        palette::Row::Item { index: 3, positions } if positions == &[0, 4, 5]
    ));
}

/// Shows the palette over one frame of `events`; returns what it picked.
fn palette_frame(
    ctx: &Context,
    state: &mut CommandPaletteState,
    items: &[PaletteItem<'static, u8>],
    events: Vec<Event>,
) -> Option<u8> {
    let mut picked = None;
    run_with(ctx, events, |ui| {
        picked = CommandPalette::new().show(ui.ctx(), state, items);
    });
    picked
}

#[test]
fn the_palette_moves_with_the_keys_and_runs_the_selection_on_enter() {
    let ctx = context(Density::Regular);
    let items = palette_items();
    let mut state = CommandPaletteState::new();
    assert_eq!(palette_frame(&ctx, &mut state, &items, Vec::new()), None);
    state.open();
    palette_frame(&ctx, &mut state, &items, Vec::new());
    palette_frame(&ctx, &mut state, &items, Vec::new());
    let down = key(egui::Key::ArrowDown);
    palette_frame(&ctx, &mut state, &items, vec![down.clone(), down.clone()]);
    assert_eq!(state.selected, 2);
    palette_frame(&ctx, &mut state, &items, vec![key(egui::Key::ArrowUp)]);
    assert_eq!(state.selected, 1);
    palette_frame(&ctx, &mut state, &items, vec![key(egui::Key::PageDown)]);
    assert_eq!(state.selected, 4, "a page stops at the last result");
    palette_frame(&ctx, &mut state, &items, vec![down]);
    assert_eq!(state.selected, 0, "the arrows wrap");
    let picked = palette_frame(&ctx, &mut state, &items, vec![key(egui::Key::Enter)]);
    assert_eq!(picked, Some(0));
    assert!(!state.is_open(), "running a command closes the palette");
}

#[test]
fn typing_filters_the_palette_and_escape_closes_it() {
    let ctx = context(Density::Regular);
    let items = palette_items();
    let mut state = CommandPaletteState::new();
    state.open();
    palette_frame(&ctx, &mut state, &items, Vec::new());
    palette_frame(&ctx, &mut state, &items, vec![Event::Text("sett".into())]);
    assert_eq!(state.query, "sett", "the query field holds focus");
    assert_eq!(state.selected, 0);
    let picked = palette_frame(&ctx, &mut state, &items, vec![key(egui::Key::Enter)]);
    assert_eq!(picked, Some(2), "the best match runs");
    state.open();
    palette_frame(&ctx, &mut state, &items, Vec::new());
    palette_frame(&ctx, &mut state, &items, vec![Event::Text("zzz".into())]);
    let shapes = painted(&ctx, 1, |ui| {
        let _ =
            CommandPalette::new()
                .empty_text("Nothing found")
                .show(ui.ctx(), &mut state, &items);
    });
    assert!(painted_texts(&shapes).iter().any(|t| t == "Nothing found"));
    let picked = palette_frame(&ctx, &mut state, &items, vec![key(egui::Key::Enter)]);
    assert_eq!(picked, None, "enter on no results runs nothing");
    palette_frame(&ctx, &mut state, &items, vec![key(egui::Key::Escape)]);
    assert!(!state.is_open());
}

#[test]
fn the_palette_focuses_its_query_on_open_and_leaves_a_surrendered_focus_alone() {
    let ctx = context(Density::Regular);
    let items = palette_items();
    let query = CommandPalette::new().query_id();
    let focused = || ctx.memory(|m| m.has_focus(query));
    let mut state = CommandPaletteState::new();
    state.open();
    palette_frame(&ctx, &mut state, &items, Vec::new());
    palette_frame(&ctx, &mut state, &items, Vec::new());
    assert!(focused(), "opening focuses the query");
    ctx.memory_mut(|m| m.surrender_focus(query));
    for _ in 0..3 {
        palette_frame(&ctx, &mut state, &items, Vec::new());
    }
    assert!(!focused(), "a surrendered query stays unfocused");
    assert!(state.is_open());
    state.close();
    palette_frame(&ctx, &mut state, &items, Vec::new());
    state.open = true;
    palette_frame(&ctx, &mut state, &items, Vec::new());
    palette_frame(&ctx, &mut state, &items, Vec::new());
    assert!(focused(), "reopening focuses the query again");
}

#[test]
fn back_closes_the_palette_like_escape() {
    let ctx = context(Density::Regular);
    let items = palette_items();
    let query = CommandPalette::new().query_id();
    let mut state = CommandPaletteState::new();
    state.open();
    palette_frame(&ctx, &mut state, &items, Vec::new());
    palette_frame(&ctx, &mut state, &items, vec![Event::Text("sett".into())]);
    ctx.memory_mut(|m| m.surrender_focus(query));
    palette_frame(&ctx, &mut state, &items, Vec::new());
    let picked = palette_frame(&ctx, &mut state, &items, vec![key(egui::Key::BrowserBack)]);
    assert_eq!(picked, None);
    assert!(!state.is_open(), "back closes the palette");
    assert!(state.query.is_empty());
    palette_frame(&ctx, &mut state, &items, Vec::new());
    assert!(!ctx.memory(|m| m.has_focus(query)));
}

#[test]
fn the_palette_fits_a_phone_screen() {
    let ctx = context(Density::Regular);
    let items = palette_items();
    let mut state = CommandPaletteState::new();
    state.open();
    let phone = |events| RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 800.0))),
        events,
        ..Default::default()
    };
    let mut shapes = Vec::new();
    for _ in 0..3 {
        let mut full = ctx.run_ui(phone(Vec::new()), |ui| {
            let _ = CommandPalette::new().show(ui.ctx(), &mut state, &items);
        });
        full.textures_delta.clear();
        shapes = full.shapes;
    }
    let screen = Rect::from_min_size(Pos2::ZERO, vec2(400.0, 800.0));
    let frame = shapes
        .iter()
        .filter_map(|c| match &c.shape {
            Shape::Rect(r) if r.rect.width() < 399.0 && r.rect.width() > 300.0 => Some(r.rect),
            _ => None,
        })
        .next()
        .expect("the palette frame");
    assert!(screen.contains_rect(frame), "{frame:?}");
}

/// One frame at `time` seconds with `events`.
fn run_at(ctx: &Context, time: f64, events: Vec<Event>, mut f: impl FnMut(&mut Ui)) -> Vec<Shape> {
    let raw = RawInput {
        time: Some(time),
        ..input(events)
    };
    let mut full = ctx.run_ui(raw, |ui| {
        CentralPanel::default().show(ui, |ui| f(ui));
    });
    full.textures_delta.clear();
    let mut out = Vec::new();
    fn flatten(shape: Shape, out: &mut Vec<Shape>) {
        match shape {
            Shape::Vec(shapes) => shapes.into_iter().for_each(|s| flatten(s, out)),
            other => out.push(other),
        }
    }
    for clipped in full.shapes {
        flatten(clipped.shape, &mut out);
    }
    out
}

#[test]
fn a_collapsed_nav_row_shows_its_label_only_as_a_tooltip() {
    let ctx = context(Density::Regular);
    let label = "Sessions";
    let mut rect = Rect::NOTHING;
    let draw = |ui: &mut Ui, rect: &mut Rect| {
        *rect = NavItem::new(icons::LIST, label)
            .collapsed(true)
            .show(ui)
            .rect;
    };
    let shapes = run_at(&ctx, 0.0, Vec::new(), |ui| draw(ui, &mut rect));
    assert!(!painted_texts(&shapes).iter().any(|t| t == label));
    assert_eq!(
        NavItem::new(icons::LIST, label)
            .collapsed(true)
            .hover_text(),
        Some(label)
    );
    assert_eq!(NavItem::new(icons::LIST, label).hover_text(), None);
    let p = rect.center();
    run_at(&ctx, 0.1, vec![Event::PointerMoved(p)], |ui| {
        draw(ui, &mut rect)
    });
    let mut shown = false;
    for k in 1..6 {
        let shapes = run_at(&ctx, 0.1 + k as f64, Vec::new(), |ui| draw(ui, &mut rect));
        shown |= painted_texts(&shapes).iter().any(|t| t == label);
    }
    assert!(shown, "hovering shows the label");
    let expanded = run_at(&ctx, 10.0, Vec::new(), |ui| {
        let _ = NavItem::new(icons::LIST, label).width(200.0).show(ui);
    });
    assert!(painted_texts(&expanded).iter().any(|t| t == label));
}

#[test]
fn a_pulsing_dot_repaints_and_a_still_one_does_not() {
    let ctx = context(Density::Regular);
    let repaint = |pulse: bool| {
        let mut full = ctx.run_ui(input(Vec::new()), |ui| {
            CentralPanel::default().show(ui, |ui| {
                let _ = StatusDot::new(Color32::GREEN).pulse(pulse).show(ui);
            });
        });
        full.textures_delta.clear();
        full.viewport_output
            .values()
            .any(|v| v.repaint_delay == std::time::Duration::ZERO)
    };
    for _ in 0..4 {
        let _ = repaint(false);
    }
    assert!(!repaint(false));
    assert!(repaint(true));
}

#[cfg(feature = "qr")]
#[test]
fn a_qr_code_paints_dark_modules_on_a_light_tile_and_a_placeholder_when_too_long() {
    let ctx = context(Density::Regular);
    let fills = |data: String| {
        let shapes = painted(&ctx, 2, |ui| {
            CentralPanel::default().show(ui, |ui| {
                let _ = QrCode::new(&data).size(120.0).caption("Scan").show(ui);
            });
        });
        shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Rect(r) => Some((r.fill, r.rect)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let ok = fills("https://example.com/pair?code=ABCD".to_owned());
    let tile = ok
        .iter()
        .find(|(fill, _)| *fill == Color32::WHITE)
        .map(|(_, r)| *r)
        .expect("a light tile");
    assert_eq!(tile.size(), Vec2::splat(120.0));
    let dark: Vec<Rect> = ok
        .iter()
        .filter(|(fill, _)| *fill == Color32::BLACK)
        .map(|(_, r)| *r)
        .collect();
    assert!(dark.len() > 20, "{} dark runs", dark.len());
    for r in &dark {
        assert!(tile.contains_rect(*r), "{r:?} leaves the tile");
    }
    let quiet = tile.shrink(4.0 * 120.0 / 37.0 - 1.0);
    assert!(
        dark.iter().all(|r| quiet.contains_rect(*r)),
        "the quiet zone stays clear"
    );
    let too_long = fills("x".repeat(5000));
    assert!(!too_long.iter().any(|(fill, _)| *fill == Color32::BLACK));
    assert!(!too_long.iter().any(|(fill, _)| *fill == Color32::WHITE));
}

#[test]
fn the_dashboard_builders_take_glyph_strings_and_draw_under_every_preset() {
    let ctx = themed_context();
    for preset in MATCHABLE_PRESETS {
        apply_preset(&ctx, preset);
        run(&ctx, 2, |ui| {
            let _ = NavItem::new("G", "Glyph").selected(true).show(ui);
            let _ = IconChip::new("G").show(ui);
            let _ = StatTile::new("G", "1", "Glyph").show(ui);
            let _ = ActionTile::new("G", "Glyph").mark(TileMark::Busy).show(ui);
            let _ = status_chip(ui, "G", "Glyph", Color32::GREEN);
            let _ = PaletteItem::new(0, "Glyph").icon("G");
            let _ = Readout::new("", "ms").caption("Empty").show(ui);
            let _ = LineChart::new(&[]).height(60.0).show(ui);
        });
    }
}
