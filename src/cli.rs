use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Result, bail};
use tokio::sync::RwLock;

use crate::graph::CodebaseGraph;
use crate::tools::get_source::{GetSourceParams, get_source};
use crate::tools::query_call_graph::{QueryCallGraphParams, query_call_graph};
use crate::tools::symbol_lookup::{SymbolLookupParams, symbol_lookup};

pub async fn run(
    path: PathBuf,
    use_toon: bool,
    cache_overhead_pct: usize,
    subcommand: &str,
    args: &[String],
) -> Result<()> {
    let graph = Arc::new(RwLock::new(CodebaseGraph::default()));
    crate::tools::index::index_project(Arc::clone(&graph), &path, cache_overhead_pct)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let graph = graph.read().await;

    let result = match subcommand {
        "symbol-lookup" => {
            let file = args
                .iter()
                .find(|a| !a.starts_with("--"))
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("usage: {} <file> [--kind <kind>]", subcommand))?;
            let kind = flag_value(args, "--kind");
            symbol_lookup(&graph, SymbolLookupParams { file, kind })
        }
        "get-source" => {
            let name = args
                .iter()
                .find(|a| !a.starts_with("--"))
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("usage: {} <name> [--file <file>]", subcommand))?;
            let file = flag_value(args, "--file");
            get_source(&graph, GetSourceParams { name, file })
        }
        "query-call-graph" => {
            let name = args
                .iter()
                .find(|a| !a.starts_with("--"))
                .cloned()
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "usage: {} <name> --direction <callers|callees|both> [--depth <n>] [--file <file>]",
                        subcommand
                    )
                })?;
            let direction = flag_value(args, "--direction").ok_or_else(|| {
                anyhow::anyhow!("--direction <callers|callees|both> is required")
            })?;
            let file = flag_value(args, "--file");
            let depth = flag_value(args, "--depth")
                .map(|v| v.parse::<u32>().map_err(|_| anyhow::anyhow!("--depth must be a positive integer")))
                .transpose()?;
            query_call_graph(&graph, QueryCallGraphParams { name, file, direction, depth })
        }
        other => bail!("unknown subcommand '{}'\nvalid: symbol-lookup, get-source, query-call-graph, mcp", other),
    };

    let output = match result {
        serde_json::Value::String(s) => s,
        ref v if use_toon => toon_format::encode_default(v).unwrap_or_else(|_| v.to_string()),
        ref v => serde_json::to_string_pretty(v)?,
    };
    println!("{output}");
    Ok(())
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|w| w[0] == flag)
        .map(|w| w[1].clone())
}
