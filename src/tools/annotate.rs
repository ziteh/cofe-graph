use std::collections::HashSet;

use rmcp::schemars;
use serde::Deserialize;
use serde_json::json;

use crate::annotations::{AnnotationStore, FileAnnotation};
use crate::graph::CallGraph;

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
    #[schemars(description = "Names of key/important functions in this file")]
    pub key_functions: Vec<String>,
    #[schemars(description = "Optional additional notes or observations")]
    pub notes: Option<String>,
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

/// BLAKE3 of sorted function sources for a specific file, first 16 hex chars.
fn compute_hash(graph: &CallGraph, file_path: &str) -> String {
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
fn all_files(graph: &CallGraph) -> Vec<String> {
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
fn match_one_file(graph: &CallGraph, substr: &str) -> Result<String, String> {
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

/// Write a semantic annotation for a file. The current file hash is computed from
/// the graph and stored alongside the annotation for staleness detection.
pub fn annotate_file(
    graph: &CallGraph,
    store: &mut AnnotationStore,
    p: AnnotateFileParams,
) -> String {
    let file = match match_one_file(graph, &p.path) {
        Ok(f) => f,
        Err(e) => return json!({"content": {"error": e}, "isError": true}).to_string(),
    };
    let file_hash = compute_hash(graph, &file);
    store.files.insert(
        file.clone(),
        FileAnnotation {
            subsystem: p.subsystem,
            summary: p.summary,
            key_functions: p.key_functions,
            notes: p.notes,
            file_hash,
        },
    );
    store.save();
    json!({"content": {"ok": true, "file": file}, "isError": false}).to_string()
}

/// Retrieve the annotation for files matching a path substring.
/// Each result includes a `stale` flag indicating whether the file has changed
/// since the annotation was written.
pub fn get_file_annotation(
    graph: &CallGraph,
    store: &AnnotationStore,
    p: FilePathParams,
) -> String {
    let q = p.path.to_lowercase();
    let mut matches: Vec<_> = store
        .files
        .iter()
        .filter(|(k, _)| k.to_lowercase().contains(&q))
        .collect();
    if matches.is_empty() {
        return json!({"content": {"found": false}, "isError": false}).to_string();
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
                "key_functions": ann.key_functions,
                "notes": ann.notes,
                "stale": current_hash != ann.file_hash,
            })
        })
        .collect();
    json!({"content": {"found": true, "results": results}, "isError": false}).to_string()
}

/// List all annotated files with their subsystem and staleness status.
pub fn list_file_annotations(graph: &CallGraph, store: &AnnotationStore) -> String {
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
    json!({"content": {"count": results.len(), "files": results}, "isError": false}).to_string()
}

/// List indexed files that have no annotation yet, ordered by function count
/// (most functions first — highest priority targets for annotation).
pub fn list_unannotated_files(graph: &CallGraph, store: &AnnotationStore) -> String {
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
    json!({"content": {"count": results.len(), "files": results}, "isError": false}).to_string()
}

/// Return a rich analysis bundle for a single file: all functions with call
/// statistics and source, globals, types, and any existing annotation.
/// Designed to give an LLM everything it needs to annotate the file in one call.
pub fn get_file_context(
    graph: &CallGraph,
    store: &AnnotationStore,
    p: FileContextParams,
) -> String {
    let file = match match_one_file(graph, &p.filename) {
        Ok(f) => f,
        Err(e) => return json!({"content": {"error": e}, "isError": true}).to_string(),
    };

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
                "is_static": n.is_static,
                "conditions": n.conditions,
                "caller_count": caller_count,
                "callee_count": callee_count,
                "callees": callees,
                "source": n.source,
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
                "is_static": g.is_static,
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
            "key_functions": ann.key_functions,
            "notes": ann.notes,
            "stale": current_hash != ann.file_hash,
        })
    });

    json!({
        "content": {
            "file": file,
            "function_count": functions.len(),
            "functions": functions,
            "globals": globals,
            "types": types,
            "existing_annotation": annotation,
        },
        "isError": false,
    })
    .to_string()
}
