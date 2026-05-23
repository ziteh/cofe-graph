use anyhow::Result;
use std::path::Path;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Node, Query, QueryCursor};

use crate::graph::{CallGraph, IncludeEdge};

pub fn parse_includes(
    path: &Path,
    source: &str,
    language: &Language,
    root: Node,
    graph: &mut CallGraph,
) -> Result<()> {
    let q = Query::new(language, "(preproc_include path: (_) @path)")?;
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
    Ok(())
}
