use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use walkdir::WalkDir;

use serde_json::json;

use crate::cache::Cache;
use crate::graph::CallGraph;

pub async fn index_project(
    graph: Arc<RwLock<CallGraph>>,
    path: &Path,
    max_cache_entries: usize,
) -> String {
    let cache = Cache::open(path, max_cache_entries);

    if let Some(ref c) = cache {
        if let Some(cached) = c.load() {
            let fn_count = cached.nodes.len();
            let edge_count: usize = cached.callees.values().map(|s| s.len()).sum();
            *graph.write().await = cached;
            c.record_hit();
            return json!({
                "status": "cached",
                "commit": c.commit_hash,
                "functions": fn_count,
                "call_edges": edge_count,
            })
            .to_string();
        }
    }

    let mut g = graph.write().await;
    g.clear();

    let mut files_ok = 0usize;
    let mut files_err = 0usize;
    let mut sources: Vec<String> = Vec::new();

    for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("");
        if ext != "c" && ext != "h" {
            continue;
        }
        match std::fs::read_to_string(p) {
            Ok(src) => match crate::parser::parse_file(p, &src, &mut g) {
                Ok(_) => {
                    files_ok += 1;
                    sources.push(src);
                }
                Err(e) => {
                    eprintln!("[cofe-graph] parse error {:?}: {e}", p);
                    files_err += 1;
                }
            },
            Err(e) => {
                eprintln!("[cofe-graph] read error {:?}: {e}", p);
                files_err += 1;
            }
        }
    }

    for src in &sources {
        if let Err(e) = crate::parser::scan_macro_refs(src, &mut g) {
            eprintln!("[cofe-graph] macro scan error: {e}");
        }
    }

    let cached = if let Some(ref c) = cache {
        c.save(&g);
        true
    } else {
        false
    };

    let fn_count = g.nodes.len();
    let edge_count: usize = g.callees.values().map(|s| s.len()).sum();

    json!({
        "status": "indexed",
        "files_ok": files_ok,
        "files_err": files_err,
        "functions": fn_count,
        "call_edges": edge_count,
        "cached": cached,
    })
    .to_string()
}
