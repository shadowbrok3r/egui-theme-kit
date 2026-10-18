use egui::{Color32, Context, Image, Pos2, Vec2};

use crate::pack::is_sorted;
use crate::{Icon, IconPack, IconRegistry, IconSize, Pack, gallery, widgets};

const SQUARE: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" fill="#ffffff"><path d="M32,32H224V224H32Z"/></svg>"##;
const RING: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" fill="none" stroke="#ffffff" stroke-width="16"><circle cx="128" cy="128" r="80"/></svg>"##;

const fn icon(
    name: &'static str,
    uri: &'static str,
    duo: Option<&'static [u8]>,
    glyph: &'static str,
    custom: bool,
) -> Icon {
    Icon::new(
        name,
        uri,
        "bytes://test/duo/any.svg",
        SQUARE,
        duo,
        glyph,
        custom,
    )
}

const SAMPLE: &[Icon] = &[
    icon(
        "alpha",
        "bytes://test/alpha.svg",
        Some(RING),
        "\u{e001}",
        false,
    ),
    icon("beta", "bytes://test/beta.svg", None, "\u{e002}", false),
    icon(
        "beta-custom",
        "bytes://test/beta-custom.svg",
        Some(RING),
        "\u{e002}",
        true,
    ),
    icon("gamma", "bytes://test/gamma.svg", None, "\u{e003}", false),
    icon(
        "gamma-2",
        "bytes://test/gamma-2.svg",
        None,
        "\u{e003}",
        false,
    ),
];
const SAMPLE_PACK: Pack = Pack::new(SAMPLE);

#[cfg(feature = "font")]
fn rasterise(name: &str, bytes: &[u8], px: u32) -> resvg::tiny_skia::Pixmap {
    let text = std::str::from_utf8(bytes).unwrap_or_else(|e| panic!("{name}: not UTF-8: {e}"));
    assert!(
        text.trim_start().starts_with("<svg"),
        "{name}: not an <svg> root"
    );
    assert!(
        !text.contains("currentColor"),
        "{name}: currentColor cannot be tinted"
    );
    let tree = resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default())
        .unwrap_or_else(|e| panic!("{name}: {e}"));
    assert_eq!(tree.size().width(), 256.0, "{name}: not on the 256 grid");
    assert_eq!(tree.size().height(), 256.0, "{name}: not on the 256 grid");
    let mut pixmap = resvg::tiny_skia::Pixmap::new(px, px).expect("non-zero pixmap");
    let scale = px as f32 / 256.0;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap
}

#[cfg(feature = "font")]
fn coverage(pixmap: &resvg::tiny_skia::Pixmap) -> f32 {
    let lit = pixmap.pixels().iter().filter(|p| p.alpha() > 127).count();
    lit as f32 / pixmap.pixels().len() as f32
}

fn installed() -> Context {
    let ctx = Context::default();
    crate::install(&ctx);
    ctx
}

#[test]
fn icon_exposes_its_parts() {
    let alpha = SAMPLE[0];
    assert_eq!(alpha.name(), "alpha");
    assert_eq!(alpha.uri(), "bytes://test/alpha.svg");
    assert_eq!(alpha.glyph(), "\u{e001}");
    assert_eq!(alpha.svg(), SQUARE);
    assert_eq!(alpha.duo_svg(), Some(RING));
    assert!(alpha.has_duo() && !alpha.is_custom());
    assert!(SAMPLE[2].is_custom() && !SAMPLE[1].has_duo());
    assert_eq!(SAMPLE[4].const_name(), "GAMMA_2");
    assert_eq!(
        format!("{alpha:?}"),
        "Icon { name: \"alpha\", has_duo: true, custom: false }"
    );
}

#[test]
fn icon_equality_includes_the_image_key() {
    assert_eq!(SAMPLE[0], SAMPLE[0]);
    assert_ne!(
        SAMPLE[0],
        icon("alpha", "bytes://other/alpha.svg", None, "", false)
    );
}

#[test]
fn sorted_check_requires_strictly_increasing_names() {
    assert!(is_sorted(SAMPLE));
    assert!(is_sorted(&[]));
    assert!(!is_sorted(&[SAMPLE[1], SAMPLE[0]]));
    assert!(!is_sorted(&[SAMPLE[0], SAMPLE[0]]));
    assert!(
        is_sorted(&[SAMPLE[1], SAMPLE[2]]),
        "a prefix sorts before its extension"
    );
}

#[test]
fn pack_finds_icons_by_name() {
    for icon in SAMPLE {
        assert_eq!(SAMPLE_PACK.find(icon.name()), Some(*icon));
    }
    assert_eq!(SAMPLE_PACK.find("delta"), None);
    assert_eq!(SAMPLE_PACK.len(), 5);
    assert!(!SAMPLE_PACK.is_empty());
    assert_eq!(SAMPLE_PACK.iter().count(), 5);
    assert_eq!(SAMPLE_PACK.into_iter().next(), Some(SAMPLE[0]));
    assert_eq!(SAMPLE_PACK.as_slice().len(), 5);
}

#[test]
fn from_glyph_skips_custom_icons_and_prefers_the_last_owner() {
    assert_eq!(SAMPLE_PACK.from_glyph("\u{e001}"), Some(SAMPLE[0]));
    assert_eq!(SAMPLE_PACK.from_glyph("\u{e002}"), Some(SAMPLE[1]));
    assert_eq!(SAMPLE_PACK.from_glyph("\u{e003}"), Some(SAMPLE[4]));
    assert_eq!(SAMPLE_PACK.from_glyph("x"), None);
    assert_eq!(SAMPLE_PACK.from_glyph(""), None);
}

#[test]
fn slices_arrays_vecs_and_references_are_packs() {
    assert_eq!(SAMPLE.find("gamma"), Some(SAMPLE[3]));
    assert_eq!([SAMPLE[0], SAMPLE[1]].find("beta"), Some(SAMPLE[1]));
    assert_eq!(SAMPLE.to_vec().find("alpha"), Some(SAMPLE[0]));
    let by_ref: &dyn IconPack = &SAMPLE_PACK;
    assert_eq!(by_ref.find("alpha"), Some(SAMPLE[0]));
    assert_eq!((&&SAMPLE_PACK).icons().len(), 5);
}

#[test]
fn registry_merges_sorted_and_earlier_packs_win() {
    let other: &[Icon] = &[
        icon("alpha", "bytes://test/alpha.svg", None, "\u{e0ff}", false),
        icon(
            "aardvark",
            "bytes://test/aardvark.svg",
            None,
            "\u{e004}",
            false,
        ),
        icon("zeta", "bytes://test/zeta.svg", None, "\u{e005}", false),
    ];
    let mut sorted_other = other.to_vec();
    sorted_other.sort_by_key(|i| i.name());
    let registry = IconRegistry::new().with(&SAMPLE_PACK).with(&sorted_other);
    let names: Vec<_> = registry.iter().map(Icon::name).collect();
    assert_eq!(
        names,
        [
            "aardvark",
            "alpha",
            "beta",
            "beta-custom",
            "gamma",
            "gamma-2",
            "zeta"
        ]
    );
    assert_eq!(registry.find("alpha").map(Icon::glyph), Some("\u{e001}"));
    assert_eq!(registry.find("zeta").map(Icon::name), Some("zeta"));
    assert_eq!(
        registry.from_glyph("\u{e005}").map(Icon::name),
        Some("zeta")
    );
    assert!(IconRegistry::new().is_empty());
}

#[test]
fn snapped_square_is_pixel_exact() {
    for ppp in [1.0, 1.25, 1.5, 2.0] {
        for x in [10.0, 10.3, 10.5, 10.77] {
            let r = crate::snapped_square(Pos2::new(x, x + 0.2), 16.0, ppp);
            let px = r.size() * ppp;
            assert!((px.x - (16.0 * ppp).round()).abs() < 1e-3);
            assert!(((r.min.x * ppp) - (r.min.x * ppp).round()).abs() < 1e-3);
        }
    }
}

#[test]
fn icon_size_follows_body_font() {
    let ctx = Context::default();
    let mut edges = Vec::new();
    let mut output = ctx.run_ui(Default::default(), |ui| {
        edges = [IconSize::Sm, IconSize::Md, IconSize::Lg, IconSize::Tool]
            .map(|s| s.px(ui))
            .to_vec();
    });
    output.textures_delta.clear();
    assert!(edges.windows(2).all(|w| w[0] < w[1]), "{edges:?}");
    assert!(edges[0] >= 8.0);
}

#[test]
fn show_allocates_a_square_and_paints() {
    let ctx = installed();
    let mut sizes = Vec::new();
    let mut output = ctx.run_ui(Default::default(), |ui| {
        sizes.push(SAMPLE[0].show(ui, 24.0).rect.size());
        sizes.push(SAMPLE[1].show_tinted(ui, 16.0, Color32::RED).rect.size());
        let rect = ui.available_rect_before_wrap();
        SAMPLE[0].paint_duo(ui, rect, 20.0, Color32::WHITE, Color32::GRAY);
    });
    output.textures_delta.clear();
    assert_eq!(sizes, [Vec2::splat(24.0), Vec2::splat(16.0)]);
    assert!(!output.shapes.is_empty());
}

#[test]
fn widgets_draw_without_panicking() {
    let ctx = installed();
    let mut output = ctx.run_ui(Default::default(), |ui| {
        let (a, b, c) = (SAMPLE[0], SAMPLE[1], SAMPLE[2]);
        widgets::button(ui, a, "Settings");
        widgets::small_button(ui, b, "Settings");
        widgets::selectable_button(ui, c, true, "Starred");
        widgets::icon_button(ui, a, "Delete");
        widgets::icon_button_tinted(ui, b, "Delete", Color32::RED);
        widgets::toggle_button(ui, c, false, "Preview");
        widgets::label(ui, a, "CPU");
        widgets::heading(ui, b, "Refine");
        widgets::checkbox(ui, &mut true, c, "Assistant");
        widgets::menu_button(ui, a, "Menu", |ui| {
            widgets::button(ui, b, "Entry");
        });
        widgets::icon_menu_button(ui, c, "Icon menu", |ui| {
            widgets::button(ui, a, "Entry");
        });
    });
    output.textures_delta.clear();
}

#[test]
fn gallery_draws_a_pack_without_panicking() {
    let ctx = installed();
    gallery::set_open(&ctx, true);
    assert!(gallery::is_open(&ctx));
    for _ in 0..2 {
        let mut output = ctx.run_ui(Default::default(), |ui| {
            gallery::show_window(ui.ctx(), &SAMPLE_PACK);
            gallery::show(ui, &SAMPLE_PACK);
        });
        output.textures_delta.clear();
    }
    gallery::toggle(&ctx);
    assert!(!gallery::is_open(&ctx));
}

#[test]
fn the_registered_pack_is_drawn_without_passing_it_again() {
    let ctx = installed();
    assert!(gallery::pack(&ctx).is_none());
    let mut output = ctx.run_ui(Default::default(), |ui| {
        gallery::show_pack_window(ui.ctx());
        gallery::show_pack(ui);
    });
    output.textures_delta.clear();

    gallery::set_pack(&ctx, SAMPLE_PACK);
    assert_eq!(gallery::pack(&ctx).map(Pack::len), Some(SAMPLE.len()));
    gallery::set_open(&ctx, true);
    for _ in 0..2 {
        let mut output = ctx.run_ui(Default::default(), |ui| {
            gallery::show_pack_window(ui.ctx());
            gallery::show_pack(ui);
        });
        output.textures_delta.clear();
    }
}

#[test]
fn gallery_survives_empty_and_tiny_packs() {
    let ctx = installed();
    let empty: &[Icon] = &[];
    let mut output = ctx.run_ui(Default::default(), |ui| {
        gallery::show(ui, &empty);
        gallery::show(ui, &&SAMPLE[..1]);
    });
    output.textures_delta.clear();
}

#[test]
fn loads_both_layers_through_the_svg_loader() {
    let ctx = installed();
    let icon = SAMPLE[0];
    let duo = icon.duo_source().expect("duo layer");
    for source in [icon.source(), duo] {
        let image = Image::new(source).fit_to_exact_size(Vec2::splat(24.0));
        match image.load_for_size(&ctx, Vec2::splat(100.0)) {
            Ok(egui::load::TexturePoll::Ready { texture }) => {
                let raster = ctx.tex_manager().read().meta(texture.id).map(|m| m.size);
                assert_eq!(raster, Some([24, 24]));
            }
            Ok(_) => panic!("still loading"),
            Err(e) => panic!("{e}"),
        }
    }
}

#[test]
fn install_is_idempotent() {
    let ctx = Context::default();
    crate::install(&ctx);
    crate::install(&ctx);
}

#[cfg(feature = "font")]
mod font {
    use egui::{FontDefinitions, FontFamily};

    use crate::{FONT_KEY, icon, icon_colored, icon_sized, install_fonts, phosphor};

    fn keys(fonts: &FontDefinitions, family: &FontFamily) -> Vec<String> {
        fonts.families[family].clone()
    }

    #[test]
    fn install_registers_phosphor_in_every_known_family() {
        let mut fonts = FontDefinitions::default();
        for name in ["Regular", "Bold"] {
            let base = keys(&fonts, &FontFamily::Proportional)
                .into_iter()
                .filter(|k| k != FONT_KEY)
                .collect();
            fonts.families.insert(FontFamily::Name(name.into()), base);
        }
        install_fonts(&mut fonts);
        assert!(fonts.font_data.contains_key(FONT_KEY));
        for family in [
            FontFamily::Proportional,
            FontFamily::Monospace,
            FontFamily::Name("Regular".into()),
            FontFamily::Name("Bold".into()),
        ] {
            let keys = keys(&fonts, &family);
            assert_eq!(
                keys.iter().filter(|k| *k == FONT_KEY).count(),
                1,
                "{family:?}: {keys:?}"
            );
            assert_ne!(
                keys[0], FONT_KEY,
                "{family:?} must keep its text font first"
            );
        }
    }

    #[test]
    fn install_is_idempotent() {
        let mut once = FontDefinitions::default();
        install_fonts(&mut once);
        let mut twice = once.clone();
        install_fonts(&mut twice);
        assert_eq!(once.families, twice.families);
    }

    #[test]
    fn installed_fonts_lay_out_glyphs() {
        let ctx = egui::Context::default();
        let mut fonts = FontDefinitions::default();
        install_fonts(&mut fonts);
        ctx.set_fonts(fonts);
        let mut output = ctx.run_ui(Default::default(), |ui| {
            let glyph = phosphor::regular::GEAR;
            ui.label(icon(glyph));
            ui.label(icon_sized(glyph, 20.0));
            ui.label(icon_colored(glyph, egui::Color32::RED));
        });
        output.textures_delta.clear();
    }
}

#[cfg(feature = "phosphor-regular")]
mod bundled {
    use std::collections::HashSet;
    use std::path::Path;

    use super::*;
    use crate::phosphor;
    use crate::phosphor_regular::{self as svg, ALL, BORROWED, PACK};

    const ASSETS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/phosphor");

    fn stems(dir: &str) -> Vec<String> {
        let mut out: Vec<String> = std::fs::read_dir(Path::new(ASSETS).join(dir))
            .expect("asset dir")
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "svg"))
            .filter_map(|p| p.file_stem().and_then(|s| s.to_str()).map(str::to_owned))
            .collect();
        out.sort();
        out
    }

    #[test]
    fn names_are_sorted_unique_kebab_case() {
        assert!(ALL.windows(2).all(|w| w[0].name() < w[1].name()));
        let uris: HashSet<_> = ALL.iter().map(|i| i.uri()).collect();
        assert_eq!(uris.len(), ALL.len());
        assert!(ALL.len() >= 400, "pack shrank to {}", ALL.len());
        for icon in ALL {
            let name = icon.name();
            assert!(name.starts_with(|c: char| c.is_ascii_lowercase()), "{name}");
            assert!(
                name.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "{name}"
            );
            assert!(!name.ends_with('-') && !name.contains("--"), "{name}");
        }
    }

    #[test]
    fn every_svg_file_has_a_const() {
        let names: Vec<_> = ALL.iter().map(|i| i.name().to_owned()).collect();
        assert_eq!(stems("regular"), names);
        assert_eq!(PACK.len(), ALL.len());
    }

    #[test]
    fn find_round_trips() {
        for icon in ALL {
            assert_eq!(PACK.find(icon.name()), Some(*icon));
        }
        assert_eq!(PACK.find("cpu"), Some(svg::CPU));
        assert_eq!(PACK.find("hard-drive"), Some(svg::HARD_DRIVE));
        assert_eq!(PACK.find("no-such-icon"), None);
    }

    #[test]
    fn from_glyph_round_trips_plain_icons() {
        for icon in ALL.iter().filter(|i| !i.is_custom()) {
            let found = PACK
                .from_glyph(icon.glyph())
                .unwrap_or_else(|| panic!("{}", icon.name()));
            assert_eq!(found.glyph(), icon.glyph(), "{}", icon.name());
            assert!(!found.is_custom());
        }
        assert_eq!(PACK.from_glyph(phosphor::regular::CPU), Some(svg::CPU));
        assert_eq!(PACK.from_glyph("x"), None);
    }

    #[test]
    fn borrowed_icons_declare_a_glyph_and_are_listed() {
        let custom: Vec<_> = ALL.iter().filter(|i| i.is_custom()).collect();
        assert_eq!(BORROWED.len(), custom.len());
        for icon in &custom {
            assert!(BORROWED.contains(&icon.name()), "{}", icon.name());
            let text = std::str::from_utf8(icon.svg()).unwrap();
            assert!(
                text.contains("data-glyph=\""),
                "{}: missing data-glyph",
                icon.name()
            );
            let owner = PACK
                .from_glyph(icon.glyph())
                .expect("borrowed glyph has an owner");
            assert!(!owner.is_custom());
        }
        assert!(svg::CROWN_SIMPLE.is_custom());
        assert_eq!(
            PACK.from_glyph(svg::CROWN_SIMPLE.glyph()),
            Some(svg::USER_GEAR)
        );
    }

    #[test]
    fn glyphs_are_single_private_use_chars() {
        for icon in ALL {
            let mut chars = icon.glyph().chars();
            let c = chars
                .next()
                .unwrap_or_else(|| panic!("{} has an empty glyph", icon.name()));
            assert!(
                chars.next().is_none(),
                "{} glyph is not one char",
                icon.name()
            );
            assert!(
                ('\u{E000}'..='\u{F8FF}').contains(&c),
                "{} glyph is outside the PUA",
                icon.name()
            );
        }
    }

    #[test]
    fn uris_are_namespaced_and_end_in_svg() {
        for icon in ALL {
            assert_eq!(
                icon.uri(),
                format!(
                    "bytes://egui-icon-pack/phosphor-regular/{}.svg",
                    icon.name()
                )
            );
            assert_eq!(
                icon.duo_uri(),
                format!(
                    "bytes://egui-icon-pack/phosphor-regular/duo/{}.svg",
                    icon.name()
                )
            );
        }
    }

    #[test]
    fn every_layer_parses_and_renders_white() {
        for icon in ALL {
            let small = rasterise(icon.name(), icon.svg(), 16);
            assert!(
                small.pixels().iter().any(|p| p.alpha() > 0),
                "{}: empty at 16 px",
                icon.name()
            );
            let front = rasterise(icon.name(), icon.svg(), 48);
            let cov = coverage(&front);
            assert!(
                (0.003..0.75).contains(&cov),
                "{}: front coverage {cov}",
                icon.name()
            );
            for p in front.pixels().iter().filter(|p| p.alpha() == 255) {
                assert_eq!(
                    (p.red(), p.green(), p.blue()),
                    (255, 255, 255),
                    "{}: non-white ink",
                    icon.name()
                );
            }
            if let Some(duo) = icon.duo_svg() {
                let small = rasterise(icon.name(), duo, 16);
                assert!(
                    small.pixels().iter().any(|p| p.alpha() > 0),
                    "{}: empty duo at 16 px",
                    icon.name()
                );
                assert!(
                    coverage(&rasterise(icon.name(), duo, 48)) > 0.003,
                    "{}: empty duo layer",
                    icon.name()
                );
            }
        }
    }

    #[test]
    fn duo_layers_pair_with_regular_icons() {
        let regular: HashSet<_> = stems("regular").into_iter().collect();
        let duo = stems("duo");
        assert!(duo.len() >= 400);
        for name in &duo {
            assert!(
                regular.contains(name),
                "duo/{name}.svg has no regular/{name}.svg"
            );
            assert!(
                PACK.find(name).is_some_and(|i| i.has_duo()),
                "{name}: duo not wired"
            );
        }
        assert_eq!(ALL.iter().filter(|i| i.has_duo()).count(), duo.len());
        assert!(svg::CPU.has_duo() && svg::GEAR.has_duo());
    }

    #[test]
    fn loads_regular_and_duo_layers_through_the_svg_loader() {
        let ctx = installed();
        for icon in [svg::CPU, svg::HARD_DRIVE, svg::GEAR, svg::CROWN_SIMPLE] {
            let duo = icon.duo_source().expect("duo layer");
            for source in [icon.source(), duo] {
                let image = Image::new(source).fit_to_exact_size(Vec2::splat(24.0));
                match image.load_for_size(&ctx, Vec2::splat(100.0)) {
                    Ok(egui::load::TexturePoll::Ready { texture }) => {
                        let raster = ctx.tex_manager().read().meta(texture.id).map(|m| m.size);
                        assert_eq!(raster, Some([24, 24]), "{}", icon.name());
                    }
                    Ok(_) => panic!("{}: still loading", icon.name()),
                    Err(e) => panic!("{}: {e}", icon.name()),
                }
            }
        }
    }

    #[test]
    fn widgets_and_gallery_draw_the_bundled_pack() {
        let ctx = installed();
        let mut output = ctx.run_ui(Default::default(), |ui| {
            widgets::button(ui, svg::GEAR, "Settings");
            widgets::icon_button(ui, svg::TRASH, "Delete");
            widgets::toggle_button(ui, svg::EYE, false, "Preview");
            widgets::menu_button(ui, svg::DATABASE, "Database", |ui| {
                widgets::button(ui, svg::DOWNLOAD, "Export");
            });
            gallery::show(ui, &PACK);
        });
        output.textures_delta.clear();
    }

    #[test]
    fn registry_merges_the_bundled_pack_with_custom_icons() {
        let custom: &[Icon] = &[icon(
            "zz-custom",
            "bytes://test/zz-custom.svg",
            None,
            phosphor::regular::GAUGE,
            true,
        )];
        let registry = IconRegistry::new().with(&custom).with(&PACK);
        assert_eq!(registry.len(), PACK.len() + 1);
        assert_eq!(
            registry.find("zz-custom").map(Icon::name),
            Some("zz-custom")
        );
        assert_eq!(registry.find("cpu"), Some(svg::CPU));
    }

    #[test]
    fn assets_carry_their_license() {
        for file in ["LICENSE", "ATTRIBUTION"] {
            let text = std::fs::read_to_string(Path::new(ASSETS).join(file)).expect(file);
            assert!(text.contains("Phosphor"), "{file}");
        }
    }
}

#[cfg(feature = "font")]
mod generated {
    use super::*;
    use crate::phosphor;

    #[allow(dead_code)]
    mod table {
        include!("../testdata/custom_pack.rs");
    }
    use table::{ALL, BORROWED, DEMO_ORBIT, DEMO_SIGNAL, DEMO_STACK, PACK, STAR};

    #[test]
    fn table_is_sorted_and_wrapped_in_a_pack() {
        assert!(is_sorted(ALL));
        let names: Vec<_> = PACK.iter().map(Icon::name).collect();
        assert_eq!(names, ["demo-orbit", "demo-signal", "demo-stack", "star"]);
        assert_eq!(PACK.find("demo-stack"), Some(DEMO_STACK));
        assert_eq!(PACK.find("missing"), None);
    }

    #[test]
    fn custom_icons_borrow_glyphs_and_are_listed() {
        assert_eq!(BORROWED, ["demo-orbit", "demo-signal", "demo-stack"]);
        for icon in [DEMO_ORBIT, DEMO_SIGNAL, DEMO_STACK] {
            assert!(icon.is_custom(), "{}", icon.name());
        }
        assert!(!STAR.is_custom());
        assert_eq!(DEMO_ORBIT.glyph(), phosphor::regular::ATOM);
        assert_eq!(STAR.glyph(), phosphor::regular::STAR);
        assert_eq!(PACK.from_glyph(phosphor::regular::STAR), Some(STAR));
        assert_eq!(PACK.from_glyph(phosphor::regular::ATOM), None);
    }

    #[test]
    fn layers_and_uris_follow_the_prefix() {
        for icon in ALL {
            assert!(icon.has_duo(), "{}", icon.name());
            assert_eq!(
                icon.uri(),
                format!("bytes://custom-pack-example/icons/{}.svg", icon.name())
            );
            assert_eq!(
                icon.duo_uri(),
                format!("bytes://custom-pack-example/icons/duo/{}.svg", icon.name())
            );
        }
    }

    #[test]
    fn every_layer_renders_white_ink() {
        for icon in ALL {
            let front = rasterise(icon.name(), icon.svg(), 48);
            assert!(coverage(&front) > 0.003, "{}: empty", icon.name());
            for p in front.pixels().iter().filter(|p| p.alpha() == 255) {
                assert_eq!(
                    (p.red(), p.green(), p.blue()),
                    (255, 255, 255),
                    "{}",
                    icon.name()
                );
            }
            let duo = icon.duo_svg().expect("duo layer");
            assert!(
                coverage(&rasterise(icon.name(), duo, 48)) > 0.003,
                "{}: empty duo",
                icon.name()
            );
        }
    }

    #[test]
    fn widgets_and_gallery_draw_the_generated_pack() {
        let ctx = installed();
        let mut output = ctx.run_ui(Default::default(), |ui| {
            widgets::button(ui, DEMO_ORBIT, "Orbit");
            widgets::icon_button(ui, DEMO_SIGNAL, "Signal");
            gallery::show(ui, &PACK);
        });
        output.textures_delta.clear();
    }
}
