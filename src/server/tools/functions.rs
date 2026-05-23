use rmcp::schemars;
use serde::Deserialize;

use super::fmt_fn;
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

pub fn find_function(graph: &CallGraph, params: FindFunctionParams) -> String {
    let FindFunctionParams { name } = params;
    let mut results: Vec<String> = graph.find_function(&name).into_iter().map(fmt_fn).collect();
    results.sort();
    if results.is_empty() {
        return format!("No functions matching '{name}'");
    }
    results.join("\n")
}

pub fn find_functions_in_file(graph: &CallGraph, params: FindInFileParams) -> String {
    let FindInFileParams { filename } = params;
    let mut results: Vec<String> = graph
        .find_functions_in_file(&filename)
        .into_iter()
        .map(fmt_fn)
        .collect();
    results.sort();
    if results.is_empty() {
        return format!("No functions found in files matching '{filename}'");
    }
    results.join("\n")
}

pub fn get_public_api(graph: &CallGraph, params: FindInFileParams) -> String {
    let FindInFileParams { filename } = params;
    let mut results: Vec<String> = graph
        .get_public_api(&filename)
        .into_iter()
        .map(fmt_fn)
        .collect();
    results.sort();
    if results.is_empty() {
        return format!("No public (non-static) functions found in files matching '{filename}'");
    }
    results.join("\n")
}

pub fn get_source(graph: &CallGraph, params: GetSourceParams) -> String {
    let GetSourceParams { name } = params;
    match graph.nodes.get(&name) {
        Some(n) => {
            let vis = if n.is_static { " [static]" } else { "" };
            format!("// {}:{}{}\n{}", n.file.display(), n.line, vis, n.source)
        }
        None => format!("Function '{name}' not found"),
    }
}

pub fn find_high_fan_in(graph: &CallGraph, params: TopNParams) -> String {
    let TopNParams { top_n } = params;
    let ranked = graph.top_by_fan_in(top_n.unwrap_or(10) as usize);
    if ranked.is_empty() {
        return "No functions in graph".to_string();
    }
    ranked
        .iter()
        .enumerate()
        .map(|(i, (name, count))| {
            let loc = graph.nodes.get(*name).map_or(String::new(), |nd| {
                format!(" @ {}:{}", nd.file.display(), nd.line)
            });
            format!("{}. {name}{loc} — {count} callers", i + 1)
        })
        .collect::<Vec<_>>()
        .join("\n")
}
