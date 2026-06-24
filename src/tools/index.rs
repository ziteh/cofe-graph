use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use walkdir::WalkDir;

use serde_json::{Value, json};

use crate::cache::Cache;
use crate::graph::{CodebaseGraph, FileGraph};

/// Parse `(path, source)` pairs into the graph.
/// Returns `(files_ok, files_err)`.
fn parse_sources_into_graph(g: &mut CodebaseGraph, sources: &[(&Path, &str)]) -> (usize, usize) {
    let start = std::time::Instant::now();

    let results: Vec<(&Path, anyhow::Result<CodebaseGraph>)> = sources
        .par_iter()
        .map(|&(path, src)| {
            let mut local = CodebaseGraph::default();
            let res = crate::parser::parse_file(path, src, &mut local).map(|_| local);
            (path, res)
        })
        .collect();

    let mut files_ok = 0usize;
    let mut files_err = 0usize;

    for (path, res) in results {
        match res {
            Ok(local) => {
                g.merge(local);
                files_ok += 1;
            }
            Err(e) => {
                tracing::warn!("parse error {:?}: {e}", path);
                files_err += 1;
            }
        }
    }

    tracing::info!(
        files_ok,
        files_err,
        functions = g.nodes.values().map(|v| v.len()).sum::<usize>(),
        total_ms = start.elapsed().as_millis(),
        "parse_sources_into_graph completed"
    );

    (files_ok, files_err)
}

/// Index from in-memory sources.
pub async fn index_sources(
    graph: Arc<RwLock<CodebaseGraph>>,
    sources: &[(&Path, &str)],
) -> Result<Value, String> {
    let mut g = graph.write().await;
    g.clear();

    let (files_ok, files_err) = parse_sources_into_graph(&mut g, sources);

    let fn_count: usize = g.nodes.values().map(|v| v.len()).sum();
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
    graph: Arc<RwLock<CodebaseGraph>>,
    path: &Path,
    cache_overhead_pct: usize,
) -> Result<Value, String> {
    let cache = Cache::open(path);

    let start = std::time::Instant::now();

    let all_files: Vec<PathBuf> = WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let ext = e.path().extension().and_then(|x| x.to_str()).unwrap_or("");
            ext == "c" || ext == "h"
        })
        .map(|e| e.into_path())
        .collect();

    let max_cache_entries = all_files.len() * (100 + cache_overhead_pct) / 100;

    let mut file_graphs: Vec<FileGraph> = Vec::new();
    let mut to_parse: Vec<(PathBuf, String, String)> = Vec::new();
    let mut read_err = 0usize;
    let mut l2_hits = 0usize;

    for file_path in &all_files {
        match std::fs::read(file_path) {
            Ok(bytes) => {
                let key = blake3::hash(&bytes).to_hex().to_string();

                if let Some(ref c) = cache
                    && let Some(fg) = c.load_file_graph(&key)
                {
                    file_graphs.push(fg);
                    l2_hits += 1;
                    continue;
                }

                match String::from_utf8(bytes) {
                    Ok(src) => to_parse.push((file_path.clone(), key, src)),
                    Err(_) => {
                        tracing::warn!("non-UTF-8 file, skipping: {:?}", file_path);
                        read_err += 1;
                    }
                }
            }
            Err(e) => {
                tracing::warn!("read error {:?}: {e}", file_path);
                read_err += 1;
            }
        }
    }

    let phase1_start = std::time::Instant::now();
    let parsed: Vec<Option<FileGraph>> = to_parse
        .par_iter()
        .map(|(path, _key, src)| {
            parse_file_to_graph(path, src)
                .inspect_err(|e| tracing::warn!("parse error {:?}: {e}", path))
                .ok()
        })
        .collect();
    let phase1_ms = phase1_start.elapsed().as_millis();

    let mut files_ok = l2_hits;
    let mut files_err = read_err;

    for (opt_fg, (_path, key, _src)) in parsed.into_iter().zip(to_parse.iter()) {
        match opt_fg {
            Some(fg) => {
                if let Some(ref c) = cache {
                    c.save_file_graph(key, &fg);
                }
                file_graphs.push(fg);
                files_ok += 1;
            }
            None => {
                files_err += 1;
            }
        }
    }

    let merged = crate::graph::merge_file_graphs(file_graphs);
    let fn_count: usize = merged.nodes.values().map(|v| v.len()).sum();
    let edge_count: usize = merged.callees.values().map(|s| s.len()).sum();

    tracing::info!(
        files_ok,
        files_err,
        l2_hits,
        functions = fn_count,
        call_edges = edge_count,
        phase1_ms,
        total_ms = start.elapsed().as_millis(),
        "index_project completed"
    );

    *graph.write().await = merged;

    if let Some(ref c) = cache {
        c.evict_if_needed(max_cache_entries);
    }

    Ok(json!({
        "status": "indexed",
        "files_ok": files_ok,
        "files_err": files_err,
        "l2_hits": l2_hits,
        "functions": fn_count,
        "call_edges": edge_count,
    }))
}

fn parse_file_to_graph(path: &Path, src: &str) -> anyhow::Result<FileGraph> {
    let mut local = CodebaseGraph::default();
    crate::parser::parse_file(path, src, &mut local)?;
    Ok(FileGraph {
        nodes: local.nodes,
        callees: local.callees,
        symbols: local.symbols,
        types: local.types,
        globals: local.globals,
    })
}

/// Return `true` if any .c/.h file under `project_path` has been modified after `since`.
pub fn has_stale_files(project_path: &Path, since: std::time::SystemTime) -> bool {
    WalkDir::new(project_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let ext = e.path().extension().and_then(|x| x.to_str()).unwrap_or("");
            ext == "c" || ext == "h"
        })
        .any(|e| {
            std::fs::metadata(e.path())
                .and_then(|m| m.modified())
                .map(|mtime| mtime > since)
                .unwrap_or(false)
        })
}
