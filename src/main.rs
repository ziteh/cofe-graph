mod cache;
mod graph;
mod parser;
mod server;

use anyhow::Result;
use rmcp::ServiceExt;
use server::CofeGraph;

#[tokio::main]
async fn main() -> Result<()> {
    eprintln!("[cofe-graph] starting MCP server on stdio");
    let service = CofeGraph::new().serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}
