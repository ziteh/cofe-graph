use rmcp::schemars;
use serde::Deserialize;
use serde_json::{Value, json};

use super::rel_file;
use crate::annotations::AnnotationStore;
use crate::graph::CodebaseGraph;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchParams {
    #[schemars(description = "Name substring to search for (case-insensitive)")]
    pub name: String,
    #[schemars(
        description = "Limit to a specific kind: \"function\", \"type\", or \"symbol\". Omit to search all."
    )]
    pub kind: Option<String>,
}

/// Sort items by name, then build their JSON representation and attach any stored annotation.
fn format_results<'a, T>(
    mut items: Vec<&'a T>,
    file_of: impl Fn(&'a T) -> &'a std::path::Path,
    name_of: impl Fn(&'a T) -> &'a str,
    to_obj: impl Fn(&'a T) -> serde_json::Map<String, Value>,
    store: &AnnotationStore,
    file_shas: &std::collections::HashMap<std::path::PathBuf, String>,
) -> Vec<Value> {
    items.sort_by(|a, b| name_of(a).cmp(name_of(b)));
    items
        .into_iter()
        .map(|item| {
            let mut obj = to_obj(item);
            if let Some(ann) = file_shas
                .get(file_of(item))
                .and_then(|sha| store.get_symbol_annotation(sha, name_of(item)))
            {
                obj.insert("annotation".into(), json!(ann));
            }
            Value::Object(obj)
        })
        .collect()
}

pub fn search(
    graph: &CodebaseGraph,
    store: &AnnotationStore,
    root: &std::path::Path,
    params: SearchParams,
) -> Result<Value, String> {
    let SearchParams { name, kind } = params;
    let kind_lc = kind.as_deref().map(str::to_lowercase);
    let k = kind_lc.as_deref();

    let do_functions = k.is_none_or(|s| s == "function");
    let do_types = k.is_none_or(|s| s == "type");
    let do_symbols = k.is_none_or(|s| s == "symbol");

    if !do_functions && !do_types && !do_symbols {
        return Err(format!(
            "Unknown kind '{}' — use \"function\", \"type\", or \"symbol\"",
            kind.unwrap_or_default()
        ));
    }

    let mut result = serde_json::Map::new();
    let mut total = 0usize;

    if do_functions {
        let values = format_results(
            graph.find_function(&name),
            |n| n.file.as_path(),
            |n| n.name.as_str(),
            |n| {
                let mut obj = serde_json::Map::new();
                obj.insert("name".into(), json!(n.name));
                obj.insert("file".into(), json!(rel_file(root, &n.file)));
                obj.insert("line".into(), json!(n.line));
                obj.insert("static".into(), json!(n.is_static));
                if !n.conditions.is_empty() {
                    obj.insert("conditions".into(), json!(n.conditions));
                }
                obj
            },
            store,
            &graph.file_shas,
        );
        total += values.len();
        result.insert("functions".into(), json!(values));
    }

    if do_types {
        let values = format_results(
            graph.find_type(&name),
            |t| t.file.as_path(),
            |t| t.name.as_str(),
            |t| {
                let mut obj = serde_json::Map::new();
                obj.insert("name".into(), json!(t.name));
                obj.insert("kind".into(), json!(t.kind.as_str()));
                obj.insert("file".into(), json!(rel_file(root, &t.file)));
                obj.insert("line".into(), json!(t.line));
                obj.insert("definition".into(), json!(t.definition));
                if !t.conditions.is_empty() {
                    obj.insert("conditions".into(), json!(t.conditions));
                }
                obj
            },
            store,
            &graph.file_shas,
        );
        total += values.len();
        result.insert("types".into(), json!(values));
    }

    if do_symbols {
        let values = format_results(
            graph.find_symbol(&name),
            |s| s.file.as_path(),
            |s| s.name.as_str(),
            |s| {
                let mut obj = serde_json::Map::new();
                obj.insert("name".into(), json!(s.name));
                obj.insert("kind".into(), json!(s.kind.as_str()));
                obj.insert("value".into(), json!(s.value));
                obj.insert("file".into(), json!(rel_file(root, &s.file)));
                obj.insert("line".into(), json!(s.line));
                if !s.conditions.is_empty() {
                    obj.insert("conditions".into(), json!(s.conditions));
                }
                obj
            },
            store,
            &graph.file_shas,
        );
        total += values.len();
        result.insert("symbols".into(), json!(values));
    }

    if total == 0 {
        return Err(format!("No results matching '{name}'"));
    }

    Ok(Value::Object(result))
}
