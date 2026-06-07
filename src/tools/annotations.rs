use std::path::Path;

use rmcp::schemars;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::annotations::AnnotationStore;
use crate::graph::CodebaseGraph;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AnnotateParams {
    #[schemars(
        description = "Relative file path as returned by other tools (e.g. \"src/main.c\")"
    )]
    pub file: String,
    #[schemars(description = "Human-readable summary of what this file does")]
    pub summary: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetAnnotationParams {
    #[schemars(
        description = "Relative file path as returned by other tools (e.g. \"src/main.c\")"
    )]
    pub file: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListUnannotatedFilesParams {
    #[schemars(
        description = "Optional filename substring filter to narrow results (case-insensitive)"
    )]
    pub filename_filter: Option<String>,
}

/// Add or update a file-level annotation.
///
/// Keyed by blake3 content hash, so the annotation becomes invisible once
/// the file changes.
pub fn annotate(
    store: &mut AnnotationStore,
    base: &Path,
    params: AnnotateParams,
) -> Result<Value, String> {
    let AnnotateParams { file, summary } = params;
    let sha = resolve_sha(base, &file)?;
    store.upsert_file(&sha, summary);
    store.save(base)?;
    Ok(json!({ "ok": true, "file": file, "sha": sha }))
}

/// Return the annotation for a file, or null if none exists or the file
/// has changed since the annotation was written.
pub fn get_annotation(
    store: &AnnotationStore,
    graph: &CodebaseGraph,
    base: &Path,
    params: GetAnnotationParams,
) -> Result<Value, String> {
    let GetAnnotationParams { file } = params;
    let abs = base.join(&file);
    match graph.file_shas.get(&abs) {
        None => Ok(json!({ "file": file, "annotation": null, "reason": "not in index" })),
        Some(sha) => match store.get_file_annotation(sha) {
            Some(s) => Ok(json!({ "file": file, "annotation": s })),
            None => Ok(json!({ "file": file, "annotation": null })),
        },
    }
}

/// List source files that have no annotation in the current index snapshot.
///
/// Results are sorted and filtered by `filename_filter` (case-insensitive
/// substring match) when provided.
pub fn list_unannotated_files(
    store: &AnnotationStore,
    graph: &CodebaseGraph,
    base: &Path,
    params: ListUnannotatedFilesParams,
) -> Result<Value, String> {
    let filter = params.filename_filter.as_deref().map(str::to_lowercase);
    let filter = filter.as_deref();

    let mut missing: Vec<String> = graph
        .file_shas
        .iter()
        .filter_map(|(abs, sha)| {
            let rel = super::rel_file(base, abs);
            if filter.is_some_and(|f| !rel.to_lowercase().contains(f)) {
                return None;
            }
            if store.get_file_annotation(sha).is_none() {
                Some(rel)
            } else {
                None
            }
        })
        .collect();
    missing.sort();
    Ok(json!({ "count": missing.len(), "items": missing }))
}

fn resolve_sha(base: &Path, rel: &str) -> Result<String, String> {
    let abs = base.join(rel);
    let bytes = std::fs::read(&abs).map_err(|e| format!("cannot read '{rel}': {e}"))?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}
