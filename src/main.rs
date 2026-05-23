mod cache;
mod graph;
mod parser;
mod server;

use anyhow::Result;
use cache::DEFAULT_MAX_CACHE_ENTRIES;
use rmcp::ServiceExt;
use server::CofeGraph;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let use_toon = args.contains(&"--toon".to_string());
    let max_cache = args
        .windows(2)
        .find(|w| w[0] == "--max-cache")
        .map(|w| {
            w[1].parse::<usize>()
                .expect("--max-cache must be a positive integer")
        })
        .unwrap_or(DEFAULT_MAX_CACHE_ENTRIES);
    let path = PathBuf::from(
        args.iter()
            .find(|a| !a.starts_with("--"))
            .expect("usage: cofe-graph <project-path> [--toon] [--max-cache <N>]"),
    );
    anyhow::ensure!(
        path.is_dir(),
        "project path is not a directory: {}",
        path.display()
    );

    eprintln!(
        "[cofe-graph] starting MCP server on stdio (project: {}, format: {}, max-cache: {})",
        path.display(),
        if use_toon { "toon" } else { "json" },
        max_cache,
    );
    let service = CofeGraph::new(path, use_toon, max_cache)
        .serve(rmcp::transport::stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}
