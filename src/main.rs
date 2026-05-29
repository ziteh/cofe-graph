use anyhow::Result;
use cofe_graph::cache::DATA_DIR_NAME;
use cofe_graph::server::GraphAnalyzer;
use rmcp::ServiceExt;
use std::path::PathBuf;

const DEFAULT_MAX_L1_CACHE: usize = 5;
const DEFAULT_MAX_L2_CACHE: usize = 5000;
const DEFAULT_WEBUI_PORT: u16 = 5113;

struct Args {
    path: PathBuf,
    use_toon: bool,
    quiet: bool,
    max_l1_cache: usize,
    max_l2_cache: usize,
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

    let max_l1_cache = args
        .windows(2)
        .find(|w| w[0] == "--max-l1-cache")
        .map(|w| {
            w[1].parse::<usize>()
                .expect("--max-l1-cache must be a positive integer")
        })
        .unwrap_or(DEFAULT_MAX_L1_CACHE);

    let max_l2_cache = args
        .windows(2)
        .find(|w| w[0] == "--max-l2-cache")
        .map(|w| {
            w[1].parse::<usize>()
                .expect("--max-l2-cache must be a positive integer")
        })
        .unwrap_or(DEFAULT_MAX_L2_CACHE);

    let path = PathBuf::from(
        args.iter()
            .find(|a| !a.starts_with("--"))
            .unwrap_or_else(|| {
                panic!(
                    "usage: {} <project-path> [--toon] [--quiet] [--max-l1-cache <N>] [--max-l2-cache <N>]",
                    program_name
                )
            }),
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
        max_l1_cache,
        max_l2_cache,
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args()?;

    let log_dir = args.path.join(DATA_DIR_NAME).join("logs");
    let _log_guard = cofe_graph::log::init(&log_dir, args.quiet);

    // Start server
    tracing::info!(
        project = %args.path.display(),
        format = if args.use_toon { "toon" } else { "json" },
        max_l1_cache = args.max_l1_cache,
        max_l2_cache = args.max_l2_cache,
        "starting MCP server on stdio, and the web UI on http://localhost:{DEFAULT_WEBUI_PORT}",
    );
    let server = GraphAnalyzer::new(
        args.path,
        args.use_toon,
        args.max_l1_cache,
        args.max_l2_cache,
    );
    tokio::spawn(cofe_graph::webui::start(server.clone(), DEFAULT_WEBUI_PORT));
    let service = server.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;

    Ok(())
}
