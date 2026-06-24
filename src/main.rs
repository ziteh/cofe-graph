use anyhow::Result;
use cofe_graph::cache::DATA_DIR_NAME;
use cofe_graph::server::GraphAnalyzer;
use rmcp::ServiceExt;
use std::path::PathBuf;

const DEFAULT_CACHE_OVERHEAD_PCT: usize = 25;

struct Args {
    path: PathBuf,
    use_toon: bool,
    quiet: bool,
    cache_overhead_pct: usize,
}

fn parse_args() -> Result<Args> {
    let mut all_args = std::env::args();
    let program_name = all_args
        .next()
        .and_then(|p| {
            std::path::Path::new(&p)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "app".to_string());

    let args: Vec<String> = all_args.collect();

    let use_toon = args.contains(&"--toon".to_string());
    let quiet = args.contains(&"--quiet".to_string());

    let cache_overhead_pct = args
        .windows(2)
        .find(|w| w[0] == "--cache-overhead")
        .map(|w| {
            w[1].parse::<usize>()
                .expect("--cache-overhead must be a non-negative integer")
        })
        .unwrap_or(DEFAULT_CACHE_OVERHEAD_PCT);

    let path = PathBuf::from(
        args.iter()
            .find(|a| !a.starts_with("--"))
            .unwrap_or_else(|| {
                panic!(
                    "usage: {} <project-path> [--toon] [--quiet] [--cache-overhead <PCT>]",
                    program_name
                )
            }),
    );
    anyhow::ensure!(
        path.is_dir(),
        "project path is not a directory: {}",
        path.display()
    );

    Ok(Args { path, use_toon, quiet, cache_overhead_pct })
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args()?;

    let log_dir = args.path.join(DATA_DIR_NAME).join("logs");
    let _log_guard = cofe_graph::log::init(&log_dir, args.quiet);

    tracing::info!(
        project = %args.path.display(),
        format = if args.use_toon { "toon" } else { "json" },
        cache_overhead_pct = args.cache_overhead_pct,
        "starting MCP server on stdio",
    );
    let server = GraphAnalyzer::new(args.path, args.use_toon, args.cache_overhead_pct);
    let service = server.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;

    Ok(())
}
