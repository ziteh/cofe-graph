use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use walkdir::WalkDir;

use serde_json::{Value, json};

use crate::cache::Cache;
use crate::graph::CallGraph;

/// Parse `(path, source)` pairs into the graph.
/// Returns `(files_ok, files_err)`.
fn parse_sources_into_graph(g: &mut CallGraph, sources: &[(&Path, &str)]) -> (usize, usize) {
    let mut files_ok = 0usize;
    let mut files_err = 0usize;
    let mut raw_sources: Vec<&str> = Vec::new();

    for &(path, src) in sources {
        match crate::parser::parse_file(path, src, g) {
            Ok(_) => {
                files_ok += 1;
                raw_sources.push(src);
            }
            Err(e) => {
                tracing::warn!("parse error {:?}: {e}", path);
                files_err += 1;
            }
        }
    }

    for src in &raw_sources {
        if let Err(e) = crate::parser::scan_macro_refs(src, g) {
            tracing::warn!("macro scan error: {e}");
        }
    }

    (files_ok, files_err)
}

/// Index from in-memory sources.
pub async fn index_sources(
    graph: Arc<RwLock<CallGraph>>,
    sources: &[(&Path, &str)],
) -> Result<Value, String> {
    let mut g = graph.write().await;
    g.clear();

    let (files_ok, files_err) = parse_sources_into_graph(&mut g, sources);

    let fn_count = g.nodes.len();
    let edge_count: usize = g.callees.values().map(|s| s.len()).sum();

    Ok(json!({
        "status": "indexed",
        "files_ok": files_ok,
        "files_err": files_err,
        "functions": fn_count,
        "call_edges": edge_count,
        "cached": false,
    }))
}

/// Index from filesystem path.
pub async fn index_project(
    graph: Arc<RwLock<CallGraph>>,
    path: &Path,
    max_cache_entries: usize,
) -> Result<Value, String> {
    let cache = Cache::open(path, max_cache_entries);

    if let Some(ref c) = cache
        && let Some(cached) = c.load()
    {
        let fn_count = cached.nodes.len();
        let edge_count: usize = cached.callees.values().map(|s| s.len()).sum();
        *graph.write().await = cached;
        c.record_hit();
        return Ok(json!({
            "status": "cached",
            "commit": c.commit_hash,
            "functions": fn_count,
            "call_edges": edge_count,
        }));
    }

    let mut file_contents: Vec<(PathBuf, String)> = Vec::new();
    let mut read_err = 0usize;

    for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("");
        if ext != "c" && ext != "h" {
            continue;
        }
        match std::fs::read_to_string(p) {
            Ok(src) => file_contents.push((p.to_path_buf(), src)),
            Err(e) => {
                tracing::warn!("read error {:?}: {e}", p);
                read_err += 1;
            }
        }
    }

    let pairs: Vec<(&Path, &str)> = file_contents
        .iter()
        .map(|(p, s)| (p.as_path(), s.as_str()))
        .collect();

    let mut g = graph.write().await;
    g.clear();

    let (files_ok, parse_err) = parse_sources_into_graph(&mut g, &pairs);

    let cached = if let Some(ref c) = cache {
        c.save(&g);
        true
    } else {
        false
    };

    let fn_count = g.nodes.len();
    let edge_count: usize = g.callees.values().map(|s| s.len()).sum();

    Ok(json!({
        "status": "indexed",
        "files_ok": files_ok,
        "files_err": read_err + parse_err,
        "functions": fn_count,
        "call_edges": edge_count,
        "cached": cached,
    }))
}
