use serde_json::{Value, json};

use crate::graph::{CallGraph, DeadCodeKind, FunctionNode};

pub fn find_dead_code(graph: &CallGraph) -> Result<Value, String> {
    let mut dead = graph.find_dead_code();
    dead.sort_by_key(|(n, _)| &n.name);

    let fmt = |(n, k): &&(&FunctionNode, DeadCodeKind)| {
        json!({
            "kind": k.as_str(),
            "name": n.name,
            "file": n.file,
            "line": n.line,
        })
    };

    let suspicious: Vec<_> = dead
        .iter()
        .filter(|(_, k)| *k == DeadCodeKind::Suspicious)
        .collect();
    let macro_reg: Vec<_> = dead
        .iter()
        .filter(|(_, k)| *k == DeadCodeKind::MacroRegistered)
        .collect();
    let cb_name: Vec<_> = dead
        .iter()
        .filter(|(_, k)| *k == DeadCodeKind::CallbackByName)
        .collect();
    let entry: Vec<_> = dead
        .iter()
        .filter(|(_, k)| *k == DeadCodeKind::Entrypoint)
        .collect();

    Ok(json!({
        "summary": {
            "total": dead.len(),
            "suspicious": suspicious.len(),
            "macro_registered": macro_reg.len(),
            "callback_by_name": cb_name.len(),
            "entrypoint": entry.len(),
        },
        "suspicious": suspicious.iter().map(fmt).collect::<Vec<_>>(),
        "macro_registered": macro_reg.iter().map(fmt).collect::<Vec<_>>(),
        "callback_by_name": cb_name.iter().map(fmt).collect::<Vec<_>>(),
        "entrypoints": entry.iter().map(fmt).collect::<Vec<_>>(),
    }))
}

pub fn get_stats(graph: &CallGraph) -> Result<Value, String> {
    let fn_count = graph.nodes.len();
    let edge_count: usize = graph.callees.values().map(|s| s.len()).sum();
    let dead = graph.find_dead_code();
    let dead_count = dead.len();
    let true_dead_count = dead
        .iter()
        .filter(|(_, k)| *k == DeadCodeKind::Suspicious)
        .count();

    let top_fan_in: Vec<_> = graph
        .top_by_fan_in(5)
        .iter()
        .map(|(name, count)| json!({"name": name, "callers": count}))
        .collect();
    let top_fan_out: Vec<_> = graph
        .top_by_fan_out(5)
        .iter()
        .map(|(name, count)| json!({"name": name, "callees": count}))
        .collect();

    Ok(json!({
        "functions": fn_count,
        "call_edges": edge_count,
        "dead_code": {
            "total": dead_count,
            "suspicious": true_dead_count,
        },
        "top_fan_in": top_fan_in,
        "top_fan_out": top_fan_out,
    }))
}
