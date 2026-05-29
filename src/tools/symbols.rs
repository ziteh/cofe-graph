use rmcp::schemars;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::graph::CodebaseGraph;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FindSymbolParams {
    #[schemars(
        description = "Symbol name substring to search for (case-insensitive). Matches #define constants, function-like macros, and enum values."
    )]
    pub name: String,
}

pub fn find_symbol(graph: &CodebaseGraph, params: FindSymbolParams) -> Result<Value, String> {
    let FindSymbolParams { name } = params;
    let results = graph.find_symbol(&name);
    if results.is_empty() {
        return Err(format!("No symbols matching '{name}'"));
    }
    let matches: Vec<Value> = results
        .iter()
        .map(|s| {
            let mut obj = serde_json::Map::new();
            obj.insert("name".into(), json!(s.name));
            obj.insert("kind".into(), json!(s.kind.as_str()));
            obj.insert("value".into(), json!(s.value));
            obj.insert("file".into(), json!(s.file));
            obj.insert("line".into(), json!(s.line));
            if !s.conditions.is_empty() {
                obj.insert("conditions".into(), json!(s.conditions));
            }
            Value::Object(obj)
        })
        .collect();
    Ok(json!(matches))
}
