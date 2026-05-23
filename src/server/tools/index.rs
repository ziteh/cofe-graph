use rmcp::schemars;
use serde::Deserialize;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use walkdir::WalkDir;

use crate::cache::Cache;
use crate::graph::CallGraph;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IndexProjectParams {
    #[schemars(description = "Path to the directory containing .c/.h files")]
    pub path: String,
}

pub async fn index_project(graph: Arc<RwLock<CallGraph>>, params: IndexProjectParams) -> String {
    let IndexProjectParams { path } = params;
    let base = Path::new(&path);
    if !base.is_dir() {
        return format!("error: '{}' is not a directory", path);
    }

    let cache = Cache::open(base);

    if let Some(ref c) = cache {
        if let Some(cached) = c.load() {
            let fn_count = cached.nodes.len();
            let edge_count: usize = cached.callees.values().map(|s| s.len()).sum();
            *graph.write().await = cached;
            c.record_hit();
            return format!(
                "Loaded from cache (commit {}). Found {fn_count} functions and {edge_count} call edges.",
                c.commit_hash
            );
        }
    }

    let mut g = graph.write().await;
    g.clear();

    let mut files_ok = 0usize;
    let mut files_err = 0usize;
    let mut sources: Vec<String> = Vec::new();

    for entry in WalkDir::new(base).into_iter().filter_map(|e| e.ok()) {
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

    if let Some(ref c) = cache {
        c.save(&g);
    }

    let fn_count = g.nodes.len();
    let edge_count: usize = g.callees.values().map(|s| s.len()).sum();
    let cache_note = if cache.is_some() { " (cached)" } else { "" };
    format!(
        "Indexed {files_ok} files ({files_err} errors). Found {fn_count} functions and {edge_count} call edges.{cache_note}"
    )
}
