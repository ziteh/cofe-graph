use crate::graph::{CallGraph, DeadCodeKind, FunctionNode};

pub fn find_dead_code(graph: &CallGraph) -> String {
    let mut dead = graph.find_dead_code();
    dead.sort_by_key(|(n, _)| &n.name);
    if dead.is_empty() {
        return "No dead code found (all functions have at least one caller)".to_string();
    }

    let fmt = |(n, k): &&(&FunctionNode, DeadCodeKind)| {
        format!(
            "[{}] {} @ {}:{}",
            k.as_str(),
            n.name,
            n.file.display(),
            n.line
        )
    };

    let true_dead: Vec<_> = dead
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

    let mut sections: Vec<String> = Vec::new();

    if !true_dead.is_empty() {
        sections.push(format!(
            "=== Suspicious (no evidence of use) ({}) ===\n{}",
            true_dead.len(),
            true_dead.iter().map(fmt).collect::<Vec<_>>().join("\n")
        ));
    } else {
        sections.push("=== Suspicious (no evidence of use) (0) ===\n(none)".to_string());
    }
    if !macro_reg.is_empty() {
        sections.push(format!(
            "=== Registered via macro ({}) ===\n{}",
            macro_reg.len(),
            macro_reg.iter().map(fmt).collect::<Vec<_>>().join("\n")
        ));
    }
    if !cb_name.is_empty() {
        sections.push(format!(
            "=== Likely callbacks by name ({}) ===\n{}",
            cb_name.len(),
            cb_name.iter().map(fmt).collect::<Vec<_>>().join("\n")
        ));
    }
    if !entry.is_empty() {
        sections.push(format!(
            "=== Entrypoints ({}) ===\n{}",
            entry.len(),
            entry.iter().map(fmt).collect::<Vec<_>>().join("\n")
        ));
    }

    sections.join("\n\n")
}

pub fn get_stats(graph: &CallGraph) -> String {
    let fn_count = graph.nodes.len();
    let edge_count: usize = graph.callees.values().map(|s| s.len()).sum();
    let dead = graph.find_dead_code();
    let dead_count = dead.len();
    let true_dead_count = dead
        .iter()
        .filter(|(_, k)| *k == DeadCodeKind::Suspicious)
        .count();

    let fan_in_lines: Vec<String> = graph
        .top_by_fan_in(5)
        .iter()
        .map(|(name, count)| format!("  {name} ({count} callers)"))
        .collect();
    let fan_out_lines: Vec<String> = graph
        .top_by_fan_out(5)
        .iter()
        .map(|(name, count)| format!("  {name} ({count} callees)"))
        .collect();

    format!(
        "Functions : {fn_count}\nCall edges: {edge_count}\nDead code : {dead_count} (no callers, {true_dead_count} true dead)\n\nTop fan-in (most callers):\n{}\n\nTop fan-out (most callees):\n{}",
        fan_in_lines.join("\n"),
        fan_out_lines.join("\n"),
    )
}
