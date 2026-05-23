use anyhow::Result;
use std::path::Path;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Node, Query, QueryCursor};

use crate::graph::{CallGraph, FunctionNode};

const FUNCTION_DEF_QUERY: &str = r#"
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @fn.name)
) @fn.def
"#;

const CALL_EXPR_QUERY: &str = r#"
(call_expression
  function: (identifier) @callee)
"#;

pub fn parse_functions(
    path: &Path,
    source: &str,
    language: &Language,
    root: Node,
    graph: &mut CallGraph,
) -> Result<()> {
    let src = source.as_bytes();
    let fn_query = Query::new(language, FUNCTION_DEF_QUERY)?;
    let name_idx = fn_query.capture_index_for_name("fn.name").unwrap();
    let def_idx = fn_query.capture_index_for_name("fn.def").unwrap();
    let call_query = Query::new(language, CALL_EXPR_QUERY)?;
    let callee_idx = call_query.capture_index_for_name("callee").unwrap();

    // Pass 1: extract function definitions
    {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&fn_query, root, src);
        while let Some(m) = matches.next() {
            let name_node = m
                .captures
                .iter()
                .find(|c| c.index == name_idx)
                .map(|c| c.node);
            let def_node = m
                .captures
                .iter()
                .find(|c| c.index == def_idx)
                .map(|c| c.node);
            if let (Some(nn), Some(dn)) = (name_node, def_node) {
                let name = nn.utf8_text(src)?.to_string();
                let source_text = dn.utf8_text(src)?.to_string();
                let line = nn.start_position().row as u32 + 1;
                let is_static = dn.children(&mut dn.walk()).any(|child| {
                    child.kind() == "storage_class_specifier"
                        && child.utf8_text(src).ok() == Some("static")
                });
                graph.insert_node(FunctionNode {
                    name,
                    file: path.to_path_buf(),
                    line,
                    source: source_text,
                    is_static,
                });
            }
        }
    }

    // Pass 2: extract call edges per function body
    {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&fn_query, root, src);
        while let Some(m) = matches.next() {
            let name_node = m
                .captures
                .iter()
                .find(|c| c.index == name_idx)
                .map(|c| c.node);
            let def_node = m
                .captures
                .iter()
                .find(|c| c.index == def_idx)
                .map(|c| c.node);
            if let (Some(nn), Some(dn)) = (name_node, def_node) {
                let caller = nn.utf8_text(src)?.to_string();
                let mut call_cursor = QueryCursor::new();
                let mut calls = call_cursor.matches(&call_query, dn, src);
                while let Some(cm) = calls.next() {
                    for cap in cm.captures {
                        if cap.index == callee_idx {
                            let callee = cap.node.utf8_text(src)?.to_string();
                            if callee != caller {
                                graph.add_edge(&caller, &callee);
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
