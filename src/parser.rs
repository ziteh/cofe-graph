use crate::graph::{CallGraph, FunctionNode};
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
                graph.insert_node(FunctionNode {
                    name,
                    file: path.to_path_buf(),
                    line,
                    source: src,
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
