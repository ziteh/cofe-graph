use anyhow::Result;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Parser, Query, QueryCursor};

use crate::graph::CallGraph;

const MACRO_ARG_QUERY: &str = r#"
(call_expression
  function: (identifier) @macro.name
  arguments: (argument_list (identifier) @macro.arg))
"#;

fn is_macro_name(name: &str) -> bool {
    name.len() >= 2
        && name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// Scan for function names that appear as arguments to macro calls.
/// Must be called after all files have been parsed (needs graph.nodes to be populated).
pub fn scan_macro_refs(source: &str, graph: &mut CallGraph) -> Result<()> {
    let language: Language = tree_sitter_c::LANGUAGE.into();
    let mut parser = Parser::new();
    parser.set_language(&language)?;

    let tree = parser
        .parse(source.as_bytes(), None)
        .ok_or_else(|| anyhow::anyhow!("parse failed during macro scan"))?;
    let root = tree.root_node();

    let query = Query::new(&language, MACRO_ARG_QUERY)?;
    let name_idx = query.capture_index_for_name("macro.name").unwrap();
    let arg_idx = query.capture_index_for_name("macro.arg").unwrap();

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, root, source.as_bytes());
    while let Some(m) = matches.next() {
        let macro_name = m
            .captures
            .iter()
            .find(|c| c.index == name_idx)
            .and_then(|c| c.node.utf8_text(source.as_bytes()).ok());

        if macro_name.is_some_and(is_macro_name) {
            for cap in m.captures.iter().filter(|c| c.index == arg_idx) {
                if let Ok(arg) = cap.node.utf8_text(source.as_bytes())
                    && graph.nodes.contains_key(arg)
                {
                    graph.macro_referenced.insert(arg.to_string());
                }
            }
        }
    }

    Ok(())
}

pub fn collect_macro_refs(
    source: &str,
    nodes: &std::collections::HashMap<String, crate::graph::FunctionNode>,
) -> Result<std::collections::HashSet<String>> {
    let language: Language = tree_sitter_c::LANGUAGE.into();
    let mut parser = Parser::new();
    parser.set_language(&language)?;

    let tree = parser
        .parse(source.as_bytes(), None)
        .ok_or_else(|| anyhow::anyhow!("parse failed during macro scan"))?;
    let root = tree.root_node();

    let query = Query::new(&language, MACRO_ARG_QUERY)?;
    let name_idx = query.capture_index_for_name("macro.name").unwrap();
    let arg_idx = query.capture_index_for_name("macro.arg").unwrap();

    let mut refs = std::collections::HashSet::new();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, root, source.as_bytes());
    while let Some(m) = matches.next() {
        let macro_name = m
            .captures
            .iter()
            .find(|c| c.index == name_idx)
            .and_then(|c| c.node.utf8_text(source.as_bytes()).ok());

        if macro_name.is_some_and(is_macro_name) {
            for cap in m.captures.iter().filter(|c| c.index == arg_idx) {
                if let Ok(arg) = cap.node.utf8_text(source.as_bytes())
                    && nodes.contains_key(arg)
                {
                    refs.insert(arg.to_string());
                }
            }
        }
    }

    Ok(refs)
}

/// Collect raw identifiers that appear in macro-argument positions
/// without filtering by known function names
pub fn collect_macro_arg_candidates(source: &str) -> Result<std::collections::HashSet<String>> {
    let language: Language = tree_sitter_c::LANGUAGE.into();
    let mut parser = Parser::new();
    parser.set_language(&language)?;

    let tree = parser
        .parse(source.as_bytes(), None)
        .ok_or_else(|| anyhow::anyhow!("parse failed during macro arg scan"))?;
    let root = tree.root_node();

    let query = Query::new(&language, MACRO_ARG_QUERY)?;
    let name_idx = query.capture_index_for_name("macro.name").unwrap();
    let arg_idx = query.capture_index_for_name("macro.arg").unwrap();

    let mut candidates = std::collections::HashSet::new();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, root, source.as_bytes());
    while let Some(m) = matches.next() {
        let macro_name = m
            .captures
            .iter()
            .find(|c| c.index == name_idx)
            .and_then(|c| c.node.utf8_text(source.as_bytes()).ok());

        if macro_name.is_some_and(is_macro_name) {
            for cap in m.captures.iter().filter(|c| c.index == arg_idx) {
                if let Ok(arg) = cap.node.utf8_text(source.as_bytes()) {
                    candidates.insert(arg.to_string());
                }
            }
        }
    }

    Ok(candidates)
}
