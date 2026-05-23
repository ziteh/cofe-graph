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

pub(super) fn extract_conditions(node: tree_sitter::Node<'_>, src: &[u8]) -> Vec<String> {
    let mut conditions = Vec::new();
    let mut curr = node.parent();
    while let Some(p) = curr {
        let kind = p.kind();
        if (kind.starts_with("preproc_if") || kind == "preproc_else" || kind == "preproc_elif")
            && let Ok(text) = p.utf8_text(src)
            && let Some(first_line) = text.lines().next()
        {
            conditions.push(first_line.trim().to_string());
        }
        curr = p.parent();
    }
    conditions.reverse();
    conditions
}
