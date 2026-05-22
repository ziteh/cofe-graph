use std::path::Path;
use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{ServerCapabilities, ServerInfo};
use rmcp::schemars;
use rmcp::{ServerHandler, tool, tool_handler, tool_router};
use serde::Deserialize;
use tokio::sync::RwLock;
use walkdir::WalkDir;

use crate::cache::Cache;
use crate::graph::CallGraph;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IndexProjectParams {
    #[schemars(description = "Path to the directory containing .c/.h files")]
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FindFunctionParams {
    #[schemars(description = "Function name substring to search for (case-insensitive)")]
    pub name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TraverseParams {
    #[schemars(description = "Function name")]
    pub name: String,
    #[schemars(description = "BFS traversal depth (default: 1)")]
    pub depth: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetSourceParams {
    #[schemars(description = "Exact function name")]
    pub name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetPathParams {
    #[schemars(description = "Starting function name")]
    pub from: String,
    #[schemars(description = "Target function name")]
    pub to: String,
}

#[derive(Clone)]
pub struct CofeGraph {
    graph: Arc<RwLock<CallGraph>>,
    tool_router: ToolRouter<Self>,
}

impl CofeGraph {
    pub fn new() -> Self {
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

        let cache = Cache::open(base);

        if let Some(ref c) = cache {
            if let Some(cached) = c.load() {
                let fn_count = cached.nodes.len();
                let edge_count: usize = cached.callees.values().map(|s| s.len()).sum();
                *self.graph.write().await = cached;
                c.record_hit();
                return format!(
                    "Loaded from cache (commit {}). Found {fn_count} functions and {edge_count} call edges.",
                    c.commit_hash
                );
            }
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
                Ok(src) => match crate::parser::parse_file(p, &src, &mut graph) {
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

        if let Some(ref c) = cache {
            c.save(&graph);
        }

        let fn_count = graph.nodes.len();
        let edge_count: usize = graph.callees.values().map(|s| s.len()).sum();
        let cache_note = if cache.is_some() { " (cached)" } else { "" };
        format!(
            "Indexed {files_ok} files ({files_err} errors). Found {fn_count} functions and {edge_count} call edges.{cache_note}"
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

    #[tool(description = "Get the source code of a function by exact name")]
    async fn get_source(&self, params: Parameters<GetSourceParams>) -> String {
        let Parameters(GetSourceParams { name }) = params;
        let graph = self.graph.read().await;
        match graph.nodes.get(&name) {
            Some(n) => format!("// {}:{}\n{}", n.file.display(), n.line, n.source),
            None => format!("Function '{name}' not found"),
        }
    }

    #[tool(description = "Find the shortest call path from one function to another")]
    async fn get_path(&self, params: Parameters<GetPathParams>) -> String {
        let Parameters(GetPathParams { from, to }) = params;
        let graph = self.graph.read().await;
        match graph.find_path(&from, &to) {
            Some(path) => path.join(" -> "),
            None => format!("No call path from '{from}' to '{to}'"),
        }
    }

    #[tool(description = "List functions that are never called (potential dead code)")]
    async fn find_dead_code(&self) -> String {
        let graph = self.graph.read().await;
        let mut dead = graph.find_dead_code();
        dead.sort_by_key(|n| &n.name);
        if dead.is_empty() {
            return "No dead code found (all functions have at least one caller)".to_string();
        }
        dead.iter()
            .map(|n| format!("{} @ {}:{}", n.name, n.file.display(), n.line))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for CofeGraph {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions("GraphRAG tools for C code analysis. Call index_project first, then use find_function / get_callers / get_callees / get_source / get_path / find_dead_code.")
    }
}
