mod functions;
mod globals;
mod includes;
mod macro_refs;
mod symbols;
mod types;
mod utils;

pub use macro_refs::scan_macro_refs;

use anyhow::Result;
use std::path::Path;
use tree_sitter::{Language, Parser};

use crate::graph::CallGraph;

pub fn parse_file(path: &Path, source: &str, graph: &mut CallGraph) -> Result<()> {
    let language: Language = tree_sitter_c::LANGUAGE.into();
    let mut parser = Parser::new();
    parser.set_language(&language)?;

    let tree = parser
        .parse(source.as_bytes(), None)
        .ok_or_else(|| anyhow::anyhow!("parse failed: {:?}", path))?;
    let root = tree.root_node();

    functions::parse_functions(path, source, &language, root, graph)?;
    symbols::parse_symbols(path, source, &language, root, graph)?;
    includes::parse_includes(path, source, &language, root, graph)?;
    types::parse_types(path, source, &language, root, graph)?;
    globals::parse_globals(path, source, &language, root, graph)?;

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
