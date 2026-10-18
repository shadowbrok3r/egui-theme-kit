//! Minimal SVG text helpers: root tag and attribute lookup.

/// The opening `<svg ...>` tag when only an XML declaration, comments or a doctype precede it.
pub(crate) fn root_tag(svg: &str) -> Option<&str> {
    let start = svg.find("<svg")?;
    let preceded_by_element = svg[..start]
        .match_indices('<')
        .any(|(i, _)| !matches!(svg.as_bytes().get(i + 1), Some(b'?' | b'!')));
    if preceded_by_element {
        return None;
    }
    let rest = &svg[start..];
    if !rest[4..].starts_with(|c: char| c.is_whitespace() || c == '>') {
        return None;
    }
    let end = rest.find('>')?;
    Some(&rest[..=end])
}

/// Value of attribute `name` inside `tag`.
pub(crate) fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("{name}=\"");
    let mut from = 0;
    while let Some(found) = tag[from..].find(&needle) {
        let at = from + found;
        let start = at + needle.len();
        if tag[..at].ends_with(char::is_whitespace) {
            let len = tag[start..].find('"')?;
            return Some(&tag[start..start + len]);
        }
        from = start;
    }
    None
}

/// `viewBox` value with separators collapsed to single spaces.
pub(crate) fn normalize_view_box(value: &str) -> String {
    value
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
