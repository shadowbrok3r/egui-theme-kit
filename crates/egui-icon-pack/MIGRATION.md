# Moving an app-local icon module onto egui-icon-pack

For an app that carried its own `Icon` type, widgets, gallery and a `build.rs` table generator, and
now uses `egui-icon-pack` and `egui-icon-pack-build` instead. Every item below is a place where the
library differs from such a copy.

## Dependencies

```toml
[dependencies]
egui-icon-pack = { git = "https://github.com/shadowbrok3r/egui-theme-kit", rev = "<commit>" }
egui-phosphor = { git = "https://github.com/shadowbrok3r/egui-phosphor", branch = "egui-037", default-features = false, features = ["regular"] }

[build-dependencies]
egui-icon-pack-build = { git = "https://github.com/shadowbrok3r/egui-theme-kit", rev = "<commit>" }
```

- The generated table names `egui_icon_pack::Icon` and `egui_icon_pack::Pack` by path. The crate that
  includes the table must depend on `egui-icon-pack` under that name, or point the generator at a
  re-export with `runtime_path` (see below).
- `egui-icon-pack` has a default `font` feature that brings in the `egui-phosphor` fork above. An app
  that also depends on `egui-phosphor` should use the same git source and branch so one copy links.
  Turn the default off and generate with `GlyphSource::Empty` for a pack without font glyphs.

## build.rs

The library generator replaces a hand-written script. These options keep an existing URI scheme,
glyph source and custom-icon rule:

```rust
use egui_icon_pack_build::IconPackBuild;

fn main() -> Result<(), egui_icon_pack_build::Error> {
    IconPackBuild {
        // Image cache keys stay `bytes://acme-icons/<name>.svg` and `bytes://acme-icons/duo/<name>.svg`.
        uri_prefix: "acme-icons".into(),
        // Files named `acme-*` must declare `data-glyph`, as before.
        custom_prefix: Some("acme-".into()),
        // Phosphor constants come from the app's own dependency instead of `egui_icon_pack::phosphor`.
        glyph_path: Some("egui_phosphor::regular".into()),
        ..IconPackBuild::in_out_dir("assets/icons", "icon_table.rs")
    }
    .generate()?;
    Ok(())
}
```

| Option | Use it when |
|---|---|
| `uri_prefix` | the old keys were `bytes://<prefix>/<name>.svg`; the duo layer is `bytes://<prefix>/<duo_dir>/<name>.svg` |
| `duo_dir`, `front_dir` | the folders are not called `duo` and `regular` |
| `custom_prefix` | an existing file-name prefix marks icons that borrow another glyph |
| `glyph_path` | the constants come from a crate other than `egui_icon_pack::phosphor::regular` |
| `glyphs: GlyphSource::Empty` | the pack has no font glyphs; every icon counts as custom |
| `runtime_path` | the app cannot depend on `egui-icon-pack` by that name; set it to a re-export such as `crate::icons::runtime` (`pub use egui_icon_pack as runtime;`). It also prefixes the default glyph path |
| `view_box: None` | existing SVGs do not use a `0 0 256 256` canvas |

Differences from a hand-written generator:

- The generator validates the folder: kebab-case names, no `currentColor` (an icon must be white to be
  tinted), a `0 0 256 256` `viewBox` unless disabled, a `data-glyph` on `custom_prefix` files, a front
  file for every duo file. A file named `all`, `borrowed` or `pack` fails because its constant would
  collide with the generated `ALL`, `BORROWED` and `PACK`.
- It prints the `cargo:rerun-if-changed` lines itself (`emit_cargo_directives`); drop your own.
- It rewrites the output only when the contents change.
- `include_bytes!` paths are relative to `CARGO_MANIFEST_DIR`, so a checked-in table stays portable.

## The generated table

```rust
pub use egui_icon_pack::{Icon, IconPack, IconRegistry, Pack};

include!(concat!(env!("OUT_DIR"), "/icon_table.rs"));
```

| Item | Type |
|---|---|
| `pub const NAME` per icon | `egui_icon_pack::Icon` (built with the `const fn Icon::new`) |
| `ALL` | `&[egui_icon_pack::Icon]`, sorted by name |
| `BORROWED` | `&[&str]`, now `pub` (it was crate-private in typical local copies) |
| `PACK` | `egui_icon_pack::Pack`, new: `ALL` as an icon pack |

The re-export line keeps `icons::Icon` and `icons::IconPack` paths working.

## Icon

| App-local copy | Library |
|---|---|
| `Icon::find(name)` | `PACK.find(name)` through the `IconPack` trait (import it); also works on `IconRegistry` and slices |
| `Icon::from_glyph(glyph)` | `PACK.from_glyph(glyph)`; custom icons are skipped and the last of several icons sharing a glyph wins |
| struct literal with private fields | `Icon::new(name, uri, duo_uri, svg, duo, glyph, custom)`; `uri` and `duo_uri` must end in `.svg` |
| `is_custom()` derived from `BORROWED` | a stored flag set by the generator: true for `data-glyph` icons and under `GlyphSource::Empty` |
| `PartialEq` and `Hash` on the name | on the name and the uri |
| derived `Debug` | prints name, `has_duo` and `custom` only |

`const_name`, `name`, `glyph`, `svg`, `duo_svg`, `has_duo`, `source`, `duo_source`, `image`, `show`,
`show_tinted`, `paint` and `paint_duo` keep their names and signatures. The new `uri` and `duo_uri`
accessors return the image cache keys.

Image cache keys must be unique per pack, and one key must always carry the same bytes.
`IconRegistry::extend` asserts this in debug builds.

## Widgets, loaders and fonts

| App-local copy | Library |
|---|---|
| `icons::widgets::*` and `IconSize` | `egui_icon_pack::widgets::*` and `egui_icon_pack::IconSize`; names and signatures are unchanged |
| `icons::install(ctx)` | `egui_icon_pack::install(ctx)` |
| font installer and glyph helpers | `egui_icon_pack::{install_fonts, icon, icon_sized, icon_colored}` and the `egui_icon_pack::phosphor` re-export (feature `font`) |

`install_fonts` has the signature of a font installer hook, so it plugs into the font stack of
`egui-theme-kit`: `FontConfig::new().installer(egui_icon_pack::install_fonts)`.

## Gallery

| App-local copy | Library |
|---|---|
| `gallery::show_window(ctx)` | `gallery::show_window(ctx, &PACK)` |
| `gallery::show(ui)` | `gallery::show(ui, &PACK)` |
| `is_open`, `set_open`, `toggle` | unchanged |

The pack argument is a `&dyn IconPack`: a `Pack`, a slice, or an `IconRegistry` that merges several
packs. An app with a single pack registers it once and calls the argument-less functions:

```rust
egui_icon_pack::gallery::set_pack(&ctx, icons::PACK); // once, at startup
egui_icon_pack::gallery::show_pack_window(&ctx);      // every frame
egui_icon_pack::gallery::show_pack(ui);               // contents in any Ui
```

`show_pack_window` draws nothing and `show_pack` says that no pack is registered until `set_pack` ran.

## Pixel snapping

`snapped_square(center, size, pixels_per_point)` is now public at the crate root. `Icon::paint` places
every icon with it, so app code that paints its own images next to icons can use the same rounding.

## Checklist

1. Add the two dependencies and keep `egui-phosphor` on the same git source.
2. Replace the table script in `build.rs` with `IconPackBuild` and the options that match the old
   keys and prefix.
3. Replace the local `Icon`, widgets, loader and gallery code with the re-exports above.
4. Change `Icon::find` and `Icon::from_glyph` calls to `PACK.find` and `PACK.from_glyph`.
5. Pass the pack to `gallery::show` and `gallery::show_window`, or register it with `gallery::set_pack`.
6. Delete the local tests for code that moved (widgets, `snapped_square`, icon sizes); keep the ones
   that check your own SVG folder, such as one constant per file and wired duo layers.
