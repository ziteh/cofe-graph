use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::graph::{CodebaseGraph, SymbolKind};

#[derive(Deserialize, JsonSchema)]
pub struct SymbolLookupParams {
    pub file: String,
    pub kind: Option<String>,
}

const VALID_KINDS: &[&str] = &[
    "function", "variable", "typedef", "struct", "union", "enum", "macro", "enum_value",
];

pub fn symbol_lookup(graph: &CodebaseGraph, params: SymbolLookupParams) -> Value {
    let file_q = params.file.to_lowercase();
    let kind_filter = params.kind.as_deref();

    if let Some(k) = kind_filter
        && !VALID_KINDS.contains(&k)
    {
        return json!({ "error": format!("unknown kind '{}', valid: {}", k, VALID_KINDS.join("|")) });
    }

    let mut symbols: Vec<Value> = Vec::new();

    if kind_filter.is_none() || kind_filter == Some("function") {
        for nodes in graph.nodes.values() {
            for node in nodes {
                if !node.file.to_string_lossy().to_lowercase().contains(&file_q) {
                    continue;
                }
                let sig = crate::tools::extract_fn_signature(&node.source);
                symbols.push(json!({
                    "name": node.name,
                    "kind": "function",
                    "line": node.line,
                    "signature": sig,
                }));
            }
        }
    }

    if kind_filter.is_none() || kind_filter == Some("variable") {
        for vars in graph.globals.values() {
            for var in vars {
                if !var.file.to_string_lossy().to_lowercase().contains(&file_q) {
                    continue;
                }
                symbols.push(json!({
                    "name": var.name,
                    "kind": "variable",
                    "line": var.line,
                    "signature": var.decl,
                }));
            }
        }
    }

    let type_kinds: &[&str] = &["typedef", "struct", "union", "enum"];
    if kind_filter.is_none() || type_kinds.contains(&kind_filter.unwrap_or("")) {
        for nodes in graph.types.values() {
            for node in nodes {
                if !node.file.to_string_lossy().to_lowercase().contains(&file_q) {
                    continue;
                }
                let kind_str = node.kind.as_str();
                if let Some(k) = kind_filter
                    && k != kind_str
                {
                    continue;
                }
                let sig = node.definition.lines().next().unwrap_or("").trim().to_string();
                symbols.push(json!({
                    "name": node.name,
                    "kind": kind_str,
                    "line": node.line,
                    "signature": sig,
                }));
            }
        }
    }

    if kind_filter.is_none() || kind_filter == Some("macro") {
        for nodes in graph.symbols.values() {
            for node in nodes {
                if !node.file.to_string_lossy().to_lowercase().contains(&file_q) {
                    continue;
                }
                if matches!(node.kind, SymbolKind::EnumValue) {
                    continue;
                }
                let sig = macro_signature(&node.name, &node.kind, node.value.as_deref());
                symbols.push(json!({
                    "name": node.name,
                    "kind": "macro",
                    "line": node.line,
                    "signature": sig,
                }));
            }
        }
    }

    if kind_filter.is_none() || kind_filter == Some("enum_value") {
        for nodes in graph.symbols.values() {
            for node in nodes {
                if !node.file.to_string_lossy().to_lowercase().contains(&file_q) {
                    continue;
                }
                if !matches!(node.kind, SymbolKind::EnumValue) {
                    continue;
                }
                let sig = match node.value.as_deref() {
                    Some(v) => format!("{} = {}", node.name, v),
                    None => node.name.clone(),
                };
                symbols.push(json!({
                    "name": node.name,
                    "kind": "enum_value",
                    "line": node.line,
                    "signature": sig,
                }));
            }
        }
    }

    symbols.sort_by(|a, b| {
        a["line"].as_u64().unwrap_or(0).cmp(&b["line"].as_u64().unwrap_or(0))
    });

    json!({ "file": params.file, "symbols": symbols })
}

fn macro_signature(name: &str, kind: &SymbolKind, value: Option<&str>) -> String {
    match kind {
        SymbolKind::MacroFn => {
            let params = value
                .and_then(|v| {
                    let trimmed = v.trim_start();
                    if trimmed.starts_with('(') {
                        trimmed.find(')').map(|i| &trimmed[..=i])
                    } else {
                        None
                    }
                })
                .unwrap_or("(...)");
            format!("#define {}{}", name, params)
        }
        SymbolKind::Define => format!("#define {}", name),
        SymbolKind::EnumValue => unreachable!(),
    }
}
