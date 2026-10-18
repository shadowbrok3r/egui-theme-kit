# egui-icon-pack

SVG icon packs for egui, with a build-time generator for your own icon set.

| Crate | Role |
| --- | --- |
| `egui-icon-pack` | `Icon`, `IconPack`, tinted and duotone painting, icon widgets, Phosphor font installer, gallery window, optional bundled Phosphor Regular set |
| `egui-icon-pack-build` | `build.rs` generator: a folder of SVG files becomes one typed `Icon` constant per file |

Every icon is a white SVG that egui tints at paint time, so one file serves every theme color. An
optional second layer gives a duotone look. Each icon also carries a Phosphor font glyph for use in
plain text labels.

## Quick start: bundled Phosphor Regular

```toml
[dependencies]
egui-icon-pack = { git = "https://github.com/shadowbrok3r/egui-theme-kit", features = ["phosphor-regular"] }
```

```rust
use egui_icon_pack::phosphor_regular as svg;
use egui_icon_pack::widgets;

// Once, at startup (fonts before the first frame, loaders on the context).
let mut fonts = egui::FontDefinitions::default();
egui_icon_pack::install_fonts(&mut fonts);
ctx.set_fonts(fonts);
egui_icon_pack::install(&ctx);

// Every frame.
svg::GEAR.show(ui, 16.0);                                  // text color
svg::TRASH.show_tinted(ui, 16.0, egui::Color32::LIGHT_RED);
if widgets::button(ui, svg::FOLDER_OPEN, "Open").clicked() { /* ... */ }
widgets::icon_button(ui, svg::EYE, "Preview");             // icon only, tooltip
widgets::toggle_button(ui, svg::PUSH_PIN, pinned, "Pin");
widgets::menu_button(ui, svg::LIST, "Menu", |ui| { /* ... */ });
ui.label(format!("{} Settings", svg::GEAR.glyph()));       // font glyph in text
```

The widgets only read `ui.visuals()` and the `Button` style, so they follow any egui theme.

The bundled set has 439 icons (`svg::ALL`, sorted by name), 438 with a duotone layer, about 300 KB
of SVG in total. Icons are only linked into the binary when used.

### Gallery

```rust
egui_icon_pack::gallery::show_window(ctx, &svg::PACK); // call every frame
egui_icon_pack::gallery::toggle(ctx);                  // bind to a debug menu entry
```

`gallery::show(ui, &pack)` draws the same contents into any `Ui`. It works for any `IconPack`:
search, size, tint, duotone, backdrops, widget samples, a detail pane and a stress mode. An app with
one pack registers it once with `gallery::set_pack(ctx, PACK)` and calls `gallery::show_pack_window(ctx)`
and `gallery::show_pack(ui)` without the argument.

```text
cargo run -p egui-icon-pack --example gallery --features phosphor-regular
```

## Your own icon pack

An app that already has its own icon table, widgets and gallery code finds the API differences, the
matching `IconPackBuild` options and a checklist in [MIGRATION.md](MIGRATION.md).

Folder layout:

```text
assets/icons/
  regular/cpu.svg          front layer, one file per icon (kebab-case name)
  regular/acme-logo.svg    custom icon: declares data-glyph
  duo/acme-logo.svg        optional back layer
```

`Cargo.toml`:

```toml
[dependencies]
egui-icon-pack = { git = "https://github.com/shadowbrok3r/egui-theme-kit" }

[build-dependencies]
egui-icon-pack-build = { git = "https://github.com/shadowbrok3r/egui-theme-kit" }
```

`build.rs`:

```rust
use egui_icon_pack_build::IconPackBuild;

fn main() -> Result<(), egui_icon_pack_build::Error> {
    IconPackBuild {
        uri_prefix: "acme-icons".into(),          // unique per pack in the app
        custom_prefix: Some("acme-".into()),      // acme-* files must declare data-glyph
        ..IconPackBuild::in_out_dir("assets/icons", "icon_table.rs")
    }
    .generate()?;
    Ok(())
}
```

`src/icons.rs`:

```rust
include!(concat!(env!("OUT_DIR"), "/icon_table.rs"));

// icons::CPU, icons::ACME_LOGO, icons::ALL, icons::BORROWED, icons::PACK
```

A complete, runnable template lives in `examples/custom-pack`
(`cargo run --manifest-path crates/egui-icon-pack/examples/custom-pack/Cargo.toml`).

The generator emits, for every SVG, `pub const NAME: Icon`, plus `ALL` (sorted by name),
`BORROWED` (names of icons that carry their own glyph) and `PACK` (`ALL` as a `Pack`). It writes the
file only when the contents changed, and fails the build with a message naming the file when:

- a file name is not lowercase kebab-case (`a-z`, `0-9`, single hyphens, starts with a letter)
- a name collides with `ALL`, `BORROWED` or `PACK`
- an icon uses `currentColor` (it cannot be tinted) or its root `viewBox` is not `0 0 256 256`
- a file with the `custom_prefix` has no valid `data-glyph`
- a `duo/` file has no matching `regular/` file

Generator options: `front_dir`, `duo_dir` (folder names), `glyphs` (`GlyphSource::Empty` for packs
without font glyphs), `glyph_path`, `runtime_path`, `view_box` (`None` disables the check) and
`emit_cargo_directives`.

### SVG conventions

| Rule | Value |
| --- | --- |
| Canvas | `viewBox="0 0 256 256"` |
| Ink | white: `stroke="#ffffff"` or `fill="#ffffff"`; never `currentColor` |
| Stroke icons | `fill="none" stroke-width="16" stroke-linecap="round" stroke-linejoin="round"` |
| Content | paths and basic shapes; no `<image>`, `<text>`, gradients or filters |
| Custom icons | `data-glyph="STAR"` on the root `<svg>`: the Phosphor font constant used for inline text |
| Duo layer | same canvas, only the soft back shapes, filled white; painted under the front layer |

Stroke icons on the 16-unit grid sit well next to Phosphor Regular.

### Adding a custom icon

1. Draw it on a 256 canvas with a 16-unit round stroke, white ink.
2. Save it as `assets/icons/regular/acme-thing.svg` and add `data-glyph="SPARKLE"` (any constant of
   `egui_icon_pack::phosphor::regular`) to the root element.
3. Optionally add `assets/icons/duo/acme-thing.svg` with the shapes that should be tinted softly.
4. Build. `icons::ACME_THING` exists; `Icon::is_custom()` is true because it borrows a glyph.

Icons without `data-glyph` must be named like a Phosphor icon (`hard-drive` for `HARD_DRIVE`): the
generator uses the font constant of the same name.

### Duo layers from Phosphor duotone SVGs

A Phosphor duotone SVG holds the soft shapes with `opacity="0.2"` and the solid shapes without.
The duo layer is the soft shapes alone, white and without the opacity attribute; the regular layer
is the Regular SVG with `currentColor` replaced by `#ffffff`. `egui-icon-pack-build` automates both:

```text
cargo run -p egui-icon-pack-build --example import_phosphor -- \
    <phosphor-core>/assets assets/icons cpu hard-drive gear
```

`<phosphor-core>` is an unpacked `@phosphor-icons/core` package. The command writes
`assets/icons/regular/<name>.svg` and `assets/icons/duo/<name>.svg`. The same conversion is
available as `egui_icon_pack_build::phosphor::{white_front, duo_back_layer, import}`.

Paint a duotone icon with `icon.paint_duo(ui, rect, size, fg, bg)`; `bg` is usually `fg` at 40 %
opacity (`fg.gamma_multiply(0.4)`).

### Several packs

`IconRegistry` merges packs into one searchable collection; earlier packs win name clashes.

```rust
let registry = IconRegistry::new().with(&icons::PACK).with(&svg::PACK);
registry.find("cpu");
registry.from_glyph(egui_icon_pack::phosphor::regular::CPU);
gallery::show(ui, &registry);
```

Image cache keys must be unique per pack: give every generated pack its own `uri_prefix`. The
bundled set uses `egui-icon-pack/phosphor-regular`.

## Fonts

`egui_icon_pack::install_fonts(&mut FontDefinitions)` registers the Phosphor font in the
Proportional, Monospace and named "Regular" / "Bold" families, behind the text font, and is
idempotent. It has the signature of a font installer hook, so it plugs into the font stack of
`egui-theme-kit`, which runs hooks before it installs Inter:

```rust
let fonts = FontConfig::new().installer(egui_icon_pack::install_fonts);
```

`egui_icon_pack::phosphor` re-exports `egui-phosphor`, and `icon`, `icon_sized` and `icon_colored`
build rich text from a glyph.

## Features

| Feature | Default | Effect |
| --- | --- | --- |
| `font` | yes | `install_fonts`, glyph helpers, the `phosphor` re-export and glyph fallbacks of generated packs |
| `phosphor-regular` | no | the bundled Phosphor Regular set in `phosphor_regular` (implies `font`) |

Without default features the crate has no font dependency; generate packs with
`glyphs: GlyphSource::Empty`.

## Licensing

The code is licensed `MIT OR Apache-2.0`. The bundled SVGs in `assets/phosphor` are derived from
[Phosphor Icons](https://github.com/phosphor-icons/core) (`@phosphor-icons/core` 2.1.1), MIT License,
Copyright (c) 2023 Phosphor Icons; the license text is in `assets/phosphor/LICENSE` and the
modifications are listed in `assets/phosphor/ATTRIBUTION`. Keep both files when redistributing the
`phosphor-regular` icons.
