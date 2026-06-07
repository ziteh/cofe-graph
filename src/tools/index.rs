use rayon::prelude::*;
use std::collections::HashMap;
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

    // Phase 1: parse files in parallel into isolated local graphs
    let phase1_start = std::time::Instant::now();
    let results: Vec<(&Path, &str, anyhow::Result<CodebaseGraph>)> = sources
        .par_iter()
        .map(|&(path, src)| {
            let mut local = CodebaseGraph::default();
            let res = crate::parser::parse_file(path, src, &mut local).map(|_| local);
            (path, src, res)
        })
        .collect();
    let phase1_elapsed = phase1_start.elapsed();

    let mut files_ok = 0usize;
    let mut files_err = 0usize;
    let mut ok_sources: Vec<&str> = Vec::new();

    for (path, src, res) in results {
        match res {
            Ok(local) => {
                g.merge(local);
                files_ok += 1;
                ok_sources.push(src);
            }
            Err(e) => {
                tracing::warn!("parse error {:?}: {e}", path);
                files_err += 1;
            }
        }
    }

    // Phase 2: scan macro refs in parallel (read-only on g.nodes)
    let phase2_start = std::time::Instant::now();
    let macro_refs: Vec<std::collections::HashSet<String>> = {
        let nodes = &g.nodes;
        ok_sources
            .par_iter()
            .filter_map(|src| {
                crate::parser::collect_macro_refs(src, nodes)
                    .map_err(|e| tracing::warn!("macro scan error: {e}"))
                    .ok()
            })
            .collect()
    };
    let phase2_elapsed = phase2_start.elapsed();

    g.macro_referenced.extend(macro_refs.into_iter().flatten());

    let total_elapsed = start.elapsed();
    tracing::info!(
        files_ok,
        files_err,
        functions = g.nodes.len(),
        phase1_ms = phase1_elapsed.as_millis(),
        phase2_ms = phase2_elapsed.as_millis(),
        total_ms = total_elapsed.as_millis(),
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
    graph: Arc<RwLock<CodebaseGraph>>,
    path: &Path,
    max_l1_entries: usize,
    max_l2_entries: usize,
) -> Result<Value, String> {
    let cache = Cache::open(path, max_l1_entries, max_l2_entries);

    // L1: full-graph hit
    if let Some(ref c) = cache
        && let Some(cached) = c.load()
    {
        let fn_count = cached.nodes.len();
        let edge_count: usize = cached.callees.values().map(|s| s.len()).sum();
        *graph.write().await = cached;
        return Ok(json!({
            "status": "cached",
            "commit": c.commit_hash,
            "functions": fn_count,
            "call_edges": edge_count,
        }));
    }

    let start = std::time::Instant::now();

    // L1 miss: get file→blob_sha from git
    let blob_map: Option<HashMap<PathBuf, String>> =
        cache.as_ref().and_then(|_| Cache::ls_files(path));

    // Discover all .c/.h files
    let all_files: Vec<PathBuf> = WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let ext = e.path().extension().and_then(|x| x.to_str()).unwrap_or("");
            ext == "c" || ext == "h"
        })
        .map(|e| e.into_path())
        .collect();

    // Split: L2 hits (no file read) vs. files needing parse
    let mut file_graphs: Vec<FileGraph> = Vec::new();
    // (path, cache_key, source)
    let mut to_parse: Vec<(PathBuf, String, String)> = Vec::new();
    let mut read_err = 0usize;
    let mut l2_hits = 0usize;

    for file_path in &all_files {
        let blob_sha: Option<String> = blob_map.as_ref().and_then(|m| m.get(file_path)).cloned();

        // Fast path: L2 hit by git blob SHA — no file read needed
        if let (Some(sha), Some(c)) = (blob_sha.as_deref(), cache.as_ref())
            && let Some(fg) = c.load_file_graph(sha)
        {
            file_graphs.push(fg);
            l2_hits += 1;
            continue;
        }

        // Read file content (needed for parse or content-hash L2 fallback)
        match std::fs::read_to_string(file_path) {
            Ok(src) => {
                let no_blob_sha = blob_sha.is_none();
                let key = blob_sha.unwrap_or_else(|| {
                    // No git: use blake3 hash of content as cache key
                    blake3::hash(src.as_bytes()).to_hex().to_string()
                });

                // Content-hash L2 check: covers both non-git repos and untracked files
                if no_blob_sha
                    && let Some(c) = cache.as_ref()
                    && let Some(fg) = c.load_file_graph(&key)
                {
                    file_graphs.push(fg);
                    l2_hits += 1;
                    continue;
                }
                to_parse.push((file_path.clone(), key, src));
            }
            Err(e) => {
                tracing::warn!("read error {:?}: {e}", file_path);
                read_err += 1;
            }
        }
    }

    // Parse uncached files in parallel
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
                if let Some(c) = cache.as_ref() {
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

    // Merge all FileGraphs → CodebaseGraph
    let mut merged = crate::graph::merge_file_graphs(file_graphs);
    if let Some(ref bm) = blob_map {
        merged.file_shas = bm.clone();
    }
    let fn_count = merged.nodes.len();
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

    // Only cache a complete graph
    if files_err == 0
        && let Some(c) = cache.as_ref()
    {
        c.save(&merged);
    }

    *graph.write().await = merged;

    Ok(json!({
        "status": "indexed",
        "commit": cache.as_ref().map(|c| c.commit_hash.as_str()),
        "files_ok": files_ok,
        "files_err": files_err,
        "l2_hits": l2_hits,
        "functions": fn_count,
        "call_edges": edge_count,
    }))
}

/// Parse a single source file into a file graph.
fn parse_file_to_graph(path: &Path, src: &str) -> anyhow::Result<FileGraph> {
    let mut local = CodebaseGraph::default();
    crate::parser::parse_file(path, src, &mut local)?;
    let macro_arg_candidates = crate::parser::collect_macro_arg_candidates(src).unwrap_or_default();
    Ok(FileGraph {
        nodes: local.nodes,
        callees: local.callees,
        symbols: local.symbols,
        includes: local.includes,
        types: local.types,
        globals: local.globals,
        macro_arg_candidates,
    })
}
