mod cache;
mod graph;
mod parser;
mod server;

use anyhow::Result;
use rmcp::ServiceExt;
use server::CofeGraph;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let use_toon = args.contains(&"--toon".to_string());
    let path = PathBuf::from(
        args.iter()
            .find(|a| !a.starts_with("--"))
            .expect("usage: cofe-graph <project-path> [--toon]"),
    );
    anyhow::ensure!(
        path.is_dir(),
        "project path is not a directory: {}",
        path.display()
    );

    eprintln!(
        "[cofe-graph] starting MCP server on stdio (project: {}, format: {})",
        path.display(),
        if use_toon { "toon" } else { "json" },
    );
    let service = CofeGraph::new(path, use_toon)
        .serve(rmcp::transport::stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}
