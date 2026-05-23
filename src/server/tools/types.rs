use rmcp::schemars;
use serde::Deserialize;
use serde_json::json;

use crate::graph::CallGraph;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FindTypeParams {
    #[schemars(
        description = "Type name substring to search for (case-insensitive). Matches struct, union, enum, and typedef names."
    )]
    pub name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetTypeUsersParams {
    #[schemars(description = "Exact type name to search for in function bodies (e.g. ble_evt_t)")]
    pub name: String,
}

pub fn find_type(graph: &CallGraph, params: FindTypeParams) -> String {
    let FindTypeParams { name } = params;
    let results = graph.find_type(&name);
    if results.is_empty() {
        return json!({"error": format!("No types matching '{name}'")}).to_string();
    }
    let matches: Vec<_> = results
        .iter()
        .map(|t| {
            json!({
                "name": t.name,
                "kind": t.kind.as_str(),
                "file": t.file,
                "line": t.line,
                "definition": t.definition,
            })
        })
        .collect();
    json!({ "matches": matches }).to_string()
}

pub fn get_type_users(graph: &CallGraph, params: GetTypeUsersParams) -> String {
    let GetTypeUsersParams { name } = params;
    let results = graph.get_type_users(&name);
    if results.is_empty() {
        return json!({"error": format!("No functions reference type '{name}'")}).to_string();
    }
    let users: Vec<_> = results
        .iter()
        .map(|n| json!({"name": n.name, "file": n.file, "line": n.line, "is_static": n.is_static}))
        .collect();
    json!({ "type": name, "users": users }).to_string()
}
