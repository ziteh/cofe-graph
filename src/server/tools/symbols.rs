use rmcp::schemars;
use serde::Deserialize;
use serde_json::json;

use crate::graph::CallGraph;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FindSymbolParams {
    #[schemars(
        description = "Symbol name substring to search for (case-insensitive). Matches #define constants, function-like macros, and enum values."
    )]
    pub name: String,
}

pub fn find_symbol(graph: &CallGraph, params: FindSymbolParams) -> String {
    let FindSymbolParams { name } = params;
    let results = graph.find_symbol(&name);
    if results.is_empty() {
        return json!({"error": format!("No symbols matching '{name}'")}).to_string();
    }
    let matches: Vec<_> = results
        .iter()
        .map(|s| {
            json!({
                "name": s.name,
                "kind": s.kind.as_str(),
                "value": s.value,
                "file": s.file,
                "line": s.line,
            })
        })
        .collect();
    json!({ "matches": matches }).to_string()
}
