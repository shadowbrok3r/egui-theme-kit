# Third-party notices

## Authorship

The styling system in this repository is derived from the author's own application styling code, extracted into a standalone library.

## Bundled assets

| Asset | Where | License |
|---|---|---|
| Inter (Regular, Medium, SemiBold) by The Inter Project Authors, <https://github.com/rsms/inter> | `crates/egui-theme-kit/assets/fonts/Inter-*.ttf` | SIL Open Font License 1.1, full text in `crates/egui-theme-kit/assets/fonts/OFL-Inter.txt` |

The Inter files are embedded unmodified in the source tree. At runtime the kit rewrites their in-memory cmap tables to drop Private Use Area mappings; the font files on disk are not changed.

| Phosphor Icons (Regular and duotone back layers), @phosphor-icons/core 2.1.1, <https://github.com/phosphor-icons/core> | `crates/egui-icon-pack/assets/phosphor/` | MIT, see `crates/egui-icon-pack/assets/phosphor/LICENSE` and `ATTRIBUTION` |

Modifications to the Phosphor files: `fill="currentColor"` is replaced with `#ffffff` so the icons tint at paint time, and each duotone file keeps only the back layer with its `opacity` attribute removed.

## Derived code and palettes

- `crates/egui-theme-kit/src/logger.rs` is adapted from [egui_logger](https://github.com/RegenJacob/egui_logger) (MIT), restyled with the kit's tokens.
- The Tokyo Night palettes follow the Tokyo Night editor theme (MIT) as ported by the egui-aesthetix crate (MIT).
- The Rerun and Rerun OLED presets are the author's own palettes, inspired by the Rerun viewer (MIT OR Apache-2.0).

## Vendored crates

`crates/backdrop-blur-core`, `crates/backdrop-blur-glow` and `crates/backdrop-blur-egui` are a subset of [abdu-benayad/backdrop-blur](https://github.com/abdu-benayad/backdrop-blur), limited to the eframe-on-glow grab-pass path. They are licensed MIT OR Apache-2.0 by their author; each crate directory carries its own `LICENSE-MIT` and `LICENSE-APACHE`, and the upstream README is kept in `docs/backdrop-blur-UPSTREAM-README.md`.

Differences from upstream:

- `backdrop-blur-wgpu`, the `own-loop` adapter path and the wgpu dependencies are removed.
- The `gl-snapshots` GPU test tier (EGL harness, `khronos-egl`, `libloading`) is removed.
- Manifests carry explicit version pins instead of workspace inheritance, and `egui` is built with `default-features = false`.

Library sources are otherwise unchanged.

## Dependencies

Dependencies are fetched by Cargo and are not redistributed in this repository. The notable ones:

| Crate | License |
|---|---|
| `egui`, `eframe`, `egui_extras` (emilk/egui) | MIT OR Apache-2.0 |
| `egui_dock`, `egui-data-table`, `egui-toast` (forks on the `egui-037` branch) | MIT |
| `rfd` | MIT |
| `directories` | MIT OR Apache-2.0 |
| `egui-phosphor` | MIT OR Apache-2.0; used by tests and examples only, not part of the library |
