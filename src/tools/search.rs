use rmcp::schemars;
use serde::Deserialize;
use serde_json::{Value, json};

use super::rel_file;
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

pub fn search(
    graph: &CodebaseGraph,
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
        let mut fns: Vec<_> = graph.find_function(&name).into_iter().collect();
        fns.sort_by(|a, b| a.name.cmp(&b.name));
        let values: Vec<Value> = fns
            .iter()
            .map(|n| {
                let mut obj = serde_json::Map::new();
                obj.insert("name".into(), json!(n.name));
                obj.insert("file".into(), json!(rel_file(root, &n.file)));
                obj.insert("line".into(), json!(n.line));
                obj.insert("static".into(), json!(n.is_static));
                if !n.conditions.is_empty() {
                    obj.insert("conditions".into(), json!(n.conditions));
                }
                Value::Object(obj)
            })
            .collect();
        total += values.len();
        result.insert("functions".into(), json!(values));
    }

    if do_types {
        let mut types: Vec<_> = graph.find_type(&name).into_iter().collect();
        types.sort_by(|a, b| a.name.cmp(&b.name));
        let values: Vec<Value> = types
            .iter()
            .map(|t| {
                let mut obj = serde_json::Map::new();
                obj.insert("name".into(), json!(t.name));
                obj.insert("kind".into(), json!(t.kind.as_str()));
                obj.insert("file".into(), json!(rel_file(root, &t.file)));
                obj.insert("line".into(), json!(t.line));
                obj.insert("definition".into(), json!(t.definition));
                if !t.conditions.is_empty() {
                    obj.insert("conditions".into(), json!(t.conditions));
                }
                Value::Object(obj)
            })
            .collect();
        total += values.len();
        result.insert("types".into(), json!(values));
    }

    if do_symbols {
        let mut syms: Vec<_> = graph.find_symbol(&name).into_iter().collect();
        syms.sort_by(|a, b| a.name.cmp(&b.name));
        let values: Vec<Value> = syms
            .iter()
            .map(|s| {
                let mut obj = serde_json::Map::new();
                obj.insert("name".into(), json!(s.name));
                obj.insert("kind".into(), json!(s.kind.as_str()));
                obj.insert("value".into(), json!(s.value));
                obj.insert("file".into(), json!(rel_file(root, &s.file)));
                obj.insert("line".into(), json!(s.line));
                if !s.conditions.is_empty() {
                    obj.insert("conditions".into(), json!(s.conditions));
                }
                Value::Object(obj)
            })
            .collect();
        total += values.len();
        result.insert("symbols".into(), json!(values));
    }

    if total == 0 {
        return Err(format!("No results matching '{name}'"));
    }

    Ok(Value::Object(result))
}
