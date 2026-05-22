mod graph;
mod parser;

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{ServerCapabilities, ServerInfo};
use rmcp::schemars;
use rmcp::{ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use serde::Deserialize;
use tokio::sync::RwLock;
use walkdir::WalkDir;

use graph::CallGraph;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct IndexProjectParams {
    #[schemars(description = "Path to the directory containing .c/.h files")]
    path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct FindFunctionParams {
    #[schemars(description = "Function name substring to search for (case-insensitive)")]
    name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct TraverseParams {
    #[schemars(description = "Function name")]
    name: String,
    #[schemars(description = "BFS traversal depth (default: 1)")]
    depth: Option<u32>,
}

#[derive(Clone)]
struct CofeGraph {
    graph: Arc<RwLock<CallGraph>>,
    tool_router: ToolRouter<Self>,
}

impl CofeGraph {
    fn new() -> Self {
        Self {
            graph: Arc::new(RwLock::new(CallGraph::default())),
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl CofeGraph {
    #[tool(
        description = "Index all .c and .h files in a directory and build a function call graph"
    )]
    async fn index_project(&self, params: Parameters<IndexProjectParams>) -> String {
        let Parameters(IndexProjectParams { path }) = params;
        let base = Path::new(&path);
        if !base.is_dir() {
            return format!("error: '{}' is not a directory", path);
        }

        let mut graph = self.graph.write().await;
        graph.clear();

        let mut files_ok = 0usize;
        let mut files_err = 0usize;

        for entry in WalkDir::new(base).into_iter().filter_map(|e| e.ok()) {
            let p = entry.path();
            let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("");
            if ext != "c" && ext != "h" {
                continue;
            }
            match std::fs::read_to_string(p) {
                Ok(src) => match parser::parse_file(p, &src, &mut graph) {
                    Ok(_) => files_ok += 1,
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

        let fn_count = graph.nodes.len();
        let edge_count: usize = graph.callees.values().map(|s| s.len()).sum();
        format!(
            "Indexed {files_ok} files ({files_err} errors). Found {fn_count} functions and {edge_count} call edges."
        )
    }

    #[tool(description = "Search for functions by name (case-insensitive substring match)")]
    async fn find_function(&self, params: Parameters<FindFunctionParams>) -> String {
        let Parameters(FindFunctionParams { name }) = params;
        let graph = self.graph.read().await;
        let mut matches: Vec<String> = graph
            .find_function(&name)
            .into_iter()
            .map(|n| format!("{} @ {}:{}", n.name, n.file.display(), n.line))
            .collect();
        matches.sort();
        if matches.is_empty() {
            return format!("No functions matching '{name}'");
        }
        matches.join("\n")
    }

    #[tool(description = "Get all functions that call the given function (BFS up to depth)")]
    async fn get_callers(&self, params: Parameters<TraverseParams>) -> String {
        let Parameters(TraverseParams { name, depth }) = params;
        let graph = self.graph.read().await;
        let mut callers = graph.get_callers(&name, depth.unwrap_or(1) as usize);
        callers.sort();
        if callers.is_empty() {
            return format!("No callers found for '{name}'");
        }
        callers.join("\n")
    }

    #[tool(description = "Get all functions called by the given function (BFS up to depth)")]
    async fn get_callees(&self, params: Parameters<TraverseParams>) -> String {
        let Parameters(TraverseParams { name, depth }) = params;
        let graph = self.graph.read().await;
        let mut callees = graph.get_callees(&name, depth.unwrap_or(1) as usize);
        callees.sort();
        if callees.is_empty() {
            return format!("No callees found for '{name}'");
        }
        callees.join("\n")
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for CofeGraph {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions("GraphRAG tools for C code analysis. Call index_project first, then use find_function / get_callers / get_callees.")
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    eprintln!("[cofe-graph] starting MCP server on stdio");
    let service = CofeGraph::new().serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}
