use serde_json::{Value, json};

use super::functions::GetSourceParams;
use super::includes::GetIncludesParams;
use super::rel_file;
use super::types::GetTypeUsersParams;
use crate::graph::CallGraph;

pub fn get_globals(
    graph: &CallGraph,
    root: &std::path::Path,
    params: GetIncludesParams,
) -> Result<Value, String> {
    let GetIncludesParams { file } = params;
    let results = graph.find_globals(&file);
    if results.is_empty() {
        return Err(format!(
            "No global variables found in files matching '{file}'"
        ));
    }
    let matches: Vec<Value> = results
        .iter()
        .map(|v| {
            let mut obj = serde_json::Map::new();
            obj.insert("name".into(), json!(v.name));
            obj.insert("file".into(), json!(rel_file(root, &v.file)));
            obj.insert("line".into(), json!(v.line));
            obj.insert("decl".into(), json!(v.decl));
            obj.insert("static".into(), json!(v.is_static));
            if !v.conditions.is_empty() {
                obj.insert("conditions".into(), json!(v.conditions));
            }
            Value::Object(obj)
        })
        .collect();
    Ok(json!(matches))
}

pub fn get_global_users(
    graph: &CallGraph,
    root: &std::path::Path,
    params: GetTypeUsersParams,
) -> Result<Value, String> {
    let GetTypeUsersParams { name } = params;
    let results = graph.get_global_users(&name);
    if results.is_empty() {
        return Err(format!("No functions reference global '{name}'"));
    }
    let users: Vec<Value> = results
        .iter()
        .map(|n| {
            let mut obj = serde_json::Map::new();
            obj.insert("name".into(), json!(n.name));
            obj.insert("file".into(), json!(rel_file(root, &n.file)));
            obj.insert("line".into(), json!(n.line));
            obj.insert("static".into(), json!(n.is_static));
            Value::Object(obj)
        })
        .collect();
    Ok(json!(users))
}

pub fn get_fn_globals(
    graph: &CallGraph,
    root: &std::path::Path,
    params: GetSourceParams,
) -> Result<Value, String> {
    let GetSourceParams { name } = params;
    let results = graph.get_fn_globals(&name);
    if results.is_empty() {
        if graph.nodes.contains_key(&name) {
            return Err(format!(
                "'{name}' does not reference any indexed global variables"
            ));
        }
        return Err(format!("Function '{name}' not found"));
    }
    let globals: Vec<Value> = results
        .iter()
        .map(|v| {
            let mut obj = serde_json::Map::new();
            obj.insert("name".into(), json!(v.name));
            obj.insert("file".into(), json!(rel_file(root, &v.file)));
            obj.insert("line".into(), json!(v.line));
            obj.insert("static".into(), json!(v.is_static));
            Value::Object(obj)
        })
        .collect();
    Ok(json!(globals))
}
