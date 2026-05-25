use anyhow::Result;
use cofe_graph::cache::COFE_DATA_DIR;
use cofe_graph::server::CofeGraph;
use rmcp::ServiceExt;
use std::path::PathBuf;

const DEFAULT_MAX_CACHE_ENTRIES: usize = 15;
const DEFAULT_WEBUI_PORT: u16 = 5113;

struct Args {
    path: PathBuf,
    use_toon: bool,
    quiet: bool,
    max_cache: usize,
}

fn parse_args() -> Result<Args> {
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

    Ok(Args {
        path,
        use_toon,
        quiet,
        max_cache,
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args()?;

    let log_dir = args.path.join(COFE_DATA_DIR).join("logs");
    let _log_guard = cofe_graph::log::init(&log_dir, args.quiet);

    // Start server
    tracing::info!(
        project = %args.path.display(),
        format = if args.use_toon { "toon" } else { "json" },
        max_cache = args.max_cache,
        "starting MCP server on stdio, and the web UI on http://localhost:{DEFAULT_WEBUI_PORT}",
    );
    let cofe = CofeGraph::new(args.path, args.use_toon, args.max_cache);
    tokio::spawn(cofe_graph::webui::start(cofe.clone(), DEFAULT_WEBUI_PORT));
    let service = cofe.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;

    Ok(())
}
