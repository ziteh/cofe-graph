use std::path::PathBuf;

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
        let entries = build_entries(graph, node, depth, true);
        result["callers"] = json!(entries);
    }

    if include_callees {
        let entries = build_entries(graph, node, depth, false);
        result["callees"] = json!(entries);
    }

    result
}

fn build_entries(graph: &CodebaseGraph, node: &FunctionNode, depth: usize, is_callers: bool) -> Vec<Value> {
    let entries: Vec<(String, PathBuf)> = if is_callers {
        graph.get_callers_from(&node.name, &node.file, depth)
    } else {
        graph.get_callees_from(&node.name, &node.file, depth)
    };

    entries.iter().map(|(name, file)| {
        let line = if depth == 1 {
            let map = if is_callers { &graph.callers } else { &graph.callees };
            let search_file = if is_callers { file.as_path() } else { node.file.as_path() };
            map.get(&node.name)
                .and_then(|edges| edges.iter()
                    .find(|e| e.name == *name && e.caller_file == search_file)
                    .map(|e| e.line))
        } else {
            None
        };

        match line {
            Some(l) => json!({ "name": name, "file": file, "line": l }),
            None    => json!({ "name": name, "file": file }),
        }
    }).collect()
}
