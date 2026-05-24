use rmcp::schemars;
use serde::Deserialize;
use serde_json::{Value, json};

use super::rel_file;
use crate::graph::CallGraph;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FindTypeParams {
    #[schemars(
        description = "Type name substring to search for (case-insensitive). Matches struct, union, enum, and typedef names."
    )]
    pub name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetTypeUsersParams {
    #[schemars(description = "Exact type name to search for in function bodies (e.g. ble_evt_t)")]
    pub name: String,
}

pub fn find_type(graph: &CallGraph, params: FindTypeParams) -> Result<Value, String> {
    let FindTypeParams { name } = params;
    let results = graph.find_type(&name);
    if results.is_empty() {
        return Err(format!("No types matching '{name}'"));
    }
    let matches: Vec<Value> = results
        .iter()
        .map(|t| {
            let mut obj = serde_json::Map::new();
            obj.insert("name".into(), json!(t.name));
            obj.insert("kind".into(), json!(t.kind.as_str()));
            obj.insert("file".into(), json!(t.file));
            obj.insert("line".into(), json!(t.line));
            obj.insert("definition".into(), json!(t.definition));
            if !t.conditions.is_empty() {
                obj.insert("conditions".into(), json!(t.conditions));
            }
            Value::Object(obj)
        })
        .collect();
    Ok(json!(matches))
}

pub fn get_type_users(
    graph: &CallGraph,
    root: &std::path::Path,
    params: GetTypeUsersParams,
) -> Result<Value, String> {
    let GetTypeUsersParams { name } = params;
    let results = graph.get_type_users(&name);
    if results.is_empty() {
        return Err(format!("No functions reference type '{name}'"));
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
