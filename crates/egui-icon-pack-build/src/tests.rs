use std::fs;
use std::path::Path;

use tempfile::TempDir;

use super::*;

const WHITE_FILL: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" fill="#ffffff"><path d="M8,8H248V248H8Z"/></svg>"##;

fn custom_svg(glyph: &str) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" fill="none" stroke="#ffffff" stroke-width="16" stroke-linecap="round" stroke-linejoin="round" data-glyph="{glyph}"><circle cx="128" cy="128" r="80"/></svg>"##
    )
}

struct Fixture {
    dir: TempDir,
}

impl Fixture {
    fn new(files: &[(&str, &str)]) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        for (path, text) in files {
            let full = dir.path().join("icons").join(path);
            fs::create_dir_all(full.parent().expect("parent")).expect("mkdir");
            fs::write(full, text).expect("write");
        }
        Self { dir }
    }

    fn build(&self) -> IconPackBuild {
        IconPackBuild {
            uri_prefix: "demo-icons".into(),
            manifest_dir: Some(self.dir.path().to_owned()),
            emit_cargo_directives: false,
            ..IconPackBuild::new(
                self.dir.path().join("icons"),
                self.dir.path().join("out/table.rs"),
            )
        }
    }
}

fn render(fixture: &Fixture) -> String {
    fixture.build().render().expect("render").0
}

fn error(fixture: &Fixture) -> String {
    fixture
        .build()
        .render()
        .expect_err("expected an error")
        .to_string()
}

#[test]
fn emits_sorted_consts_all_and_pack() {
    let fx = Fixture::new(&[
        ("regular/zoom-in.svg", WHITE_FILL),
        ("regular/cpu.svg", WHITE_FILL),
        ("regular/file-3d.svg", WHITE_FILL),
    ]);
    let (source, summary) = fx.build().render().expect("render");
    assert_eq!(
        summary,
        Generated {
            icons: 3,
            custom: 0,
            with_duo: 0,
            written: true
        }
    );
    for ident in ["CPU", "FILE_3D", "ZOOM_IN"] {
        assert!(
            source.contains(&format!("pub const {ident}: egui_icon_pack::Icon")),
            "{ident}"
        );
    }
    let all = source.split("pub const ALL").nth(1).expect("ALL");
    let (cpu, file, zoom) = (
        all.find("CPU,").expect("CPU"),
        all.find("FILE_3D,").expect("FILE_3D"),
        all.find("ZOOM_IN,").expect("ZOOM_IN"),
    );
    assert!(cpu < file && file < zoom, "ALL must be sorted by name");
    assert!(
        source.contains("pub const PACK: egui_icon_pack::Pack = egui_icon_pack::Pack::new(ALL);")
    );
    assert!(source.contains("\"bytes://demo-icons/cpu.svg\""));
    assert!(source.contains("\"bytes://demo-icons/duo/cpu.svg\""));
    assert!(source.contains("egui_icon_pack::phosphor::regular::FILE_3D,"));
    assert!(source.contains("pub const BORROWED: &[&str] = &[\n];"));
}

#[test]
fn manifest_relative_paths_use_cargo_manifest_dir() {
    let fx = Fixture::new(&[("regular/cpu.svg", WHITE_FILL)]);
    let source = render(&fx);
    assert!(
        source.contains(
            "include_bytes!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/icons/regular/cpu.svg\"))"
        ),
        "{source}"
    );
}

#[test]
fn paths_outside_the_manifest_are_absolute() {
    let fx = Fixture::new(&[("regular/cpu.svg", WHITE_FILL)]);
    let other = tempfile::tempdir().expect("tempdir");
    let build = IconPackBuild {
        manifest_dir: Some(other.path().to_owned()),
        ..fx.build()
    };
    let source = build.render().expect("render").0;
    let expected = fx.dir.path().join("icons/regular/cpu.svg");
    assert!(
        source.contains(&format!("include_bytes!({:?})", expected.to_string_lossy())),
        "{source}"
    );
    assert!(!source.contains("CARGO_MANIFEST_DIR"));
}

#[test]
fn custom_icons_borrow_a_glyph_and_are_listed() {
    let fx = Fixture::new(&[
        ("regular/cpu.svg", WHITE_FILL),
        ("regular/demo-gauge.svg", &custom_svg("GAUGE")),
    ]);
    let (source, summary) = fx.build().render().expect("render");
    assert_eq!(summary.custom, 1);
    assert!(source.contains("egui_icon_pack::phosphor::regular::GAUGE,\n    true,"));
    assert!(source.contains("egui_icon_pack::phosphor::regular::CPU,\n    false,"));
    assert!(source.contains("pub const BORROWED: &[&str] = &[\n    \"demo-gauge\",\n];"));
}

#[test]
fn custom_prefix_requires_data_glyph() {
    let fx = Fixture::new(&[("regular/demo-gauge.svg", WHITE_FILL)]);
    let build = IconPackBuild {
        custom_prefix: Some("demo-".into()),
        ..fx.build()
    };
    let err = build.render().expect_err("missing glyph").to_string();
    assert!(
        err.contains("demo-gauge.svg") && err.contains("data-glyph"),
        "{err}"
    );
}

#[test]
fn without_custom_prefix_missing_glyph_means_phosphor_name() {
    let fx = Fixture::new(&[("regular/demo-gauge.svg", WHITE_FILL)]);
    let source = render(&fx);
    assert!(source.contains("egui_icon_pack::phosphor::regular::DEMO_GAUGE,"));
}

#[test]
fn rejects_malformed_glyph_names() {
    for glyph in ["gauge", "9GAUGE", "GAUGE-2", ""] {
        let fx = Fixture::new(&[("regular/demo-gauge.svg", &custom_svg(glyph))]);
        let err = error(&fx);
        assert!(err.contains("data-glyph"), "{glyph}: {err}");
    }
}

#[test]
fn rejects_names_that_are_not_kebab_case() {
    for name in [
        "Bad",
        "two--hyphens",
        "9lives",
        "trailing-",
        "has_underscore",
        "space name",
        "ünï",
    ] {
        let fx = Fixture::new(&[(&format!("regular/{name}.svg"), WHITE_FILL)]);
        let err = error(&fx);
        assert!(err.contains("kebab-case"), "{name}: {err}");
    }
}

#[test]
fn rejects_names_that_collide_with_generated_items() {
    for name in ["all", "borrowed", "pack"] {
        let fx = Fixture::new(&[(&format!("regular/{name}.svg"), WHITE_FILL)]);
        let err = error(&fx);
        assert!(err.contains("already uses"), "{name}: {err}");
    }
}

#[test]
fn rejects_untintable_or_misdrawn_svgs() {
    let cases = [
        (
            WHITE_FILL.replace("#ffffff", "currentColor"),
            "currentColor",
        ),
        (WHITE_FILL.replace("0 0 256 256", "0 0 24 24"), "viewBox"),
        (
            WHITE_FILL.replace(" viewBox=\"0 0 256 256\"", ""),
            "viewBox",
        ),
        (
            "<html><svg viewBox=\"0 0 256 256\"/></html>".to_owned(),
            "not an <svg>",
        ),
        ("not svg at all".to_owned(), "not an <svg>"),
    ];
    for (text, needle) in cases {
        let fx = Fixture::new(&[("regular/cpu.svg", &text)]);
        let err = error(&fx);
        assert!(
            err.contains(needle) && err.contains("regular/cpu.svg"),
            "{needle}: {err}"
        );
    }
}

#[test]
fn accepts_xml_declaration_comments_and_loose_view_box_syntax() {
    let text = format!(
        "<?xml version=\"1.0\"?>\n<!-- an icon -->\n{}",
        WHITE_FILL.replace("0 0 256 256", "0,0, 256  256")
    );
    let fx = Fixture::new(&[("regular/cpu.svg", &text)]);
    assert!(fx.build().render().is_ok());
}

#[test]
fn view_box_check_can_be_disabled() {
    let fx = Fixture::new(&[(
        "regular/cpu.svg",
        &WHITE_FILL.replace("0 0 256 256", "0 0 24 24"),
    )]);
    let build = IconPackBuild {
        view_box: None,
        ..fx.build()
    };
    assert!(build.render().is_ok());
}

#[test]
fn duo_layers_pair_with_front_layers() {
    let fx = Fixture::new(&[("regular/cpu.svg", WHITE_FILL), ("duo/cpu.svg", WHITE_FILL)]);
    let (source, summary) = fx.build().render().expect("render");
    assert_eq!(summary.with_duo, 1);
    assert!(source.contains(
        "Some(include_bytes!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/icons/duo/cpu.svg\")))"
    ));

    let fx = Fixture::new(&[("regular/cpu.svg", WHITE_FILL), ("duo/gpu.svg", WHITE_FILL)]);
    let err = error(&fx);
    assert!(
        err.contains("duo/gpu.svg") && err.contains("no front layer"),
        "{err}"
    );
}

#[test]
fn duo_layers_are_validated_like_front_layers() {
    let fx = Fixture::new(&[
        ("regular/cpu.svg", WHITE_FILL),
        (
            "duo/cpu.svg",
            &WHITE_FILL.replace("#ffffff", "currentColor"),
        ),
    ]);
    let err = error(&fx);
    assert!(
        err.contains("duo/cpu.svg") && err.contains("currentColor"),
        "{err}"
    );
}

#[test]
fn empty_or_missing_folders_are_errors() {
    let fx = Fixture::new(&[("regular/readme.txt", "not an icon")]);
    assert!(error(&fx).contains("no .svg files"));
    let fx = Fixture::new(&[]);
    assert!(fx.build().render().is_err());
}

#[test]
fn ignores_non_svg_files_and_subfolders() {
    let fx = Fixture::new(&[
        ("regular/cpu.svg", WHITE_FILL),
        ("regular/notes.md", "# notes"),
        ("regular/nested/inner.svg", WHITE_FILL),
    ]);
    let (_, summary) = fx.build().render().expect("render");
    assert_eq!(summary.icons, 1);
}

#[test]
fn empty_glyph_mode_marks_everything_custom() {
    let fx = Fixture::new(&[
        ("regular/anything.svg", WHITE_FILL),
        ("regular/other.svg", &custom_svg("star")),
    ]);
    let build = IconPackBuild {
        glyphs: GlyphSource::Empty,
        ..fx.build()
    };
    let (source, summary) = build.render().expect("render");
    assert_eq!(summary.custom, 2);
    assert!(source.contains("\"\",\n    true,"));
    assert!(!source.contains("phosphor"));
}

#[test]
fn runtime_and_glyph_paths_are_configurable() {
    let fx = Fixture::new(&[("regular/cpu.svg", WHITE_FILL)]);
    let build = IconPackBuild {
        runtime_path: "crate".into(),
        ..fx.build()
    };
    let source = build.render().expect("render").0;
    assert!(source.contains("pub const CPU: crate::Icon = crate::Icon::new("));
    assert!(source.contains("crate::phosphor::regular::CPU,"));

    let build = IconPackBuild {
        glyph_path: Some("my_font::glyphs".into()),
        ..fx.build()
    };
    assert!(
        build
            .render()
            .expect("render")
            .0
            .contains("my_font::glyphs::CPU,")
    );
}

#[test]
fn uri_prefix_is_normalized_and_validated() {
    let fx = Fixture::new(&[("regular/cpu.svg", WHITE_FILL)]);
    let build = IconPackBuild {
        uri_prefix: "bytes://demo/icons/".into(),
        ..fx.build()
    };
    assert!(
        build
            .render()
            .expect("render")
            .0
            .contains("\"bytes://demo/icons/cpu.svg\"")
    );
    for bad in ["", "bytes://", "has space", "quo\"te"] {
        let build = IconPackBuild {
            uri_prefix: bad.into(),
            ..fx.build()
        };
        assert!(
            matches!(build.render(), Err(Error::InvalidUriPrefix { .. })),
            "{bad:?}"
        );
    }
}

#[test]
fn generate_writes_once_and_skips_identical_output() {
    let fx = Fixture::new(&[("regular/cpu.svg", WHITE_FILL)]);
    let build = fx.build();
    let first = build.generate().expect("first run");
    assert!(first.written);
    let on_disk = fs::read_to_string(&build.out_file).expect("output");
    assert_eq!(on_disk, build.render().expect("render").0);
    let second = generate(&build).expect("second run");
    assert!(!second.written);

    fs::write(fx.dir.path().join("icons/regular/gpu.svg"), WHITE_FILL).expect("write");
    assert!(build.generate().expect("third run").written);
}

#[test]
fn cargo_directives_watch_the_script_and_the_icon_folder() {
    let fx = Fixture::new(&[("regular/cpu.svg", WHITE_FILL)]);
    let lines = fx.build().cargo_directives();
    assert_eq!(lines[0], "cargo:rerun-if-changed=build.rs");
    assert_eq!(
        lines[1],
        format!(
            "cargo:rerun-if-changed={}",
            fx.dir.path().join("icons").display()
        )
    );
}

#[test]
fn errors_display_the_cause() {
    let fx = Fixture::new(&[]);
    let err = fx.build().generate().expect_err("missing folder");
    let shown = format!("{err:?}");
    assert!(shown.contains("regular"), "{shown}");
    assert!(std::error::Error::source(&err).is_some());
}

#[test]
fn glyph_attribute_is_read_from_the_root_element_only() {
    let text = WHITE_FILL.replace("<path", "<path data-glyph=\"GAUGE\"");
    let fx = Fixture::new(&[("regular/cpu.svg", &text)]);
    let (_, summary) = fx.build().render().expect("render");
    assert_eq!(summary.custom, 0);
}

mod phosphor_import {
    use super::*;
    use crate::phosphor::{duo_back_layer, import, white_front};

    const DUOTONE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" fill="currentColor"><path d="M200,56V200H56V56Z" opacity="0.2"/><path d="M200,48H56v160h144Z"/></svg>"#;

    #[test]
    fn white_front_replaces_current_color() {
        assert_eq!(white_front(DUOTONE).matches("#ffffff").count(), 1);
        assert!(!white_front(DUOTONE).contains("currentColor"));
    }

    #[test]
    fn duo_layer_keeps_only_the_faded_shapes_without_opacity() {
        let layer = duo_back_layer(DUOTONE).expect("layer");
        assert_eq!(
            layer,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" fill="#ffffff"><path d="M200,56V200H56V56Z"/></svg>"##
        );
    }

    #[test]
    fn duo_layer_is_none_without_faded_shapes_or_with_nesting() {
        assert_eq!(duo_back_layer(&white_front(WHITE_FILL)), None);
        let nested = DUOTONE
            .replace("<path d=\"M200,48", "<g><path d=\"M200,48")
            .replace("</svg>", "</g></svg>");
        assert_eq!(duo_back_layer(&nested), None);
    }

    #[test]
    fn import_writes_both_layers() {
        let core = tempfile::tempdir().expect("tempdir");
        let out = tempfile::tempdir().expect("tempdir");
        for sub in ["regular", "duotone"] {
            fs::create_dir_all(core.path().join(sub)).expect("mkdir");
        }
        fs::write(
            core.path().join("regular/cpu.svg"),
            DUOTONE.replace(" opacity=\"0.2\"", ""),
        )
        .expect("write");
        fs::write(core.path().join("duotone/cpu-duotone.svg"), DUOTONE).expect("write");
        fs::write(
            core.path().join("regular/dot.svg"),
            WHITE_FILL.replace("#ffffff", "currentColor"),
        )
        .expect("write");

        assert!(import(core.path(), out.path(), "cpu").expect("cpu").duo);
        assert!(!import(core.path(), out.path(), "dot").expect("dot").duo);
        assert!(import(core.path(), out.path(), "missing").is_err());

        assert!(out.path().join("duo/cpu.svg").is_file());
        assert!(!out.path().join("duo/dot.svg").exists());
        let regular = fs::read_to_string(out.path().join("regular/dot.svg")).expect("dot");
        assert!(regular.contains("#ffffff") && !regular.contains("currentColor"));

        let build = IconPackBuild {
            manifest_dir: None,
            emit_cargo_directives: false,
            ..IconPackBuild::new(out.path(), out.path().join("table.rs"))
        };
        assert_eq!(build.render().expect("generated from import").1.icons, 2);
    }

    #[test]
    fn shipped_assets_round_trip() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../egui-icon-pack/assets/phosphor");
        let (regular, duo) = (dir.join("regular/cpu.svg"), dir.join("duo/cpu.svg"));
        let (Ok(regular), Ok(duo)) = (fs::read_to_string(regular), fs::read_to_string(duo)) else {
            return;
        };
        let elements = |svg: &str| {
            let start = svg.find("><").map_or(0, |i| i + 1);
            svg[start..svg.rfind("</svg>").expect("end")].to_owned()
        };
        let header = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 256 256\" fill=\"currentColor\">";
        let faded = elements(&duo).replace("/>", " opacity=\"0.2\"/>");
        let duotone = format!("{header}{faded}{}</svg>", elements(&regular));
        assert_eq!(duo_back_layer(&duotone).as_deref(), Some(duo.as_str()));
        assert_eq!(
            white_front(&format!("{header}{}</svg>", elements(&regular))),
            regular
        );
    }
}

mod golden {
    use super::*;

    #[test]
    fn custom_pack_example_matches_checked_in_table() {
        let runtime = Path::new(env!("CARGO_MANIFEST_DIR")).join("../egui-icon-pack");
        let assets = runtime.join("examples/custom-pack/assets/icons");
        let golden = runtime.join("testdata/custom_pack.rs");
        if !assets.is_dir() {
            return;
        }
        let build = IconPackBuild {
            uri_prefix: "custom-pack-example/icons".into(),
            runtime_path: "crate".into(),
            custom_prefix: Some("demo-".into()),
            manifest_dir: Some(runtime),
            emit_cargo_directives: false,
            ..IconPackBuild::new(assets, golden.clone())
        };
        let (source, _) = build.render().expect("render");
        if std::env::var_os("UPDATE_GOLDEN").is_some() {
            fs::write(&golden, &source).expect("write golden");
        }
        let expected = fs::read_to_string(&golden).expect("golden table");
        assert_eq!(
            source, expected,
            "generator output changed; rerun with UPDATE_GOLDEN=1"
        );
    }
}
