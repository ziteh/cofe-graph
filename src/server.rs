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

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FindInFileParams {
    #[schemars(description = "Filename substring to match against file paths (case-insensitive)")]
    pub filename: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TopNParams {
    #[schemars(description = "Number of results to return (default: 10)")]
    pub top_n: Option<u32>,
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
        let mut sources: Vec<String> = Vec::new();

        for entry in WalkDir::new(base).into_iter().filter_map(|e| e.ok()) {
            let p = entry.path();
            let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("");
            if ext != "c" && ext != "h" {
                continue;
            }
            match std::fs::read_to_string(p) {
                Ok(src) => match crate::parser::parse_file(p, &src, &mut graph) {
                    Ok(_) => {
                        files_ok += 1;
                        sources.push(src);
                    }
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

        // Pass 3: scan macro refs after all function definitions are known
        for src in &sources {
            if let Err(e) = crate::parser::scan_macro_refs(src, &mut graph) {
                eprintln!("[cofe-graph] macro scan error: {e}");
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

    #[tool(
        description = "List functions that are never called (potential dead code), classified by likely reason"
    )]
    async fn find_dead_code(&self) -> String {
        let graph = self.graph.read().await;
        let mut dead = graph.find_dead_code();
        dead.sort_by_key(|(n, _)| &n.name);
        if dead.is_empty() {
            return "No dead code found (all functions have at least one caller)".to_string();
        }

        use crate::graph::DeadCodeKind;
        let true_dead: Vec<_> = dead
            .iter()
            .filter(|(_, k)| *k == DeadCodeKind::Suspicious)
            .collect();
        let macro_reg: Vec<_> = dead
            .iter()
            .filter(|(_, k)| *k == DeadCodeKind::MacroRegistered)
            .collect();
        let cb_name: Vec<_> = dead
            .iter()
            .filter(|(_, k)| *k == DeadCodeKind::CallbackByName)
            .collect();
        let entry: Vec<_> = dead
            .iter()
            .filter(|(_, k)| *k == DeadCodeKind::Entrypoint)
            .collect();

        let fmt = |(n, k): &&(&crate::graph::FunctionNode, DeadCodeKind)| {
            format!(
                "[{}] {} @ {}:{}",
                k.as_str(),
                n.name,
                n.file.display(),
                n.line
            )
        };

        let mut sections: Vec<String> = Vec::new();

        if !true_dead.is_empty() {
            let lines: Vec<_> = true_dead.iter().map(fmt).collect();
            sections.push(format!(
                "=== Suspicious (no evidence of use) ({}) ===\n{}",
                true_dead.len(),
                lines.join("\n")
            ));
        } else {
            sections.push("=== Suspicious (no evidence of use) (0) ===\n(none)".to_string());
        }
        if !macro_reg.is_empty() {
            let lines: Vec<_> = macro_reg.iter().map(fmt).collect();
            sections.push(format!(
                "=== Registered via macro ({}) ===\n{}",
                macro_reg.len(),
                lines.join("\n")
            ));
        }
        if !cb_name.is_empty() {
            let lines: Vec<_> = cb_name.iter().map(fmt).collect();
            sections.push(format!(
                "=== Likely callbacks by name ({}) ===\n{}",
                cb_name.len(),
                lines.join("\n")
            ));
        }
        if !entry.is_empty() {
            let lines: Vec<_> = entry.iter().map(fmt).collect();
            sections.push(format!(
                "=== Entrypoints ({}) ===\n{}",
                entry.len(),
                lines.join("\n")
            ));
        }

        sections.join("\n\n")
    }

    #[tool(
        description = "Show overall call graph statistics: function count, edge count, and top fan-in/fan-out functions"
    )]
    async fn get_stats(&self) -> String {
        let graph = self.graph.read().await;
        let fn_count = graph.nodes.len();
        let edge_count: usize = graph.callees.values().map(|s| s.len()).sum();
        use crate::graph::DeadCodeKind;
        let dead = graph.find_dead_code();
        let dead_count = dead.len();
        let true_dead_count = dead
            .iter()
            .filter(|(_, k)| *k == DeadCodeKind::Suspicious)
            .count();

        let top_fan_in = graph.top_by_fan_in(5);
        let top_fan_out = graph.top_by_fan_out(5);

        let fan_in_lines: Vec<String> = top_fan_in
            .iter()
            .map(|(name, count)| format!("  {name} ({count} callers)"))
            .collect();
        let fan_out_lines: Vec<String> = top_fan_out
            .iter()
            .map(|(name, count)| format!("  {name} ({count} callees)"))
            .collect();

        format!(
            "Functions : {fn_count}\nCall edges: {edge_count}\nDead code : {dead_count} (no callers, {true_dead_count} true dead)\n\nTop fan-in (most callers):\n{}\n\nTop fan-out (most callees):\n{}",
            fan_in_lines.join("\n"),
            fan_out_lines.join("\n"),
        )
    }

    #[tool(
        description = "List all functions defined in files matching a filename substring (case-insensitive)"
    )]
    async fn find_functions_in_file(&self, params: Parameters<FindInFileParams>) -> String {
        let Parameters(FindInFileParams { filename }) = params;
        let graph = self.graph.read().await;
        let mut results: Vec<String> = graph
            .find_functions_in_file(&filename)
            .into_iter()
            .map(|n| format!("{} @ {}:{}", n.name, n.file.display(), n.line))
            .collect();
        results.sort();
        if results.is_empty() {
            return format!("No functions found in files matching '{filename}'");
        }
        results.join("\n")
    }

    #[tool(
        description = "List the top N functions by fan-in (most callers) — these are shared utilities or hotspots"
    )]
    async fn find_high_fan_in(&self, params: Parameters<TopNParams>) -> String {
        let Parameters(TopNParams { top_n }) = params;
        let n = top_n.unwrap_or(10) as usize;
        let graph = self.graph.read().await;
        let ranked = graph.top_by_fan_in(n);
        if ranked.is_empty() {
            return "No functions in graph".to_string();
        }
        ranked
            .iter()
            .enumerate()
            .map(|(i, (name, count))| {
                let node = graph.nodes.get(*name);
                let loc = node.map_or(String::new(), |nd| {
                    format!(" @ {}:{}", nd.file.display(), nd.line)
                });
                format!("{}. {name}{loc} — {count} callers", i + 1)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for CofeGraph {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions("GraphRAG tools for C code analysis. Call index_project first, then use: find_function / get_callers / get_callees / get_source / get_path / find_dead_code / get_stats / find_functions_in_file / find_high_fan_in.")
    }
}
