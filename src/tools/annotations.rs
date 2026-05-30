use std::path::Path;

use rmcp::schemars;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::annotations::{AnnotationStore, ModuleAnnotation};
use crate::cache::Cache;
use crate::graph::CodebaseGraph;

// ---------------------------------------------------------------------------
// Param types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AnnotateModuleParams {
    #[schemars(description = "Module name — a logical grouping label (e.g. \"BLE stack\")")]
    pub name: String,
    #[schemars(description = "Human-readable summary describing what this module does")]
    pub summary: String,
    #[schemars(
        description = "Relative file paths that belong to this module (e.g. [\"src/ble/ble_init.c\", \"src/ble/ble_adv.c\"])"
    )]
    pub files: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AnnotateParams {
    #[schemars(
        description = "Relative file path as returned by other tools (e.g. \"src/main.c\")"
    )]
    pub file: String,
    #[schemars(description = "Human-readable summary")]
    pub summary: String,
    #[schemars(
        description = "Exact symbol name (function, global variable, or #define). Omit to annotate the file itself; provide to annotate a specific symbol within the file."
    )]
    pub symbol: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetAnnotationsParams {
    #[schemars(
        description = "What to look up: \"module\" lists all module annotations; \"file\" returns the annotation for a single file; \"symbol\" returns the annotation for a single symbol within a file."
    )]
    pub kind: String,
    #[schemars(description = "Required when kind is \"file\" or \"symbol\"")]
    pub file: Option<String>,
    #[schemars(description = "Required when kind is \"symbol\"")]
    pub symbol: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListUnannotatedParams {
    #[schemars(
        description = "What to scan for missing annotations: \"file\" checks source files, \"function\" checks function nodes, \"global\" checks file-scope variables."
    )]
    pub kind: String,
    #[schemars(
        description = "Optional filename substring filter to narrow results (case-insensitive)"
    )]
    pub filename_filter: Option<String>,
}

// ---------------------------------------------------------------------------
// Write tools
// ---------------------------------------------------------------------------

/// Add or update a module-level annotation.
///
/// Module annotations are not commit-aware; they represent a conceptual
/// grouping that persists across commits.
pub fn annotate_module(
    store: &mut AnnotationStore,
    base: &Path,
    params: AnnotateModuleParams,
) -> Result<Value, String> {
    let AnnotateModuleParams {
        name,
        summary,
        files,
    } = params;
    store.upsert_module(ModuleAnnotation {
        name: name.clone(),
        summary,
        files,
    });
    store.save(base)?;
    Ok(json!({ "ok": true, "module": name }))
}

/// Add or update a file or symbol annotation.
///
/// - `symbol` absent → annotate the file (keyed by git blob SHA).
/// - `symbol` present → annotate that symbol within the file (keyed by
///   `"{blob_sha}::{symbol}"`).
///
/// Both are commit-aware: the annotation becomes invisible once the file
/// is modified on a different commit.
pub fn annotate(
    store: &mut AnnotationStore,
    base: &Path,
    params: AnnotateParams,
) -> Result<Value, String> {
    let AnnotateParams {
        file,
        summary,
        symbol,
    } = params;
    let sha = resolve_sha(base, &file)?;
    if let Some(ref sym) = symbol {
        store.upsert_symbol(&sha, sym, summary);
        store.save(base)?;
        Ok(json!({ "ok": true, "kind": "symbol", "symbol": sym, "file": file, "sha": sha }))
    } else {
        store.upsert_file(&sha, summary);
        store.save(base)?;
        Ok(json!({ "ok": true, "kind": "file", "file": file, "sha": sha }))
    }
}

// ---------------------------------------------------------------------------
// Read tools
// ---------------------------------------------------------------------------

/// Unified annotation reader.
///
/// - `kind="module"` → returns all module annotations (sorted by name).
/// - `kind="file"`   → returns the annotation for the given `file` (null if
///   none or if the file changed since the annotation was written).
/// - `kind="symbol"` → returns the annotation for `symbol` in `file`.
pub fn get_annotations(
    store: &AnnotationStore,
    graph: &CodebaseGraph,
    base: &Path,
    params: GetAnnotationsParams,
) -> Result<Value, String> {
    match params.kind.as_str() {
        "module" => {
            let mut modules: Vec<&ModuleAnnotation> = store.modules.values().collect();
            modules.sort_by_key(|m| &m.name);
            Ok(json!(modules))
        }
        "file" => {
            let file = params
                .file
                .ok_or_else(|| "\"file\" is required when kind=\"file\"".to_string())?;
            let abs = base.join(&file);
            match graph.file_shas.get(&abs) {
                None => Ok(json!({ "file": file, "annotation": null, "reason": "not in index" })),
                Some(sha) => match store.get_file_annotation(sha) {
                    Some(s) => Ok(json!({ "file": file, "annotation": s })),
                    None => Ok(json!({ "file": file, "annotation": null })),
                },
            }
        }
        "symbol" => {
            let file = params
                .file
                .ok_or_else(|| "\"file\" is required when kind=\"symbol\"".to_string())?;
            let symbol = params
                .symbol
                .ok_or_else(|| "\"symbol\" is required when kind=\"symbol\"".to_string())?;
            let abs = base.join(&file);
            match graph.file_shas.get(&abs) {
                None => Ok(json!({
                    "symbol": symbol, "file": file,
                    "annotation": null, "reason": "not in index"
                })),
                Some(sha) => match store.get_symbol_annotation(sha, &symbol) {
                    Some(s) => Ok(json!({ "symbol": symbol, "file": file, "annotation": s })),
                    None => Ok(json!({ "symbol": symbol, "file": file, "annotation": null })),
                },
            }
        }
        other => Err(format!(
            "Unknown kind '{other}' — use \"module\", \"file\", or \"symbol\""
        )),
    }
}

/// List items that have no annotation for the current index snapshot.
///
/// - `kind="file"` — source files without a file-level annotation.
/// - `kind="function"` — function nodes without a symbol annotation.
/// - `kind="global"` — global variables without a symbol annotation.
///
/// Results are sorted and filtered by `filename_filter` (case-insensitive
/// substring match on the relative file path) when provided.
pub fn list_unannotated(
    store: &AnnotationStore,
    graph: &CodebaseGraph,
    base: &Path,
    params: ListUnannotatedParams,
) -> Result<Value, String> {
    let filter = params.filename_filter.as_deref().map(str::to_lowercase);
    let filter = filter.as_deref();

    fn collect_unannotated_symbols<'a, T: 'a>(
        items: impl Iterator<Item = &'a T>,
        file_of: impl Fn(&'a T) -> &'a std::path::Path,
        name_of: impl Fn(&'a T) -> &'a str,
        graph: &CodebaseGraph,
        store: &AnnotationStore,
        base: &Path,
        filter: Option<&str>,
    ) -> Vec<Value> {
        let mut missing: Vec<Value> = items
            .filter_map(|item| {
                let rel = super::rel_file(base, file_of(item));
                if filter.is_some_and(|f| !rel.to_lowercase().contains(f)) {
                    return None;
                }
                let annotated = graph
                    .file_shas
                    .get(file_of(item))
                    .and_then(|sha| store.get_symbol_annotation(sha, name_of(item)))
                    .is_some();
                if !annotated {
                    Some(json!({ "name": name_of(item), "file": rel }))
                } else {
                    None
                }
            })
            .collect();
        missing.sort_by(|a, b| {
            let fa = a["file"].as_str().unwrap_or("");
            let fb = b["file"].as_str().unwrap_or("");
            fa.cmp(fb).then(
                a["name"]
                    .as_str()
                    .unwrap_or("")
                    .cmp(b["name"].as_str().unwrap_or("")),
            )
        });
        missing
    }

    match params.kind.as_str() {
        "file" => {
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
            Ok(json!({ "kind": "file", "count": missing.len(), "items": missing }))
        }
        "function" => {
            let missing = collect_unannotated_symbols(
                graph.nodes.values(),
                |n| n.file.as_path(),
                |n| n.name.as_str(),
                graph,
                store,
                base,
                filter,
            );
            Ok(json!({ "kind": "function", "count": missing.len(), "items": missing }))
        }
        "global" => {
            let missing = collect_unannotated_symbols(
                graph.globals.values(),
                |v| v.file.as_path(),
                |v| v.name.as_str(),
                graph,
                store,
                base,
                filter,
            );
            Ok(json!({ "kind": "global", "count": missing.len(), "items": missing }))
        }
        other => Err(format!(
            "Unknown kind '{other}' — use \"file\", \"function\", or \"global\""
        )),
    }
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

/// Resolve the git blob SHA (or blake3 content hash when git is unavailable)
/// for a project-relative file path.
fn resolve_sha(base: &Path, rel: &str) -> Result<String, String> {
    let abs = base.join(rel);
    // Try git blob map first; fall through to blake3 if not tracked.
    if let Some(blob_map) = Cache::ls_files(base)
        && let Some(sha) = blob_map.get(&abs)
    {
        return Ok(sha.clone());
    }
    // No git, or untracked file: hash the content.
    let bytes = std::fs::read(&abs).map_err(|e| format!("cannot read '{rel}': {e}"))?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}
