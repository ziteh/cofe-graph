use rmcp::schemars;
use serde::Deserialize;

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

pub fn get_includes(graph: &CallGraph, params: GetIncludesParams) -> String {
    let GetIncludesParams { file } = params;
    let results = graph.get_includes(&file);
    if results.is_empty() {
        return format!("No indexed files matching '{file}'");
    }
    results
        .iter()
        .map(|(path, edges)| {
            let header = format!("{}:", path.display());
            let lines: Vec<String> = edges
                .iter()
                .map(|e| {
                    if e.is_system {
                        format!("  <{}>", e.path)
                    } else {
                        format!("  \"{}\"", e.path)
                    }
                })
                .collect();
            if lines.is_empty() {
                format!("{header}\n  (no includes)")
            } else {
                format!("{header}\n{}", lines.join("\n"))
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub fn get_includers(graph: &CallGraph, params: GetIncludersParams) -> String {
    let GetIncludersParams { header } = params;
    let results = graph.get_includers(&header);
    if results.is_empty() {
        return format!("No files include '{header}'");
    }
    results
        .iter()
        .map(|(file, edge)| {
            let include_str = if edge.is_system {
                format!("<{}>", edge.path)
            } else {
                format!("\"{}\"", edge.path)
            };
            format!("{} includes {include_str}", file.display())
        })
        .collect::<Vec<_>>()
        .join("\n")
}
