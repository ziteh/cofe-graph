use rmcp::schemars;
use serde::Deserialize;

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

pub fn find_type(graph: &CallGraph, params: FindTypeParams) -> String {
    let FindTypeParams { name } = params;
    let results = graph.find_type(&name);
    if results.is_empty() {
        return format!("No types matching '{name}'");
    }
    results
        .iter()
        .map(|t| {
            format!(
                "[{}] {} @ {}:{}\n{}",
                t.kind.as_str(),
                t.name,
                t.file.display(),
                t.line,
                t.definition
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub fn get_type_users(graph: &CallGraph, params: GetTypeUsersParams) -> String {
    let GetTypeUsersParams { name } = params;
    let results = graph.get_type_users(&name);
    if results.is_empty() {
        return format!("No functions reference type '{name}'");
    }
    results
        .iter()
        .map(|n| format!("{} @ {}:{}", n.name, n.file.display(), n.line))
        .collect::<Vec<_>>()
        .join("\n")
}
