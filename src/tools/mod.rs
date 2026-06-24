pub mod get_source;
pub mod index;
pub mod query_call_graph;
pub mod symbol_lookup;

/// Extract function signature
/// everything before the first `{`, skipping doc comments
pub(crate) fn extract_fn_signature(source: &str) -> String {
    let pos = skip_leading_comments(source);
    let body = &source[pos..];

    if let Some(brace) = body.find('{') {
        body[..brace].trim().to_string()
    } else {
        body.trim().to_string()
    }
}

pub(crate) fn skip_leading_comments(source: &str) -> usize {
    let mut pos = 0;
    let bytes = source.as_bytes();

    loop {
        while pos < bytes.len() && matches!(bytes[pos], b' ' | b'\t' | b'\n' | b'\r') {
            pos += 1;
        }

        if pos + 1 >= bytes.len() {
            break;
        }

        if bytes[pos] == b'/' && bytes[pos + 1] == b'/' {
            while pos < bytes.len() && bytes[pos] != b'\n' {
                pos += 1;
            }
        } else if bytes[pos] == b'/' && bytes[pos + 1] == b'*' {
            pos += 2;
            while pos + 1 < bytes.len() && !(bytes[pos] == b'*' && bytes[pos + 1] == b'/') {
                pos += 1;
            }
            pos += 2;
        } else {
            break;
        }
    }

    pos
}
