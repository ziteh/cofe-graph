use anyhow::Result;
use std::path::Path;
use std::sync::OnceLock;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Node, Query, QueryCursor};

use super::utils::{cap_text, cap_text_opt, trim_value};
use crate::graph::{CodebaseGraph, SymbolKind, SymbolNode};

const DEFINE_QUERY: &str = r#"
(preproc_def
  name: (identifier) @name
  value: (preproc_arg)? @value)
"#;

const MACRO_FN_QUERY: &str = r#"
(preproc_function_def
  name: (identifier) @name
  parameters: (preproc_params) @params
  value: (preproc_arg)? @value)
"#;

const ENUM_VALUE_QUERY: &str = r#"
(enumerator
  name: (identifier) @name
  value: (_)? @value)
"#;

static DEFINE_QUERY_COMPILED: OnceLock<Query> = OnceLock::new();
static MACRO_FN_QUERY_COMPILED: OnceLock<Query> = OnceLock::new();
static ENUM_VALUE_QUERY_COMPILED: OnceLock<Query> = OnceLock::new();

fn define_query() -> &'static Query {
    DEFINE_QUERY_COMPILED.get_or_init(|| {
        let language: Language = tree_sitter_c::LANGUAGE.into();
        Query::new(&language, DEFINE_QUERY).expect("invalid query")
    })
}

fn macro_fn_query() -> &'static Query {
    MACRO_FN_QUERY_COMPILED.get_or_init(|| {
        let language: Language = tree_sitter_c::LANGUAGE.into();
        Query::new(&language, MACRO_FN_QUERY).expect("invalid query")
    })
}

fn enum_value_query() -> &'static Query {
    ENUM_VALUE_QUERY_COMPILED.get_or_init(|| {
        let language: Language = tree_sitter_c::LANGUAGE.into();
        Query::new(&language, ENUM_VALUE_QUERY).expect("invalid query")
    })
}

pub fn parse_symbols(
    path: &Path,
    source: &str,
    _language: &Language,
    root: Node,
    graph: &mut CodebaseGraph,
) -> Result<()> {
    let src = source.as_bytes();

    // Object-like #define
    {
        let q = define_query();
        let name_idx = q.capture_index_for_name("name").unwrap();
        let val_idx = q.capture_index_for_name("value").unwrap();
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(q, root, src);
        while let Some(m) = matches.next() {
            if let Some(name) = cap_text(m, name_idx, src) {
                let name_node = m
                    .captures
                    .iter()
                    .find(|c| c.index == name_idx)
                    .unwrap()
                    .node;
                graph.insert_symbol(SymbolNode {
                    name,
                    kind: SymbolKind::Define,
                    conditions: crate::parser::utils::extract_conditions(name_node, src),
                    value: cap_text_opt(m, val_idx, src).map(trim_value),
                    file: path.to_path_buf(),
                    line: m
                        .captures
                        .iter()
                        .find(|c| c.index == name_idx)
                        .map_or(0, |c| c.node.start_position().row as u32 + 1),
                });
            }
        }
    }

    // Function-like #define FOO(x) ...
    {
        let q = macro_fn_query();
        let name_idx = q.capture_index_for_name("name").unwrap();
        let params_idx = q.capture_index_for_name("params").unwrap();
        let val_idx = q.capture_index_for_name("value").unwrap();
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(q, root, src);
        while let Some(m) = matches.next() {
            if let Some(name) = cap_text(m, name_idx, src) {
                let params = cap_text_opt(m, params_idx, src);
                let value = cap_text_opt(m, val_idx, src);
                let display = match (params, value) {
                    (Some(p), Some(v)) => Some(format!("{p} {}", trim_value(v))),
                    (Some(p), None) => Some(p),
                    (None, Some(v)) => Some(trim_value(v)),
                    (None, None) => None,
                };
                let name_node = m
                    .captures
                    .iter()
                    .find(|c| c.index == name_idx)
                    .unwrap()
                    .node;
                graph.insert_symbol(SymbolNode {
                    name,
                    kind: SymbolKind::MacroFn,
                    conditions: crate::parser::utils::extract_conditions(name_node, src),
                    value: display,
                    file: path.to_path_buf(),
                    line: m
                        .captures
                        .iter()
                        .find(|c| c.index == name_idx)
                        .map_or(0, |c| c.node.start_position().row as u32 + 1),
                });
            }
        }
    }

    // Enum values
    {
        let q = enum_value_query();
        let name_idx = q.capture_index_for_name("name").unwrap();
        let val_idx = q.capture_index_for_name("value").unwrap();
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(q, root, src);
        while let Some(m) = matches.next() {
            if let Some(name) = cap_text(m, name_idx, src) {
                let name_node = m
                    .captures
                    .iter()
                    .find(|c| c.index == name_idx)
                    .unwrap()
                    .node;
                graph.insert_symbol(SymbolNode {
                    name,
                    kind: SymbolKind::EnumValue,
                    conditions: crate::parser::utils::extract_conditions(name_node, src),
                    value: cap_text_opt(m, val_idx, src).map(|v| v.trim().to_string()),
                    file: path.to_path_buf(),
                    line: m
                        .captures
                        .iter()
                        .find(|c| c.index == name_idx)
                        .map_or(0, |c| c.node.start_position().row as u32 + 1),
                });
            }
        }
    }

    Ok(())
}
