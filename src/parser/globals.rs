use anyhow::Result;
use std::path::Path;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Node, Query, QueryCursor};

use crate::graph::{CallGraph, GlobalVar};

const GLOBAL_DECL_QUERY: &str = r#"
(translation_unit (declaration) @decl)
"#;

pub fn parse_globals(
    path: &Path,
    source: &str,
    language: &Language,
    root: Node,
    graph: &mut CallGraph,
) -> Result<()> {
    let src = source.as_bytes();
    let q = Query::new(language, GLOBAL_DECL_QUERY)?;
    let decl_idx = q.capture_index_for_name("decl").unwrap();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&q, root, src);

    while let Some(m) = matches.next() {
        let decl_node = match m.captures.iter().find(|c| c.index == decl_idx) {
            Some(c) => c.node,
            None => continue,
        };

        // Skip function prototypes: any child is a function_declarator
        let has_fn_declarator = decl_node.children(&mut decl_node.walk()).any(|child| {
            child.kind() == "function_declarator"
                || (child.kind() == "pointer_declarator"
                    && child
                        .children(&mut child.walk())
                        .any(|gc| gc.kind() == "function_declarator"))
        });
        if has_fn_declarator {
            continue;
        }

        let is_static = decl_node.children(&mut decl_node.walk()).any(|child| {
            child.kind() == "storage_class_specifier" && child.utf8_text(src).ok() == Some("static")
        });

        if let Some(var_name) = extract_var_name(decl_node, src) {
            let decl_text = decl_node.utf8_text(src).unwrap_or("").trim().to_string();
            graph.insert_global(GlobalVar {
                name: var_name,
                decl: decl_text,
                is_static,
                file: path.to_path_buf(),
                line: decl_node.start_position().row as u32 + 1,
            });
        }
    }

    Ok(())
}

/// Extract variable name from a declaration node by finding the deepest
/// `identifier` that is a declarator (not inside a type specifier).
fn extract_var_name(decl: Node, src: &[u8]) -> Option<String> {
    for child in decl.children(&mut decl.walk()) {
        match child.kind() {
            "identifier" => return child.utf8_text(src).ok().map(|s| s.to_string()),
            "init_declarator" | "pointer_declarator" | "array_declarator" => {
                if let Some(name) = extract_declarator_name(child, src) {
                    return Some(name);
                }
            }
            _ => {}
        }
    }
    None
}

fn extract_declarator_name(node: Node, src: &[u8]) -> Option<String> {
    for child in node.children(&mut node.walk()) {
        match child.kind() {
            "identifier" => return child.utf8_text(src).ok().map(|s| s.to_string()),
            "init_declarator" | "pointer_declarator" | "array_declarator" => {
                if let Some(name) = extract_declarator_name(child, src) {
                    return Some(name);
                }
            }
            _ => {}
        }
    }
    None
}
