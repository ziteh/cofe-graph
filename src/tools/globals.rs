use serde_json::json;

use super::functions::GetSourceParams;
use super::includes::GetIncludesParams;
use super::types::GetTypeUsersParams;
use crate::graph::CallGraph;

pub fn get_globals(graph: &CallGraph, params: GetIncludesParams) -> String {
    let GetIncludesParams { file } = params;
    let results = graph.find_globals(&file);
    if results.is_empty() {
        return json!({"content": format!("No global variables found in files matching '{file}'"), "isError": true})
            .to_string();
    }
    let matches: Vec<_> = results
        .iter()
        .map(|v| {
            json!({
                "name": v.name,
                "is_static": v.is_static,
                "file": v.file,
                "line": v.line,
                "decl": v.decl,
                "conditions": v.conditions,
            })
        })
        .collect();
    json!({"content": matches, "isError": false}).to_string()
}

pub fn get_global_users(graph: &CallGraph, params: GetTypeUsersParams) -> String {
    let GetTypeUsersParams { name } = params;
    let results = graph.get_global_users(&name);
    if results.is_empty() {
        return json!({"content": format!("No functions reference global '{name}'"), "isError": true}).to_string();
    }
    let users: Vec<_> = results
        .iter()
        .map(|n| json!({"name": n.name, "file": n.file, "line": n.line, "is_static": n.is_static}))
        .collect();
    json!({"content": json!({"global": name, "users": users}), "isError": false}).to_string()
}

pub fn get_fn_globals(graph: &CallGraph, params: GetSourceParams) -> String {
    let GetSourceParams { name } = params;
    let results = graph.get_fn_globals(&name);
    if results.is_empty() {
        if graph.nodes.contains_key(&name) {
            return json!({"content": format!("'{name}' does not reference any indexed global variables"), "isError": true})
                .to_string();
        }
        return json!({"content": format!("Function '{name}' not found"), "isError": true})
            .to_string();
    }
    let globals: Vec<_> = results
        .iter()
        .map(|v| {
            json!({
                "name": v.name,
                "is_static": v.is_static,
                "file": v.file,
                "line": v.line,
            })
        })
        .collect();
    json!({"content": json!({"function": name, "globals": globals}), "isError": false}).to_string()
}
