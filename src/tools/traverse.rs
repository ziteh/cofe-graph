use rmcp::schemars;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::graph::CodebaseGraph;

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

fn collect_relationships(
    graph: &CodebaseGraph,
    names: &[String],
    depth: usize,
    callers: bool,
) -> Result<Value, String> {
    let direction = if callers { "callers" } else { "callees" };
    let mut result = serde_json::Map::new();
    for name in names {
        let mut rels = if callers {
            graph.get_callers(name, depth)
        } else {
            graph.get_callees(name, depth)
        };
        rels.sort();
        if !rels.is_empty() {
            result.insert(name.clone(), json!(rels));
        }
    }
    if result.is_empty() {
        return Err(format!(
            "No {direction} found for any of the given functions"
        ));
    }
    Ok(Value::Object(result))
}

pub fn get_callers(graph: &CodebaseGraph, params: CallersParams) -> Result<Value, String> {
    let CallersParams { names, depth } = params;
    collect_relationships(graph, &names, depth.unwrap_or(1) as usize, true)
}

pub fn get_callees(graph: &CodebaseGraph, params: CalleesParams) -> Result<Value, String> {
    let CalleesParams { names, depth } = params;
    collect_relationships(graph, &names, depth.unwrap_or(1) as usize, false)
}

pub fn get_path(graph: &CodebaseGraph, params: GetPathParams) -> Result<Value, String> {
    let GetPathParams { from, to } = params;
    match graph.find_path(&from, &to) {
        Some(path) => Ok(json!({"from": from, "to": to, "path": path})),
        None => Err(format!("No call path from '{from}' to '{to}'")),
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TraverseParams {
    #[schemars(
        description = "One or more function names — batch all related lookups into one call"
    )]
    pub names: Vec<String>,
    #[schemars(description = "BFS traversal depth (default: 1)")]
    pub depth: Option<u32>,
    #[schemars(
        description = "\"callers\" — who calls these functions; \"callees\" — what these functions call"
    )]
    pub direction: String,
}

pub fn traverse(graph: &CodebaseGraph, params: TraverseParams) -> Result<Value, String> {
    let TraverseParams {
        names,
        depth,
        direction,
    } = params;
    match direction.as_str() {
        "callers" => get_callers(graph, CallersParams { names, depth }),
        "callees" => get_callees(graph, CalleesParams { names, depth }),
        _ => Err(format!(
            "direction must be 'callers' or 'callees', got '{direction}'"
        )),
    }
}
