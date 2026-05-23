use crate::graph::{
    CallGraph, FunctionNode, IncludeEdge, SymbolKind, SymbolNode, TypeKind, TypeNode,
};
use anyhow::Result;
use std::path::Path;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Parser, Query, QueryCursor};

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

// Named struct: struct foo_t { ... };
const STRUCT_QUERY: &str = r#"
(struct_specifier
  name: (type_identifier) @name
  body: (field_declaration_list)) @def
"#;

// Named union: union foo_t { ... };
const UNION_QUERY: &str = r#"
(union_specifier
  name: (type_identifier) @name
  body: (field_declaration_list)) @def
"#;

// Named enum type: enum foo_t { ... };
const ENUM_TYPE_QUERY: &str = r#"
(enum_specifier
  name: (type_identifier) @name
  body: (enumerator_list)) @def
"#;

// typedef ... alias_name;
const TYPEDEF_QUERY: &str = r#"
(type_definition
  declarator: (type_identifier) @name) @def
"#;

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

pub fn parse_file(path: &Path, source: &str, graph: &mut CallGraph) -> Result<()> {
    let language: Language = tree_sitter_c::LANGUAGE.into();
    let mut parser = Parser::new();
    parser.set_language(&language)?;

    let tree = parser
        .parse(source.as_bytes(), None)
        .ok_or_else(|| anyhow::anyhow!("parse failed: {:?}", path))?;
    let root = tree.root_node();

    let fn_query = Query::new(&language, FUNCTION_DEF_QUERY)?;
    let name_idx = fn_query.capture_index_for_name("fn.name").unwrap();
    let def_idx = fn_query.capture_index_for_name("fn.def").unwrap();
    let call_query = Query::new(&language, CALL_EXPR_QUERY)?;
    let callee_idx = call_query.capture_index_for_name("callee").unwrap();

    // Pass 1: extract function definitions
    {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&fn_query, root, source.as_bytes());
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
                let name = nn.utf8_text(source.as_bytes())?.to_string();
                let src = dn.utf8_text(source.as_bytes())?.to_string();
                let line = nn.start_position().row as u32 + 1;
                let is_static = dn.children(&mut dn.walk()).any(|child| {
                    child.kind() == "storage_class_specifier"
                        && child.utf8_text(source.as_bytes()).ok() == Some("static")
                });
                graph.insert_node(FunctionNode {
                    name,
                    file: path.to_path_buf(),
                    line,
                    source: src,
                    is_static,
                });
            }
        }
    }

    // Pass 2: extract call edges per function body
    {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&fn_query, root, source.as_bytes());
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
                let caller = nn.utf8_text(source.as_bytes())?.to_string();
                let mut call_cursor = QueryCursor::new();
                let mut calls = call_cursor.matches(&call_query, dn, source.as_bytes());
                while let Some(cm) = calls.next() {
                    for cap in cm.captures {
                        if cap.index == callee_idx {
                            let callee = cap.node.utf8_text(source.as_bytes())?.to_string();
                            if callee != caller {
                                graph.add_edge(&caller, &callee);
                            }
                        }
                    }
                }
            }
        }
    }

    // Pass 3: extract #define constants, function-like macros, and enum values
    parse_symbols(path, source, &language, root, graph)?;

    // Pass 4: extract #include edges
    {
        let q = Query::new(&language, "(preproc_include path: (_) @path)")?;
        let path_idx = q.capture_index_for_name("path").unwrap();
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&q, root, source.as_bytes());
        let mut edges: Vec<IncludeEdge> = Vec::new();
        while let Some(m) = matches.next() {
            for cap in m.captures.iter().filter(|c| c.index == path_idx) {
                let node = cap.node;
                let raw = node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
                let is_system = node.kind() == "system_lib_string";
                let inner = raw
                    .trim_matches(|c: char| c == '"' || c == '<' || c == '>')
                    .to_string();
                edges.push(IncludeEdge {
                    path: inner,
                    is_system,
                });
            }
        }
        graph.includes.insert(path.to_path_buf(), edges);
    }

    // Pass 5: extract type definitions (struct / union / enum / typedef)
    parse_types(path, source, &language, root, graph)?;

    Ok(())
}

fn parse_types(
    path: &Path,
    source: &str,
    language: &Language,
    root: tree_sitter::Node,
    graph: &mut CallGraph,
) -> Result<()> {
    let src = source.as_bytes();

    let specs: &[(&str, TypeKind)] = &[
        (STRUCT_QUERY, TypeKind::Struct),
        (UNION_QUERY, TypeKind::Union),
        (ENUM_TYPE_QUERY, TypeKind::Enum),
        (TYPEDEF_QUERY, TypeKind::Typedef),
    ];

    for (query_str, kind) in specs {
        let q = Query::new(language, query_str)?;
        let name_idx = q.capture_index_for_name("name").unwrap();
        let def_idx = q.capture_index_for_name("def").unwrap();
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&q, root, src);
        while let Some(m) = matches.next() {
            let name_cap = m.captures.iter().find(|c| c.index == name_idx);
            let def_cap = m.captures.iter().find(|c| c.index == def_idx);
            if let (Some(nc), Some(dc)) = (name_cap, def_cap) {
                let name = nc.node.utf8_text(src)?.to_string();
                let raw = dc.node.utf8_text(src).unwrap_or("").trim().to_string();
                let definition = if raw.len() > 500 {
                    format!("{}…", &raw[..500])
                } else {
                    raw
                };
                let line = nc.node.start_position().row as u32 + 1;
                graph.insert_type(TypeNode {
                    name,
                    kind: kind.clone(),
                    definition,
                    file: path.to_path_buf(),
                    line,
                });
            }
        }
    }

    Ok(())
}

fn parse_symbols(
    path: &Path,
    source: &str,
    language: &Language,
    root: tree_sitter::Node,
    graph: &mut CallGraph,
) -> Result<()> {
    let src = source.as_bytes();

    // Object-like #define
    {
        let q = Query::new(language, DEFINE_QUERY)?;
        let name_idx = q.capture_index_for_name("name").unwrap();
        let val_idx = q.capture_index_for_name("value").unwrap();
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&q, root, src);
        while let Some(m) = matches.next() {
            let name = cap_text(&m, name_idx, src);
            let value = cap_text_opt(&m, val_idx, src);
            if let Some(name) = name {
                graph.insert_symbol(SymbolNode {
                    name,
                    kind: SymbolKind::Define,
                    value: value.map(trim_value),
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
        let q = Query::new(language, MACRO_FN_QUERY)?;
        let name_idx = q.capture_index_for_name("name").unwrap();
        let params_idx = q.capture_index_for_name("params").unwrap();
        let val_idx = q.capture_index_for_name("value").unwrap();
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&q, root, src);
        while let Some(m) = matches.next() {
            let name = cap_text(&m, name_idx, src);
            let params = cap_text_opt(&m, params_idx, src);
            let value = cap_text_opt(&m, val_idx, src);
            if let Some(name) = name {
                let display = match (params, value) {
                    (Some(p), Some(v)) => Some(format!("{p} {}", trim_value(v))),
                    (Some(p), None) => Some(p),
                    (None, Some(v)) => Some(trim_value(v)),
                    (None, None) => None,
                };
                graph.insert_symbol(SymbolNode {
                    name,
                    kind: SymbolKind::MacroFn,
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
        let q = Query::new(language, ENUM_VALUE_QUERY)?;
        let name_idx = q.capture_index_for_name("name").unwrap();
        let val_idx = q.capture_index_for_name("value").unwrap();
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&q, root, src);
        while let Some(m) = matches.next() {
            let name = cap_text(&m, name_idx, src);
            let value = cap_text_opt(&m, val_idx, src);
            if let Some(name) = name {
                graph.insert_symbol(SymbolNode {
                    name,
                    kind: SymbolKind::EnumValue,
                    value: value.map(|v| v.trim().to_string()),
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

fn cap_text<'a>(m: &tree_sitter::QueryMatch<'_, 'a>, idx: u32, src: &'a [u8]) -> Option<String> {
    m.captures
        .iter()
        .find(|c| c.index == idx)
        .and_then(|c| c.node.utf8_text(src).ok())
        .map(|s| s.to_string())
}

fn cap_text_opt<'a>(
    m: &tree_sitter::QueryMatch<'_, 'a>,
    idx: u32,
    src: &'a [u8],
) -> Option<String> {
    cap_text(m, idx, src)
}

fn trim_value(s: String) -> String {
    let trimmed = s.trim();
    if trimmed.len() > 200 {
        format!("{}…", &trimmed[..200])
    } else {
        trimmed.to_string()
    }
}

/// Pass 4: scan for function names that appear as arguments to macro calls.
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

        if macro_name.map_or(false, is_macro_name) {
            for cap in m.captures.iter().filter(|c| c.index == arg_idx) {
                if let Ok(arg) = cap.node.utf8_text(source.as_bytes()) {
                    if graph.nodes.contains_key(arg) {
                        graph.macro_referenced.insert(arg.to_string());
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn build_fixture_graph() -> CallGraph {
        let mut graph = CallGraph::default();
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        for name in ["math.c", "main.c"] {
            let path = fixtures.join(name);
            let src = std::fs::read_to_string(&path).expect("fixture missing");
            parse_file(&path, &src, &mut graph).expect("parse failed");
        }
        graph
    }

    #[test]
    fn finds_function_by_name() {
        let graph = build_fixture_graph();
        let results = graph.find_function("add");
        assert!(!results.is_empty(), "should find 'add'");
        assert!(results.iter().any(|n| n.name == "add"));
    }

    #[test]
    fn callees_of_main_depth1() {
        let graph = build_fixture_graph();
        let callees = graph.get_callees("main", 1);
        assert!(callees.contains(&"add".to_string()));
        assert!(callees.contains(&"subtract".to_string()));
        assert!(callees.contains(&"multiply".to_string()));
        assert!(callees.contains(&"print_result".to_string()));
    }

    #[test]
    fn callers_of_add_depth2() {
        let graph = build_fixture_graph();
        let callers = graph.get_callers("add", 2);
        assert!(callers.contains(&"multiply".to_string()));
        assert!(callers.contains(&"main".to_string()));
    }

    #[test]
    fn callees_of_multiply() {
        let graph = build_fixture_graph();
        let callees = graph.get_callees("multiply", 1);
        assert!(callees.contains(&"add".to_string()));
        assert!(callees.contains(&"subtract".to_string()));
    }
}
