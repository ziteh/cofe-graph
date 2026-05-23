use rmcp::schemars;
use serde::Deserialize;

use crate::graph::CallGraph;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FindSymbolParams {
    #[schemars(
        description = "Symbol name substring to search for (case-insensitive). Matches #define constants, function-like macros, and enum values."
    )]
    pub name: String,
}

pub fn find_symbol(graph: &CallGraph, params: FindSymbolParams) -> String {
    let FindSymbolParams { name } = params;
    let results = graph.find_symbol(&name);
    if results.is_empty() {
        return format!("No symbols matching '{name}'");
    }
    results
        .iter()
        .map(|s| {
            let value_part = match &s.value {
                Some(v) => format!(" = {v}"),
                None => String::new(),
            };
            format!(
                "[{}] {}{}  @ {}:{}",
                s.kind.as_str(),
                s.name,
                value_part,
                s.file.display(),
                s.line,
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
