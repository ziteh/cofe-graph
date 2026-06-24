use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::graph::{CodebaseGraph, FunctionNode};
use crate::tools::extract_fn_signature;

#[derive(Deserialize, JsonSchema)]
pub struct QueryCallGraphParams {
    pub name: String,
    pub file: Option<String>,
    pub direction: String,
    pub depth: Option<u32>,
}

pub fn query_call_graph(graph: &CodebaseGraph, params: QueryCallGraphParams) -> Value {
    let depth = params.depth.unwrap_or(1) as usize;

    let candidates: Vec<&FunctionNode> = match graph.nodes.get(&params.name) {
        Some(v) => v
            .iter()
            .filter(|n| {
                params.file.as_deref().is_none_or(|f| {
                    n.file.to_string_lossy().to_lowercase().contains(&f.to_lowercase())
                })
            })
            .collect(),
        None => return json!({ "error": format!("function '{}' not found", params.name) }),
    };

    let node = match candidates.len() {
        0 => {
            return json!({
                "error": format!(
                    "function '{}' not found in files matching '{}'",
                    params.name,
                    params.file.as_deref().unwrap_or("")
                )
            })
        }
        1 => candidates[0],
        _ => {
            return json!({
                "ambiguous": true,
                "matches": candidates.iter().map(|n| json!({
                    "name": n.name,
                    "kind": "function",
                    "file": n.file,
                    "signature": extract_fn_signature(&n.source),
                })).collect::<Vec<_>>()
            })
        }
    };

    let file_str = node.file.to_string_lossy().to_string();
    let mut result = json!({
        "name": params.name,
        "file": file_str,
    });

    let include_callers = params.direction == "callers" || params.direction == "both";
    let include_callees = params.direction == "callees" || params.direction == "both";

    if include_callers {
        let entries = build_entries(graph, &params.name, depth, true);
        result["callers"] = json!(entries);
    }

    if include_callees {
        let entries = build_entries(graph, &params.name, depth, false);
        result["callees"] = json!(entries);
    }

    result
}

fn build_entries(graph: &CodebaseGraph, name: &str, depth: usize, callers: bool) -> Vec<Value> {
    if depth == 1 {
        let map = if callers { &graph.callers } else { &graph.callees };
        let edges = match map.get(name) {
            Some(e) => e,
            None => return vec![],
        };

        edges.iter().map(|edge| {
            let file = graph.nodes.get(&edge.name)
                .and_then(|v| v.first())
                .map(|n| n.file.to_string_lossy().to_string())
                .unwrap_or_default();
            json!({ "name": edge.name, "file": file, "line": edge.line })
        }).collect()
    } else {
        let names = if callers {
            graph.get_callers(name, depth)
        } else {
            graph.get_callees(name, depth)
        };

        names.iter().map(|n| {
            let file = graph.nodes.get(n.as_str())
                .and_then(|v| v.first())
                .map(|node| node.file.to_string_lossy().to_string())
                .unwrap_or_default();
            json!({ "name": n, "file": file })
        }).collect()
    }
}
