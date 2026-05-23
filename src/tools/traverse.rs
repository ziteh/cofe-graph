use rmcp::schemars;
use serde::Deserialize;
use serde_json::json;

use crate::graph::CallGraph;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TraverseParams {
    #[schemars(description = "Function name")]
    pub name: String,
    #[schemars(description = "BFS traversal depth (default: 1)")]
    pub depth: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetPathParams {
    #[schemars(description = "Starting function name")]
    pub from: String,
    #[schemars(description = "Target function name")]
    pub to: String,
}

pub fn get_callers(graph: &CallGraph, params: TraverseParams) -> String {
    let TraverseParams { name, depth } = params;
    let mut callers = graph.get_callers(&name, depth.unwrap_or(1) as usize);
    callers.sort();
    if callers.is_empty() {
        return json!({"content": format!("No callers found for '{name}'"), "isError": true})
            .to_string();
    }
    json!({"content": json!({"function": name, "callers": callers}), "isError": false}).to_string()
}

pub fn get_callees(graph: &CallGraph, params: TraverseParams) -> String {
    let TraverseParams { name, depth } = params;
    let mut callees = graph.get_callees(&name, depth.unwrap_or(1) as usize);
    callees.sort();
    if callees.is_empty() {
        return json!({"content": format!("No callees found for '{name}'"), "isError": true})
            .to_string();
    }
    json!({"content": json!({"function": name, "callees": callees}), "isError": false}).to_string()
}

pub fn get_path(graph: &CallGraph, params: GetPathParams) -> String {
    let GetPathParams { from, to } = params;
    match graph.find_path(&from, &to) {
        Some(path) => {
            json!({"content": json!({"from": from, "to": to, "path": path}), "isError": false})
                .to_string()
        }
        None => {
            json!({"content": format!("No call path from '{from}' to '{to}'"), "isError": true})
                .to_string()
        }
    }
}
