use anyhow::Result;
use std::path::Path;
use std::sync::OnceLock;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Node, Query, QueryCursor};

use crate::graph::{CodebaseGraph, TypeKind, TypeNode};

const STRUCT_QUERY: &str = r#"
(struct_specifier
  name: (type_identifier) @name
  body: (field_declaration_list)) @def
"#;

const UNION_QUERY: &str = r#"
(union_specifier
  name: (type_identifier) @name
  body: (field_declaration_list)) @def
"#;

const ENUM_TYPE_QUERY: &str = r#"
(enum_specifier
  name: (type_identifier) @name
  body: (enumerator_list)) @def
"#;

const TYPEDEF_QUERY: &str = r#"
(type_definition
  declarator: (type_identifier) @name) @def
"#;

static STRUCT_QUERY_COMPILED: OnceLock<Query> = OnceLock::new();
static UNION_QUERY_COMPILED: OnceLock<Query> = OnceLock::new();
static ENUM_TYPE_QUERY_COMPILED: OnceLock<Query> = OnceLock::new();
static TYPEDEF_QUERY_COMPILED: OnceLock<Query> = OnceLock::new();

fn compiled_queries() -> [(&'static Query, TypeKind); 4] {
    [
        (
            STRUCT_QUERY_COMPILED.get_or_init(|| {
                let language: Language = tree_sitter_c::LANGUAGE.into();
                Query::new(&language, STRUCT_QUERY).expect("invalid query")
            }),
            TypeKind::Struct,
        ),
        (
            UNION_QUERY_COMPILED.get_or_init(|| {
                let language: Language = tree_sitter_c::LANGUAGE.into();
                Query::new(&language, UNION_QUERY).expect("invalid query")
            }),
            TypeKind::Union,
        ),
        (
            ENUM_TYPE_QUERY_COMPILED.get_or_init(|| {
                let language: Language = tree_sitter_c::LANGUAGE.into();
                Query::new(&language, ENUM_TYPE_QUERY).expect("invalid query")
            }),
            TypeKind::Enum,
        ),
        (
            TYPEDEF_QUERY_COMPILED.get_or_init(|| {
                let language: Language = tree_sitter_c::LANGUAGE.into();
                Query::new(&language, TYPEDEF_QUERY).expect("invalid query")
            }),
            TypeKind::Typedef,
        ),
    ]
}

pub fn parse_types(
    path: &Path,
    source: &str,
    _language: &Language,
    root: Node,
    graph: &mut CodebaseGraph,
) -> Result<()> {
    let src = source.as_bytes();

    for (q, kind) in compiled_queries() {
        let name_idx = q.capture_index_for_name("name").unwrap();
        let def_idx = q.capture_index_for_name("def").unwrap();
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(q, root, src);

        while let Some(m) = matches.next() {
            let name_cap = m.captures.iter().find(|c| c.index == name_idx);
            let def_cap = m.captures.iter().find(|c| c.index == def_idx);

            if let (Some(nc), Some(dc)) = (name_cap, def_cap) {
                let name = nc.node.utf8_text(src)?.to_string();
                let definition = dc.node.utf8_text(src).unwrap_or("").trim().to_string();

                graph.insert_type(TypeNode {
                    name,
                    kind: kind.clone(),
                    definition,
                    conditions: crate::parser::utils::extract_conditions(dc.node, src),
                    file: path.to_path_buf(),
                    line: nc.node.start_position().row as u32 + 1,
                });
            }
        }
    }

    Ok(())
}
