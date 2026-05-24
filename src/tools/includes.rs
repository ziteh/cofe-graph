use rmcp::schemars;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::graph::CallGraph;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetIncludesParams {
    #[schemars(
        description = "Filename substring to match against indexed file paths (case-insensitive)"
    )]
    pub file: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetIncludersParams {
    #[schemars(
        description = "Header name substring to search for within #include paths (case-insensitive)"
    )]
    pub header: String,
}

pub fn get_includes(graph: &CallGraph, params: GetIncludesParams) -> Result<Value, String> {
    let GetIncludesParams { file } = params;
    let results = graph.get_includes(&file);
    if results.is_empty() {
        return Err(format!("No indexed files matching '{file}'"));
    }
    let entries: Vec<Value> = results
        .iter()
        .map(|(path, edges)| {
            let includes: Vec<_> = edges
                .iter()
                .map(|e| json!({"path": e.path, "is_system": e.is_system}))
                .collect();
            json!({"file": path, "includes": includes})
        })
        .collect();
    Ok(json!(entries))
}

pub fn get_includers(graph: &CallGraph, params: GetIncludersParams) -> Result<Value, String> {
    let GetIncludersParams { header } = params;
    let results = graph.get_includers(&header);
    if results.is_empty() {
        return Err(format!("No files include '{header}'"));
    }
    let includers: Vec<Value> = results
        .iter()
        .map(|(file, edge)| {
            json!({
                "file": file,
                "path": edge.path,
                "is_system": edge.is_system,
            })
        })
        .collect();
    Ok(json!(includers))
}
