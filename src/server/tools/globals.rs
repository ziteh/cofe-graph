use super::fmt_fn;
use super::functions::GetSourceParams;
use super::includes::GetIncludesParams;
use super::types::GetTypeUsersParams;
use crate::graph::CallGraph;

pub fn get_globals(graph: &CallGraph, params: GetIncludesParams) -> String {
    let GetIncludesParams { file } = params;
    let results = graph.find_globals(&file);
    if results.is_empty() {
        return format!("No global variables found in files matching '{file}'");
    }
    results
        .iter()
        .map(|v| {
            let vis = if v.is_static { " [static]" } else { "" };
            format!(
                "{}{} @ {}:{}\n  {}",
                v.name,
                vis,
                v.file.display(),
                v.line,
                v.decl
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn get_global_users(graph: &CallGraph, params: GetTypeUsersParams) -> String {
    let GetTypeUsersParams { name } = params;
    let results = graph.get_global_users(&name);
    if results.is_empty() {
        return format!("No functions reference global '{name}'");
    }
    results
        .iter()
        .map(|n| fmt_fn(n))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn get_fn_globals(graph: &CallGraph, params: GetSourceParams) -> String {
    let GetSourceParams { name } = params;
    let results = graph.get_fn_globals(&name);
    if results.is_empty() {
        if graph.nodes.contains_key(&name) {
            return format!("'{name}' does not reference any indexed global variables");
        }
        return format!("Function '{name}' not found");
    }
    results
        .iter()
        .map(|v| {
            let vis = if v.is_static { " [static]" } else { "" };
            format!("{}{} @ {}:{}", v.name, vis, v.file.display(), v.line)
        })
        .collect::<Vec<_>>()
        .join("\n")
}
