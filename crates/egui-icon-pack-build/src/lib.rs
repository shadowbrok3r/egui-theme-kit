//! Build-script generator for `egui-icon-pack`.
//!
//! Reads a folder of SVG files and writes a Rust source file with one typed
//! `Icon` constant per SVG, a sorted `ALL` slice, a `BORROWED` name list and a
//! `PACK` constant. Include the file from a module of your crate.
//!
//! # Folder layout
//!
//! ```text
//! assets/icons/
//!   regular/cpu.svg          front layer, one file per icon
//!   regular/acme-logo.svg    custom icon, declares data-glyph
//!   duo/cpu.svg              optional back layer of the duotone look
//! ```
//!
//! # SVG conventions
//!
//! - Root `<svg>` with `viewBox="0 0 256 256"`; all coordinates on that grid.
//! - White ink (`#ffffff`) so egui can tint it at paint time; `currentColor` is rejected.
//! - Custom icons use a 16-unit stroke with round caps and joins to match Phosphor Regular.
//! - Custom icons declare `data-glyph="NAME"` on the root element, naming the
//!   Phosphor font constant used when the icon is shown as inline text.
//! - A duo layer holds only the soft back shapes; it is painted under the front layer.
//!
//! # Usage
//!
//! ```no_run
//! // build.rs
//! use egui_icon_pack_build::IconPackBuild;
//!
//! fn main() -> Result<(), egui_icon_pack_build::Error> {
//!     let build = IconPackBuild {
//!         uri_prefix: "acme-icons".into(),
//!         custom_prefix: Some("acme-".into()),
//!         ..IconPackBuild::in_out_dir("assets/icons", "icon_table.rs")
//!     };
//!     build.generate()?;
//!     Ok(())
//! }
//! ```
//!
//! ```ignore
//! // src/icons.rs
//! include!(concat!(env!("OUT_DIR"), "/icon_table.rs"));
//!
//! let cpu = icons::CPU;
//! let acme_logo = icons::ACME_LOGO;
//! ```

use std::fmt::{self, Write as _};
use std::path::{Path, PathBuf};
use std::{env, fs, io};

pub mod phosphor;
mod svg;

const RESERVED: &[&str] = &["ALL", "BORROWED", "PACK"];

/// Where the font glyph of each generated icon comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlyphSource {
    /// Icons without `data-glyph` use the Phosphor font constant of the same name;
    /// icons with it borrow the named constant and count as custom.
    Phosphor,
    /// No font glyphs: every glyph is empty and every icon counts as custom.
    Empty,
}

/// Configuration of one generated icon table.
#[derive(Clone, Debug)]
pub struct IconPackBuild {
    /// Folder holding `front_dir` and, optionally, `duo_dir`.
    pub assets_dir: PathBuf,
    /// Rust file to write.
    pub out_file: PathBuf,
    /// Sub-folder of front layers.
    pub front_dir: String,
    /// Sub-folder of duo back layers.
    pub duo_dir: String,
    /// Prefix of the `bytes://` image URIs; must be unique among all packs in the app.
    pub uri_prefix: String,
    /// Path of the runtime crate as seen from the including module (`crate` inside it).
    pub runtime_path: String,
    /// Module holding the Phosphor font constants; defaults to `<runtime_path>::phosphor::regular`.
    pub glyph_path: Option<String>,
    /// Source of the font glyphs.
    pub glyphs: GlyphSource,
    /// File-name prefix whose icons must declare `data-glyph`.
    pub custom_prefix: Option<String>,
    /// Required root `viewBox`; `None` skips the check.
    pub view_box: Option<String>,
    /// Package manifest folder used to emit portable `include_bytes!` paths.
    pub manifest_dir: Option<PathBuf>,
    /// Print `cargo:rerun-if-changed` lines for the build script and the icon folder.
    pub emit_cargo_directives: bool,
}

/// What a generation run found and wrote.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Generated {
    /// Number of icons.
    pub icons: usize,
    /// Number of icons that declare `data-glyph`.
    pub custom: usize,
    /// Number of icons with a duo layer.
    pub with_duo: usize,
    /// False when the output file already had identical contents.
    pub written: bool,
}

/// Reason a generation run failed.
pub enum Error {
    /// Reading or writing a file failed.
    Io { path: PathBuf, source: io::Error },
    /// The front folder holds no SVG files.
    NoIcons { dir: PathBuf },
    /// File name is not lowercase kebab-case.
    InvalidName { file: String },
    /// File name maps to a name the generated table already uses.
    ReservedName { file: String, ident: String },
    /// A custom icon has no `data-glyph` attribute.
    MissingGlyph { file: String },
    /// `data-glyph` is not an upper-case constant name.
    InvalidGlyph { file: String, glyph: String },
    /// SVG content breaks a convention.
    InvalidSvg { file: String, reason: String },
    /// Duo layer without a front layer of the same name.
    OrphanDuo { file: String },
    /// `uri_prefix` is empty or has characters unusable in a URI.
    InvalidUriPrefix { prefix: String },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::NoIcons { dir } => write!(f, "no .svg files in {}", dir.display()),
            Self::InvalidName { file } => write!(
                f,
                "`{file}`: file name must be lowercase kebab-case (a-z, 0-9, single hyphens, starting with a letter)"
            ),
            Self::ReservedName { file, ident } => write!(
                f,
                "`{file}`: generates the constant `{ident}`, which the table already uses; rename the file"
            ),
            Self::MissingGlyph { file } => write!(
                f,
                "`{file}`: custom icons need a data-glyph attribute naming a Phosphor glyph, e.g. data-glyph=\"STAR\""
            ),
            Self::InvalidGlyph { file, glyph } => write!(
                f,
                "`{file}`: data-glyph=\"{glyph}\" must be an upper-case constant name such as STAR or ARROW_LEFT"
            ),
            Self::InvalidSvg { file, reason } => write!(f, "`{file}`: {reason}"),
            Self::OrphanDuo { file } => {
                write!(f, "`{file}`: duo layer has no front layer of the same name")
            }
            Self::InvalidUriPrefix { prefix } => write!(
                f,
                "uri_prefix `{prefix}` must be non-empty and use only letters, digits, `-`, `_`, `.` and `/`"
            ),
        }
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl IconPackBuild {
    /// Configuration with default conventions for the given folders.
    pub fn new(assets_dir: impl Into<PathBuf>, out_file: impl Into<PathBuf>) -> Self {
        Self {
            assets_dir: assets_dir.into(),
            out_file: out_file.into(),
            front_dir: "regular".into(),
            duo_dir: "duo".into(),
            uri_prefix: env::var("CARGO_PKG_NAME")
                .map_or_else(|_| "icons".into(), |n| format!("{n}-icons")),
            runtime_path: "egui_icon_pack".into(),
            glyph_path: None,
            glyphs: GlyphSource::Phosphor,
            custom_prefix: None,
            view_box: Some("0 0 256 256".into()),
            manifest_dir: env::var_os("CARGO_MANIFEST_DIR").map(PathBuf::from),
            emit_cargo_directives: true,
        }
    }

    /// Configuration for a build script: `assets_dir` is relative to the package, the output goes to `OUT_DIR`.
    ///
    /// # Panics
    ///
    /// Panics when `OUT_DIR` is unset, i.e. outside a build script.
    pub fn in_out_dir(assets_dir: impl Into<PathBuf>, file_name: impl AsRef<Path>) -> Self {
        let out_dir = env::var_os("OUT_DIR").expect("OUT_DIR is only set inside build scripts");
        Self::new(assets_dir, Path::new(&out_dir).join(file_name))
    }

    /// Module path holding the Phosphor font constants.
    fn glyph_module(&self) -> String {
        self.glyph_path
            .clone()
            .unwrap_or_else(|| format!("{}::phosphor::regular", self.runtime_path))
    }

    /// Lines telling cargo to rerun the build script when the script or any icon file changes.
    pub fn cargo_directives(&self) -> Vec<String> {
        vec![
            "cargo:rerun-if-changed=build.rs".to_owned(),
            format!("cargo:rerun-if-changed={}", self.assets_dir.display()),
        ]
    }

    /// Generates the table and writes it to `out_file` unless it is already up to date.
    pub fn generate(&self) -> Result<Generated, Error> {
        if self.emit_cargo_directives {
            for line in self.cargo_directives() {
                println!("{line}");
            }
        }
        let (source, mut summary) = self.render()?;
        let unchanged = fs::read_to_string(&self.out_file).is_ok_and(|old| old == source);
        summary.written = !unchanged;
        if !unchanged {
            if let Some(parent) = self.out_file.parent() {
                fs::create_dir_all(parent).map_err(|source| Error::Io {
                    path: parent.to_owned(),
                    source,
                })?;
            }
            fs::write(&self.out_file, source).map_err(|source| Error::Io {
                path: self.out_file.clone(),
                source,
            })?;
        }
        Ok(summary)
    }

    /// Validates the folders and returns the generated source without writing it.
    pub fn render(&self) -> Result<(String, Generated), Error> {
        let prefix = self.normalized_uri_prefix()?;
        let front_path = self.assets_dir.join(&self.front_dir);
        let names = svg_stems(&front_path)?;
        if names.is_empty() {
            return Err(Error::NoIcons { dir: front_path });
        }
        let duo_path = self.assets_dir.join(&self.duo_dir);
        let duo_names = if duo_path.is_dir() {
            svg_stems(&duo_path)?
        } else {
            Vec::new()
        };
        for name in &duo_names {
            if names.binary_search(name).is_err() {
                return Err(Error::OrphanDuo {
                    file: format!("{}/{name}.svg", self.duo_dir),
                });
            }
            let file = format!("{}/{name}.svg", self.duo_dir);
            self.check_svg(&file, &read(&duo_path.join(format!("{name}.svg")))?)?;
        }

        let glyph_module = self.glyph_module();
        let rt = &self.runtime_path;
        let mut out = String::from("// @generated by egui-icon-pack-build; do not edit.\n");
        let mut all =
            format!("/// Every icon, sorted by name.\npub const ALL: &[{rt}::Icon] = &[\n");
        let mut borrowed = String::from(
            "/// Names of icons whose font glyph is borrowed or missing.\npub const BORROWED: &[&str] = &[\n",
        );
        let mut summary = Generated {
            icons: names.len(),
            custom: 0,
            with_duo: 0,
            written: true,
        };

        for name in &names {
            let file = format!("{}/{name}.svg", self.front_dir);
            if !is_kebab_case(name) {
                return Err(Error::InvalidName { file });
            }
            let ident = name.replace('-', "_").to_ascii_uppercase();
            if RESERVED.contains(&ident.as_str()) {
                return Err(Error::ReservedName { file, ident });
            }
            let text = read(&front_path.join(format!("{name}.svg")))?;
            self.check_svg(&file, &text)?;

            let declared = svg::root_tag(&text).and_then(|tag| svg::attr(tag, "data-glyph"));
            let (glyph_expr, custom) = match self.glyphs {
                GlyphSource::Empty => ("\"\"".to_owned(), true),
                GlyphSource::Phosphor => match declared {
                    Some(glyph) => {
                        if !is_glyph_ident(glyph) {
                            return Err(Error::InvalidGlyph {
                                file,
                                glyph: glyph.to_owned(),
                            });
                        }
                        (format!("{glyph_module}::{glyph}"), true)
                    }
                    None if self
                        .custom_prefix
                        .as_deref()
                        .is_some_and(|p| name.starts_with(p)) =>
                    {
                        return Err(Error::MissingGlyph { file });
                    }
                    None => (format!("{glyph_module}::{ident}"), false),
                },
            };
            let has_duo = duo_names.binary_search(name).is_ok();
            summary.custom += usize::from(custom);
            summary.with_duo += usize::from(has_duo);
            let duo_expr = if has_duo {
                format!("Some({})", self.include_bytes(&duo_path, name)?)
            } else {
                "None".to_owned()
            };
            let front_expr = self.include_bytes(&front_path, name)?;
            let _ = writeln!(
                out,
                "/// Icon `{name}`.\npub const {ident}: {rt}::Icon = {rt}::Icon::new(\n    \"{name}\",\n    \"bytes://{prefix}/{name}.svg\",\n    \"bytes://{prefix}/{duo}/{name}.svg\",\n    {front_expr},\n    {duo_expr},\n    {glyph_expr},\n    {custom},\n);",
                duo = self.duo_dir,
            );
            let _ = writeln!(all, "    {ident},");
            if custom {
                let _ = writeln!(borrowed, "    \"{name}\",");
            }
        }
        all.push_str("];\n");
        borrowed.push_str("];\n");
        out.push_str(&all);
        out.push_str(&borrowed);
        let _ = writeln!(
            out,
            "/// [`ALL`] as an icon pack.\npub const PACK: {rt}::Pack = {rt}::Pack::new(ALL);"
        );
        Ok((out, summary))
    }

    fn normalized_uri_prefix(&self) -> Result<String, Error> {
        let prefix = self
            .uri_prefix
            .trim_start_matches("bytes://")
            .trim_matches('/');
        let valid = !prefix.is_empty()
            && prefix
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'));
        if valid {
            Ok(prefix.to_owned())
        } else {
            Err(Error::InvalidUriPrefix {
                prefix: self.uri_prefix.clone(),
            })
        }
    }

    fn check_svg(&self, file: &str, text: &str) -> Result<(), Error> {
        let invalid = |reason: &str| Error::InvalidSvg {
            file: file.to_owned(),
            reason: reason.to_owned(),
        };
        let tag = svg::root_tag(text).ok_or_else(|| invalid("not an <svg> document"))?;
        if text.to_ascii_lowercase().contains("currentcolor") {
            return Err(invalid(
                "uses currentColor, which cannot be tinted; use fill/stroke=\"#ffffff\"",
            ));
        }
        let view_box =
            svg::attr(tag, "viewBox").ok_or_else(|| invalid("root <svg> has no viewBox"))?;
        if let Some(expected) = &self.view_box
            && svg::normalize_view_box(view_box) != svg::normalize_view_box(expected)
        {
            return Err(invalid(&format!(
                "viewBox=\"{view_box}\" must be \"{expected}\""
            )));
        }
        Ok(())
    }

    /// `include_bytes!` expression for `<dir>/<name>.svg`.
    fn include_bytes(&self, dir: &Path, name: &str) -> Result<String, Error> {
        let file = dir.join(format!("{name}.svg"));
        let absolute = std::path::absolute(&file).map_err(|source| Error::Io {
            path: file.clone(),
            source,
        })?;
        let manifest = self
            .manifest_dir
            .as_ref()
            .and_then(|m| std::path::absolute(m).ok());
        if let Some(relative) = manifest
            .as_deref()
            .and_then(|m| absolute.strip_prefix(m).ok())
        {
            let relative = relative.to_string_lossy().replace('\\', "/");
            Ok(format!(
                "include_bytes!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/{relative}\"))"
            ))
        } else {
            Ok(format!(
                "include_bytes!({:?})",
                absolute.to_string_lossy().replace('\\', "/")
            ))
        }
    }
}

/// Runs a generation with `build`; shorthand for [`IconPackBuild::generate`].
pub fn generate(build: &IconPackBuild) -> Result<Generated, Error> {
    build.generate()
}

fn read(path: &Path) -> Result<String, Error> {
    fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.to_owned(),
        source,
    })
}

/// Sorted file stems of the `.svg` files directly inside `dir`.
fn svg_stems(dir: &Path) -> Result<Vec<String>, Error> {
    let io_error = |source| Error::Io {
        path: dir.to_owned(),
        source,
    };
    let mut stems = Vec::new();
    for entry in fs::read_dir(dir).map_err(io_error)? {
        let path = entry.map_err(io_error)?.path();
        if path.is_file() && path.extension().is_some_and(|e| e == "svg") {
            let stem =
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| Error::InvalidName {
                        file: path.display().to_string(),
                    })?;
            stems.push(stem.to_owned());
        }
    }
    stems.sort();
    Ok(stems)
}

fn is_kebab_case(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.ends_with('-')
        && !name.contains("--")
}

fn is_glyph_ident(glyph: &str) -> bool {
    glyph.starts_with(|c: char| c.is_ascii_uppercase())
        && glyph
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

#[cfg(test)]
mod tests;
