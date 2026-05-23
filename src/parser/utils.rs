pub(super) fn cap_text<'a>(
    m: &tree_sitter::QueryMatch<'_, 'a>,
    idx: u32,
    src: &'a [u8],
) -> Option<String> {
    m.captures
        .iter()
        .find(|c| c.index == idx)
        .and_then(|c| c.node.utf8_text(src).ok())
        .map(|s| s.to_string())
}

pub(super) fn cap_text_opt<'a>(
    m: &tree_sitter::QueryMatch<'_, 'a>,
    idx: u32,
    src: &'a [u8],
) -> Option<String> {
    cap_text(m, idx, src)
}

pub(super) fn trim_value(s: String) -> String {
    let trimmed = s.trim();
    if trimmed.len() > 200 {
        format!("{}…", &trimmed[..200])
    } else {
        trimmed.to_string()
    }
}
