//! Source checks over the chat module.

use std::path::{Path, PathBuf};

fn chat_sources() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("chat");
    let mut stack = vec![root];
    let mut files = Vec::new();
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                files.push(path);
            }
        }
    }
    files
}

/// Source lines outside `#[cfg(test)]` modules, which start at the first such attribute.
fn non_test_lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.lines()
        .enumerate()
        .take_while(|(_, l)| l.trim() != "#[cfg(test)]")
        .map(|(i, l)| (i + 1, l))
}

#[test]
fn chat_code_has_no_colour_literals() {
    let constructor = regex::Regex::new(r"Color32::(from_\w+|[A-Z][A-Z_]*)\b|Rgba::|Hsva").unwrap();
    let allowed = ["Color32::TRANSPARENT", "Color32::PLACEHOLDER"];
    let mut hits = Vec::new();
    for path in chat_sources() {
        if path.file_name().is_some_and(|n| n == "tests.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        for (line, src) in non_test_lines(&text) {
            for m in constructor.find_iter(src) {
                if !allowed.contains(&m.as_str()) {
                    hits.push(format!("{}:{line}: {}", path.display(), m.as_str()));
                }
            }
        }
    }
    assert!(
        hits.is_empty(),
        "colour literals in chat code:\n{}",
        hits.join("\n")
    );
}

#[test]
fn chat_strings_have_no_symbol_glyphs() {
    let mut hits = Vec::new();
    for path in chat_sources() {
        if path.file_name().is_some_and(|n| n == "tests.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        for (line, src) in non_test_lines(&text) {
            if src.trim_start().starts_with("//") {
                continue;
            }
            for c in src.chars() {
                let symbol = matches!(c as u32, 0x2190..=0x2BFF | 0x1F000..=0x1FAFF);
                if symbol {
                    hits.push(format!("{}:{line}: U+{:04X}", path.display(), c as u32));
                }
            }
        }
    }
    assert!(
        hits.is_empty(),
        "symbol glyphs in chat strings:\n{}",
        hits.join("\n")
    );
}
