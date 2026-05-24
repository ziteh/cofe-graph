use rmcp::schemars;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::graph::CallGraph;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CallersParams {
    #[schemars(
        description = "One or more function names — batch all related lookups into one call"
    )]
    pub names: Vec<String>,
    #[schemars(description = "BFS traversal depth (default: 1)")]
    pub depth: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CalleesParams {
    #[schemars(
        description = "One or more function names — batch all related lookups into one call"
    )]
    pub names: Vec<String>,
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

pub fn get_callers(graph: &CallGraph, params: CallersParams) -> Result<Value, String> {
    let CallersParams { names, depth } = params;
    let d = depth.unwrap_or(1) as usize;
    let mut result = serde_json::Map::new();
    for name in &names {
        let mut callers = graph.get_callers(name, d);
        callers.sort();
        if !callers.is_empty() {
            result.insert(name.clone(), json!(callers));
        }
    }
    if result.is_empty() {
        return Err("No callers found for any of the given functions".to_string());
    }
    Ok(Value::Object(result))
}

pub fn get_callees(graph: &CallGraph, params: CalleesParams) -> Result<Value, String> {
    let CalleesParams { names, depth } = params;
    let d = depth.unwrap_or(1) as usize;
    let mut result = serde_json::Map::new();
    for name in &names {
        let mut callees = graph.get_callees(name, d);
        callees.sort();
        if !callees.is_empty() {
            result.insert(name.clone(), json!(callees));
        }
    }
    if result.is_empty() {
        return Err("No callees found for any of the given functions".to_string());
    }
    Ok(Value::Object(result))
}

pub fn get_path(graph: &CallGraph, params: GetPathParams) -> Result<Value, String> {
    let GetPathParams { from, to } = params;
    match graph.find_path(&from, &to) {
        Some(path) => Ok(json!({"from": from, "to": to, "path": path})),
        None => Err(format!("No call path from '{from}' to '{to}'")),
    }
}
