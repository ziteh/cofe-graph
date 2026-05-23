mod cache;
mod graph;
mod parser;
mod server;

use anyhow::Result;
use cache::DEFAULT_MAX_CACHE_ENTRIES;
use rmcp::ServiceExt;
use server::CofeGraph;
use std::path::PathBuf;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let use_toon = args.contains(&"--toon".to_string());
    let quiet = args.contains(&"--quiet".to_string());
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
            .expect("usage: cofe-graph <project-path> [--toon] [--quiet] [--max-cache <N>]"),
    );
    anyhow::ensure!(
        path.is_dir(),
        "project path is not a directory: {}",
        path.display()
    );

    // Init logging
    let log_dir = path.join(".cofe-graph").join("logs");
    std::fs::create_dir_all(&log_dir)?;

    let file_appender = tracing_appender::rolling::daily(&log_dir, "cofe-graph.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_target(false);

    let stderr_layer = if quiet {
        None
    } else {
        Some(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_target(false),
        )
    };

    tracing_subscriber::registry()
        .with(file_layer)
        .with(stderr_layer)
        .init();

    // Start the MCP server
    tracing::info!(
        project = %path.display(),
        format = if use_toon { "toon" } else { "json" },
        max_cache,
        "starting MCP server on stdio",
    );
    let service = CofeGraph::new(path, use_toon, max_cache)
        .serve(rmcp::transport::stdio())
        .await?;
    service.waiting().await?;

    Ok(())
}
