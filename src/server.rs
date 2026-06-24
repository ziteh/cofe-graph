use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Content, ServerCapabilities, ServerInfo};
use rmcp::{ServerHandler, tool, tool_handler, tool_router};
use tokio::sync::RwLock;

use crate::graph::CodebaseGraph;
use crate::tools::get_source::GetSourceParams;
use crate::tools::query_call_graph::QueryCallGraphParams;
use crate::tools::symbol_lookup::SymbolLookupParams;

#[derive(Clone)]
pub struct GraphAnalyzer {
    pub(crate) graph: Arc<RwLock<CodebaseGraph>>,
    pub(crate) project_path: PathBuf,
    use_toon: bool,
    cache_overhead_pct: usize,
    indexed_at: Arc<std::sync::Mutex<std::time::SystemTime>>,
    last_freshness_check: Arc<std::sync::Mutex<std::time::Instant>>,
    index_lock: Arc<tokio::sync::Mutex<()>>,
    tool_router: ToolRouter<Self>,
}

impl GraphAnalyzer {
    pub fn new(path: PathBuf, use_toon: bool, cache_overhead_pct: usize) -> Self {
        let graph = Arc::new(RwLock::new(CodebaseGraph::default()));
        let indexed_at = Arc::new(std::sync::Mutex::new(std::time::SystemTime::UNIX_EPOCH));
        let last_freshness_check = Arc::new(std::sync::Mutex::new(std::time::Instant::now()));
        let index_lock = Arc::new(tokio::sync::Mutex::new(()));
        let g = Arc::clone(&graph);
        let p = path.clone();
        let ts = Arc::clone(&indexed_at);
        let lk = Arc::clone(&index_lock);
        tokio::spawn(async move {
            let _guard = lk.lock().await;
            let t = std::time::SystemTime::now();
            let _ = crate::tools::index::index_project(g, &p, cache_overhead_pct).await;
            *ts.lock().unwrap() = t;
        });

        Self {
            graph,
            project_path: path,
            use_toon,
            cache_overhead_pct,
            indexed_at,
            last_freshness_check,
            index_lock,
            tool_router: Self::tool_router(),
        }
    }

    async fn ensure_fresh(&self) {
        // Debounce: skip the directory walk if we checked recently.
        {
            let mut last = self.last_freshness_check.lock().unwrap();
            if last.elapsed() < Duration::from_secs(2) {
                return;
            }
            *last = std::time::Instant::now();
        }

        let since = *self.indexed_at.lock().unwrap();
        if !crate::tools::index::has_stale_files(&self.project_path, since) {
            return;
        }

        let _guard = self.index_lock.lock().await;
        let since = *self.indexed_at.lock().unwrap();
        if !crate::tools::index::has_stale_files(&self.project_path, since) {
            return;
        }

        let t = std::time::SystemTime::now();
        let _ = crate::tools::index::index_project(
            Arc::clone(&self.graph),
            &self.project_path,
            self.cache_overhead_pct,
        )
        .await;
        *self.indexed_at.lock().unwrap() = t;
    }

    fn call_result(&self, value: serde_json::Value) -> CallToolResult {
        let text = match value {
            serde_json::Value::String(s) => s,
            ref v if self.use_toon => {
                toon_format::encode_default(v).unwrap_or_else(|_| v.to_string())
            }
            ref v => v.to_string(),
        };
        CallToolResult::success(vec![Content::text(text)])
    }
}

#[tool_router]
impl GraphAnalyzer {
    #[tool(
        description = "List all symbols in a file (functions, macros, typedefs, variables) with signatures. kind filters to function|variable|typedef|macro."
    )]
    async fn symbol_lookup(&self, params: Parameters<SymbolLookupParams>) -> CallToolResult {
        self.ensure_fresh().await;
        let Parameters(p) = params;
        self.call_result(crate::tools::symbol_lookup::symbol_lookup(
            &*self.graph.read().await,
            p,
        ))
    }

    #[tool(
        description = "Return the complete source of a symbol (function, type, macro, variable) by exact name. Provide file to disambiguate when the same name appears in multiple files."
    )]
    async fn get_source(&self, params: Parameters<GetSourceParams>) -> CallToolResult {
        self.ensure_fresh().await;
        let Parameters(p) = params;
        self.call_result(crate::tools::get_source::get_source(
            &*self.graph.read().await,
            p,
        ))
    }

    #[tool(
        description = "Query the call graph for a function. direction: callers|callees|both. depth defaults to 1. Returns caller/callee names, files, and call-site line numbers (depth=1 only). When multiple files define a function with the same name, provide file to disambiguate; without it, returns ambiguous. Note: call edges are merged across all same-name functions, so callers/callees may reflect calls from sibling definitions."
    )]
    async fn query_call_graph(&self, params: Parameters<QueryCallGraphParams>) -> CallToolResult {
        self.ensure_fresh().await;
        let Parameters(p) = params;
        self.call_result(crate::tools::query_call_graph::query_call_graph(
            &*self.graph.read().await,
            p,
        ))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for GraphAnalyzer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .build(),
        )
        .with_instructions("C codebase call-graph and symbol index.")
    }
}
