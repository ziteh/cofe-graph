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
    let path = PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("usage: cofe-graph <project-path>"),
    );
    anyhow::ensure!(
        path.is_dir(),
        "project path is not a directory: {}",
        path.display()
    );

    eprintln!(
        "[cofe-graph] starting MCP server on stdio (project: {})",
        path.display()
    );
    let service = CofeGraph::new(path).serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}
