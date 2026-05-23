use anyhow::Result;
use std::path::Path;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Node, Query, QueryCursor};

use crate::graph::{CallGraph, TypeKind, TypeNode};

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

pub fn parse_types(
    path: &Path,
    source: &str,
    language: &Language,
    root: Node,
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
                graph.insert_type(TypeNode {
                    name,
                    kind: kind.clone(),
                    definition,
                    file: path.to_path_buf(),
                    line: nc.node.start_position().row as u32 + 1,
                });
            }
        }
    }

    Ok(())
}
