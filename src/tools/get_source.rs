use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::graph::{CodebaseGraph, SymbolKind};

#[derive(Deserialize, JsonSchema)]
pub struct GetSourceParams {
    pub name: String,
    pub file: Option<String>,
}

struct Match {
    kind: &'static str,
    file: String,
    line: u32,
    signature: String,
    source: String,
}

pub fn get_source(graph: &CodebaseGraph, params: GetSourceParams) -> Value {
    let mut matches: Vec<Match> = Vec::new();

    let file_q = params.file.as_deref().map(|f| f.to_lowercase());
    let file_matches = |file_str: &str| file_q.as_deref().is_none_or(|q| file_str.to_lowercase().contains(q));

    if let Some(nodes) = graph.nodes.get(&params.name) {
        for node in nodes {
            let file_str = node.file.to_string_lossy().to_string();
            if file_matches(&file_str) {
                matches.push(Match {
                    kind: "function",
                    file: file_str,
                    line: node.line,
                    signature: crate::tools::extract_fn_signature(&node.source),
                    source: node.source.clone(),
                });
            }
        }
    }

    if let Some(nodes) = graph.types.get(&params.name) {
        for node in nodes {
            let file_str = node.file.to_string_lossy().to_string();
            if file_matches(&file_str) {
                matches.push(Match {
                    kind: "typedef",
                    file: file_str,
                    line: node.line,
                    signature: node.definition.lines().next().unwrap_or("").trim().to_string(),
                    source: node.definition.clone(),
                });
            }
        }
    }

    if let Some(vars) = graph.globals.get(&params.name) {
        for var in vars {
            let file_str = var.file.to_string_lossy().to_string();
            if file_matches(&file_str) {
                matches.push(Match {
                    kind: "variable",
                    file: file_str,
                    line: var.line,
                    signature: var.decl.clone(),
                    source: var.decl.clone(),
                });
            }
        }
    }

    if let Some(nodes) = graph.symbols.get(&params.name) {
        for node in nodes {
            let file_str = node.file.to_string_lossy().to_string();
            if file_matches(&file_str) {
                let source = match (&node.kind, node.value.as_deref()) {
                    (SymbolKind::MacroFn, Some(v)) => format!("#define {}{}", node.name, v),
                    (SymbolKind::Define, Some(v)) => format!("#define {} {}", node.name, v),
                    (SymbolKind::Define, None) => format!("#define {}", node.name),
                    (SymbolKind::EnumValue, Some(v)) => format!("{} = {}", node.name, v),
                    (SymbolKind::EnumValue, None) => node.name.clone(),
                    _ => node.name.clone(),
                };
                matches.push(Match {
                    kind: "macro",
                    file: file_str,
                    line: node.line,
                    signature: source.clone(),
                    source,
                });
            }
        }
    }

    match matches.len() {
        0 => json!({ "error": format!("symbol '{}' not found", params.name) }),
        1 => {
            let m = matches.remove(0);
            Value::String(format!("// file: {}\n// line: {}\n\n{}", m.file, m.line, m.source))
        }
        _ => json!({
            "ambiguous": true,
            "matches": matches.iter().map(|m| json!({
                "name": params.name,
                "kind": m.kind,
                "file": m.file,
                "signature": m.signature,
            })).collect::<Vec<_>>()
        }),
    }
}
