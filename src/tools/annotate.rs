use std::collections::HashSet;

use rmcp::schemars;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::annotations::{AnnotationStore, FileAnnotation};
use crate::graph::CodebaseGraph;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AnnotateFileParams {
    #[schemars(
        description = "File path substring — must match exactly one indexed file (case-insensitive)"
    )]
    pub path: String,
    #[schemars(description = "Subsystem or module this file belongs to")]
    pub subsystem: String,
    #[schemars(description = "Description of what this file does")]
    pub summary: String,
    #[schemars(
        description = "Free-form notes: interrupt context, cross-file dependencies, design decisions"
    )]
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AnnotateSymbolParams {
    #[schemars(description = "Exact function, global variable, or type name to annotate")]
    pub name: String,
    #[schemars(
        description = "Free-form semantic insight — interrupt context, ownership rules, invariants, design intent"
    )]
    pub insight: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FilePathParams {
    #[schemars(description = "File path substring to search for (case-insensitive)")]
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FileContextParams {
    #[schemars(
        description = "Filename substring — must match exactly one indexed file (case-insensitive)"
    )]
    pub filename: String,
}

/// BLAKE3 hash of a symbol's source/definition text (first 16 hex chars).
fn compute_symbol_hash(graph: &CodebaseGraph, name: &str) -> Option<String> {
    if let Some(n) = graph.nodes.get(name) {
        let hash = blake3::hash(n.source.as_bytes());
        return Some(hash.to_hex()[..16].to_string());
    }
    if let Some(nodes) = graph.types.get(name) {
        let mut hasher = blake3::Hasher::new();
        for t in nodes {
            hasher.update(t.definition.as_bytes());
        }
        return Some(hasher.finalize().to_hex()[..16].to_string());
    }
    if let Some(g) = graph.globals.get(name) {
        let hash = blake3::hash(g.decl.as_bytes());
        return Some(hash.to_hex()[..16].to_string());
    }
    None
}

/// BLAKE3 of sorted function sources for a specific file, first 16 hex chars.
fn compute_hash(graph: &CodebaseGraph, file_path: &str) -> String {
    let mut sources: Vec<&str> = graph
        .nodes
        .values()
        .filter(|n| n.file.to_string_lossy() == file_path)
        .map(|n| n.source.as_str())
        .collect();
    sources.sort_unstable();
    let mut hasher = blake3::Hasher::new();
    for src in sources {
        hasher.update(src.as_bytes());
    }
    hasher.finalize().to_hex()[..16].to_string()
}

/// All unique file paths that have at least one function node, sorted.
fn all_files(graph: &CodebaseGraph) -> Vec<String> {
    let set: HashSet<String> = graph
        .nodes
        .values()
        .map(|n| n.file.to_string_lossy().to_string())
        .collect();
    let mut v: Vec<String> = set.into_iter().collect();
    v.sort();
    v
}

/// Returns the single file path matching `substr`, or an error string.
fn match_one_file(graph: &CodebaseGraph, substr: &str) -> Result<String, String> {
    let q = substr.to_lowercase();
    let matches: Vec<String> = all_files(graph)
        .into_iter()
        .filter(|f| f.to_lowercase().contains(&q))
        .collect();
    match matches.len() {
        0 => Err(format!("no indexed files match '{substr}'")),
        1 => Ok(matches.into_iter().next().unwrap()),
        n => Err(format!(
            "ambiguous: {n} files match '{substr}': {}",
            matches.join(", ")
        )),
    }
}

pub fn annotate_file(
    graph: &CodebaseGraph,
    store: &mut AnnotationStore,
    p: AnnotateFileParams,
) -> Result<Value, String> {
    let file = match_one_file(graph, &p.path)?;
    let file_hash = compute_hash(graph, &file);
    store.files.insert(
        file.clone(),
        FileAnnotation {
            subsystem: p.subsystem,
            summary: p.summary,
            notes: p.notes,
            file_hash,
        },
    );
    store.save();
    Ok(json!({"ok": true, "file": file}))
}

pub fn annotate_symbol(
    graph: &CodebaseGraph,
    store: &mut AnnotationStore,
    p: AnnotateSymbolParams,
) -> Result<Value, String> {
    let source_hash = compute_symbol_hash(graph, &p.name).ok_or_else(|| {
        format!(
            "No function, type, or global named '{}' found in index",
            p.name
        )
    })?;
    store.symbols.entry(p.name.clone()).or_default().insert(
        source_hash,
        crate::annotations::SymbolAnnotation { insight: p.insight },
    );
    store.save();
    Ok(json!({"ok": true, "symbol": p.name}))
}

pub fn get_file_annotation(
    graph: &CodebaseGraph,
    store: &AnnotationStore,
    p: FilePathParams,
) -> Result<Value, String> {
    let q = p.path.to_lowercase();
    let mut matches: Vec<_> = store
        .files
        .iter()
        .filter(|(k, _)| k.to_lowercase().contains(&q))
        .collect();
    if matches.is_empty() {
        return Ok(json!({"found": false}));
    }
    matches.sort_by_key(|(k, _)| k.as_str());
    let results: Vec<_> = matches
        .into_iter()
        .map(|(file, ann)| {
            let current_hash = compute_hash(graph, file);
            json!({
                "file": file,
                "subsystem": ann.subsystem,
                "summary": ann.summary,
                "notes": ann.notes,
                "stale": current_hash != ann.file_hash,
            })
        })
        .collect();
    Ok(json!({"found": true, "results": results}))
}

pub fn list_file_annotations(
    graph: &CodebaseGraph,
    store: &AnnotationStore,
) -> Result<Value, String> {
    let mut entries: Vec<_> = store
        .files
        .iter()
        .map(|(file, ann)| {
            let current_hash = compute_hash(graph, file);
            (file, ann, current_hash != ann.file_hash)
        })
        .collect();
    entries.sort_by_key(|(f, _, _)| f.as_str());
    let results: Vec<_> = entries
        .into_iter()
        .map(|(file, ann, stale)| {
            json!({
                "file": file,
                "subsystem": ann.subsystem,
                "stale": stale,
            })
        })
        .collect();
    Ok(json!({"count": results.len(), "files": results}))
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListUnannotatedParams {
    #[schemars(
        description = "File path substring — omit to list unannotated files; provide to list unannotated functions within that file"
    )]
    pub file: Option<String>,
}

pub fn list_unannotated(
    graph: &CodebaseGraph,
    store: &AnnotationStore,
    p: ListUnannotatedParams,
) -> Result<Value, String> {
    match p.file {
        None => {
            let files = all_files(graph);
            let mut unannotated: Vec<(&String, usize)> = files
                .iter()
                .filter(|f| !store.files.contains_key(*f))
                .map(|f| {
                    let fn_count = graph
                        .nodes
                        .values()
                        .filter(|n| n.file.to_string_lossy() == f.as_str())
                        .count();
                    (f, fn_count)
                })
                .collect();
            unannotated.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
            let results: Vec<_> = unannotated
                .into_iter()
                .map(|(f, fn_count)| json!({"file": f, "function_count": fn_count}))
                .collect();
            Ok(json!({"count": results.len(), "files": results}))
        }
        Some(file_substr) => {
            let file = match_one_file(graph, &file_substr)?;
            let mut fns: Vec<_> = graph
                .nodes
                .values()
                .filter(|n| n.file.to_string_lossy() == file.as_str())
                .filter(|n| store.get_symbol(&n.name, &n.source).is_none())
                .collect();
            fns.sort_by_key(|n| n.line);
            let results: Vec<_> = fns
                .iter()
                .map(|n| json!({"name": n.name, "line": n.line}))
                .collect();
            Ok(json!({"file": file, "count": results.len(), "functions": results}))
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetAnnotationsParams {
    #[schemars(
        description = "File path substring to filter by — omit (or pass null) to list all annotated files"
    )]
    pub path: Option<String>,
}

pub fn get_annotations(
    graph: &CodebaseGraph,
    store: &AnnotationStore,
    p: GetAnnotationsParams,
) -> Result<Value, String> {
    match p.path {
        Some(path) => get_file_annotation(graph, store, FilePathParams { path }),
        None => list_file_annotations(graph, store),
    }
}

pub fn get_file_context(
    graph: &CodebaseGraph,
    store: &AnnotationStore,
    p: FileContextParams,
) -> Result<Value, String> {
    let file = match_one_file(graph, &p.filename)?;

    let mut fns: Vec<_> = graph
        .nodes
        .values()
        .filter(|n| n.file.to_string_lossy() == file.as_str())
        .collect();
    fns.sort_by_key(|n| n.line);

    let functions: Vec<_> = fns
        .iter()
        .map(|n| {
            let caller_count = graph.callers.get(&n.name).map_or(0, |s| s.len());
            let callee_count = graph.callees.get(&n.name).map_or(0, |s| s.len());
            let mut callees: Vec<_> = graph
                .callees
                .get(&n.name)
                .map(|s| s.iter().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            callees.sort();
            json!({
                "name": n.name,
                "line": n.line,
                "static": n.is_static,
                "conditions": n.conditions,
                "caller_count": caller_count,
                "callee_count": callee_count,
                "callees": callees,
                "annotation": store.get_symbol(&n.name, &n.source).map(|s| s.insight.as_str()),
            })
        })
        .collect();

    let mut globals: Vec<_> = graph
        .globals
        .values()
        .filter(|g| g.file.to_string_lossy() == file.as_str())
        .map(|g| {
            json!({
                "name": g.name,
                "decl": g.decl,
                "static": g.is_static,
                "conditions": g.conditions,
            })
        })
        .collect();
    globals.sort_by_key(|v| v["name"].as_str().unwrap_or("").to_owned());

    let mut types: Vec<_> = graph
        .types
        .values()
        .flat_map(|v| v.iter())
        .filter(|t| t.file.to_string_lossy() == file.as_str())
        .map(|t| {
            let def_preview = &t.definition[..t.definition.len().min(200)];
            json!({"name": t.name, "kind": t.kind.as_str(), "definition": def_preview})
        })
        .collect();
    types.sort_by_key(|v| v["name"].as_str().unwrap_or("").to_owned());

    let annotation = store.files.get(&file).map(|ann| {
        let current_hash = compute_hash(graph, &file);
        json!({
            "subsystem": ann.subsystem,
            "summary": ann.summary,
            "notes": ann.notes,
            "stale": current_hash != ann.file_hash,
        })
    });

    Ok(json!({
        "file": file,
        "function_count": functions.len(),
        "functions": functions,
        "globals": globals,
        "types": types,
        "existing_annotation": annotation,
    }))
}
