use std::collections::BTreeMap;

use rmcp::schemars;
use serde::Deserialize;
use serde_json::{Value, json};

use super::rel_file;
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

fn fn_entry(n: &crate::graph::FunctionNode, root: &std::path::Path) -> Value {
    let mut obj = serde_json::Map::new();
    obj.insert("name".into(), json!(n.name));
    obj.insert("file".into(), json!(rel_file(root, &n.file)));
    obj.insert("line".into(), json!(n.line));
    obj.insert("static".into(), json!(n.is_static));
    if !n.conditions.is_empty() {
        obj.insert("conditions".into(), json!(n.conditions));
    }
    Value::Object(obj)
}

fn compact_fn_entry(n: &crate::graph::FunctionNode) -> Value {
    let mut obj = serde_json::Map::new();
    obj.insert("name".into(), json!(n.name));
    obj.insert("line".into(), json!(n.line));
    obj.insert("static".into(), json!(n.is_static));
    if !n.conditions.is_empty() {
        obj.insert("conditions".into(), json!(n.conditions));
    }
    Value::Object(obj)
}

pub fn find_function(
    graph: &CallGraph,
    root: &std::path::Path,
    params: FindFunctionParams,
) -> Result<Value, String> {
    let FindFunctionParams { name } = params;
    let mut results: Vec<_> = graph.find_function(&name).into_iter().collect();
    results.sort_by_key(|n| &n.name);
    if results.is_empty() {
        return Err(format!("No functions matching '{name}'"));
    }
    Ok(json!(
        results
            .iter()
            .map(|n| fn_entry(n, root))
            .collect::<Vec<_>>()
    ))
}

pub fn find_functions_in_file(
    graph: &CallGraph,
    params: FindInFileParams,
) -> Result<Value, String> {
    let FindInFileParams { filename } = params;
    let mut results: Vec<_> = graph
        .find_functions_in_file(&filename)
        .into_iter()
        .collect();
    results.sort_by(|a, b| a.file.cmp(&b.file).then(a.name.cmp(&b.name)));
    if results.is_empty() {
        return Err(format!("No functions found in files matching '{filename}'"));
    }
    // Group by file
    let mut by_file: BTreeMap<&std::path::PathBuf, Vec<Value>> = BTreeMap::new();
    for n in &results {
        by_file
            .entry(&n.file)
            .or_default()
            .push(compact_fn_entry(n));
    }
    let groups: Vec<Value> = by_file
        .into_iter()
        .map(|(file, fns)| json!({ "file": file, "functions": fns }))
        .collect();
    let content = if groups.len() == 1 {
        groups.into_iter().next().unwrap()
    } else {
        json!(groups)
    };
    Ok(content)
}

pub fn get_public_api(graph: &CallGraph, params: FindInFileParams) -> Result<Value, String> {
    let FindInFileParams { filename } = params;
    let mut results: Vec<_> = graph.get_public_api(&filename).into_iter().collect();
    results.sort_by(|a, b| a.file.cmp(&b.file).then(a.name.cmp(&b.name)));
    if results.is_empty() {
        return Err(format!(
            "No public (non-static) functions found in files matching '{filename}'"
        ));
    }
    let mut by_file: BTreeMap<&std::path::PathBuf, Vec<Value>> = BTreeMap::new();
    for n in &results {
        by_file
            .entry(&n.file)
            .or_default()
            .push(compact_fn_entry(n));
    }
    let groups: Vec<Value> = by_file
        .into_iter()
        .map(|(file, fns)| json!({ "file": file, "functions": fns }))
        .collect();
    let content = if groups.len() == 1 {
        groups.into_iter().next().unwrap()
    } else {
        json!(groups)
    };
    Ok(content)
}

pub fn get_source(graph: &CallGraph, params: GetSourceParams) -> Result<Value, String> {
    let GetSourceParams { name } = params;
    match graph.nodes.get(&name) {
        Some(n) => {
            let mut obj = serde_json::Map::new();
            obj.insert("file".into(), json!(n.file));
            obj.insert("line".into(), json!(n.line));
            obj.insert("static".into(), json!(n.is_static));
            if !n.conditions.is_empty() {
                obj.insert("conditions".into(), json!(n.conditions));
            }
            obj.insert("source".into(), json!(n.source));
            Ok(Value::Object(obj))
        }
        None => Err(format!("Function '{name}' not found")),
    }
}

pub fn find_high_fan_in(graph: &CallGraph, params: TopNParams) -> Result<Value, String> {
    let TopNParams { top_n } = params;
    let ranked = graph.top_by_fan_in(top_n.unwrap_or(10) as usize);
    if ranked.is_empty() {
        return Err("No functions in graph".to_string());
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
    Ok(json!(results))
}
