use rmcp::schemars;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::graph::CallGraph;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FindFunctionParams {
    #[schemars(description = "Function name substring to search for (case-insensitive)")]
    pub name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FindInFileParams {
    #[schemars(description = "Filename substring to match against file paths (case-insensitive)")]
    pub filename: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetSourceParams {
    #[schemars(description = "Exact function name")]
    pub name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TopNParams {
    #[schemars(description = "Number of results to return (default: 10)")]
    pub top_n: Option<u32>,
}

fn fn_entry(n: &crate::graph::FunctionNode) -> Value {
    json!({
        "name": n.name,
        "file": n.file,
        "line": n.line,
        "is_static": n.is_static,
    })
}

pub fn find_function(graph: &CallGraph, params: FindFunctionParams) -> String {
    let FindFunctionParams { name } = params;
    let mut results: Vec<_> = graph.find_function(&name).into_iter().collect();
    results.sort_by_key(|n| &n.name);
    if results.is_empty() {
        return json!({"error": format!("No functions matching '{name}'")}).to_string();
    }
    json!({ "matches": results.iter().map(|n| fn_entry(n)).collect::<Vec<_>>() }).to_string()
}

pub fn find_functions_in_file(graph: &CallGraph, params: FindInFileParams) -> String {
    let FindInFileParams { filename } = params;
    let mut results: Vec<_> = graph
        .find_functions_in_file(&filename)
        .into_iter()
        .collect();
    results.sort_by_key(|n| &n.name);
    if results.is_empty() {
        return json!({"error": format!("No functions found in files matching '{filename}'")})
            .to_string();
    }
    json!({ "matches": results.iter().map(|n| fn_entry(n)).collect::<Vec<_>>() }).to_string()
}

pub fn get_public_api(graph: &CallGraph, params: FindInFileParams) -> String {
    let FindInFileParams { filename } = params;
    let mut results: Vec<_> = graph.get_public_api(&filename).into_iter().collect();
    results.sort_by_key(|n| &n.name);
    if results.is_empty() {
        return json!({"error": format!("No public (non-static) functions found in files matching '{filename}'")})
            .to_string();
    }
    json!({ "matches": results.iter().map(|n| fn_entry(n)).collect::<Vec<_>>() }).to_string()
}

pub fn get_source(graph: &CallGraph, params: GetSourceParams) -> String {
    let GetSourceParams { name } = params;
    match graph.nodes.get(&name) {
        Some(n) => json!({
            "name": n.name,
            "file": n.file,
            "line": n.line,
            "is_static": n.is_static,
            "source": n.source,
        })
        .to_string(),
        None => json!({"error": format!("Function '{name}' not found")}).to_string(),
    }
}

pub fn find_high_fan_in(graph: &CallGraph, params: TopNParams) -> String {
    let TopNParams { top_n } = params;
    let ranked = graph.top_by_fan_in(top_n.unwrap_or(10) as usize);
    if ranked.is_empty() {
        return json!({"error": "No functions in graph"}).to_string();
    }
    let results: Vec<Value> = ranked
        .iter()
        .enumerate()
        .map(|(i, (name, count))| {
            let loc = graph
                .nodes
                .get(*name)
                .map(|nd| json!({"file": nd.file, "line": nd.line}));
            json!({
                "rank": i + 1,
                "name": name,
                "callers": count,
                "location": loc,
            })
        })
        .collect();
    json!({ "results": results }).to_string()
}
