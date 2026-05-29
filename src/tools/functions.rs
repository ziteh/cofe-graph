use rmcp::schemars;
use serde::Deserialize;
use serde_json::{Value, json};

use super::rel_file;
use crate::graph::CodebaseGraph;

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

pub fn find_function(
    graph: &CodebaseGraph,
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
    graph: &CodebaseGraph,
    root: &std::path::Path,
    params: FindInFileParams,
) -> Result<Value, String> {
    let FindInFileParams { filename } = params;
    let mut results: Vec<_> = graph
        .find_functions_in_file(&filename)
        .into_iter()
        .collect();
    if results.is_empty() {
        return Err(format!("No functions found in files matching '{filename}'"));
    }
    results.sort_by(|a, b| a.file.cmp(&b.file).then(a.name.cmp(&b.name)));
    let mut by_file: serde_json::Map<String, Value> = serde_json::Map::new();
    for n in &results {
        let mut obj = serde_json::Map::new();
        obj.insert("name".into(), json!(n.name));
        obj.insert("line".into(), json!(n.line));
        obj.insert("static".into(), json!(n.is_static));
        if !n.conditions.is_empty() {
            obj.insert("conditions".into(), json!(n.conditions));
        }
        by_file
            .entry(rel_file(root, &n.file))
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .unwrap()
            .push(Value::Object(obj));
    }
    Ok(Value::Object(by_file))
}

pub fn get_source(
    graph: &CodebaseGraph,
    store: &crate::annotations::AnnotationStore,
    params: GetSourceParams,
) -> Result<Value, String> {
    let GetSourceParams { name } = params;
    match graph.nodes.get(&name) {
        Some(n) => {
            let mut header = format!("// file: {}\n// line: {}", n.file.display(), n.line);
            if let Some(sym_ann) = store.get_symbol(&name, &n.source) {
                header.push_str(&format!("\n// annotation: {}", sym_ann.insight));
            }
            let src_code = format!("{}\n\n{}", header, n.source.replace("\r\n", "\n"));
            Ok(json!(src_code))
        }
        None => Err(format!("Function '{name}' not found")),
    }
}

pub fn find_high_fan_in(graph: &CodebaseGraph, params: TopNParams) -> Result<Value, String> {
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
