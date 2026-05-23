use rmcp::schemars;
use serde::Deserialize;

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
        return format!("No callers found for '{name}'");
    }
    callers.join("\n")
}

pub fn get_callees(graph: &CallGraph, params: TraverseParams) -> String {
    let TraverseParams { name, depth } = params;
    let mut callees = graph.get_callees(&name, depth.unwrap_or(1) as usize);
    callees.sort();
    if callees.is_empty() {
        return format!("No callees found for '{name}'");
    }
    callees.join("\n")
}

pub fn get_path(graph: &CallGraph, params: GetPathParams) -> String {
    let GetPathParams { from, to } = params;
    match graph.find_path(&from, &to) {
        Some(path) => path.join(" -> "),
        None => format!("No call path from '{from}' to '{to}'"),
    }
}
