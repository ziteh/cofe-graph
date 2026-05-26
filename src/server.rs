use std::path::PathBuf;
use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Content, ServerCapabilities, ServerInfo};
use rmcp::{ServerHandler, tool, tool_handler, tool_router};
use tokio::sync::RwLock;

use crate::annotations::AnnotationStore;
use crate::graph::CallGraph;
use crate::tools::annotate::{AnnotateFileParams, FileContextParams, FilePathParams};
use crate::tools::functions::{FindInFileParams, GetSourceParams, TopNParams};
use crate::tools::includes::{GetIncludersParams, GetIncludesParams};
use crate::tools::search::SearchParams;
use crate::tools::traverse::{CalleesParams, CallersParams, GetPathParams};
use crate::tools::types::GetTypeUsersParams;

#[derive(Clone)]
pub struct CofeGraph {
    pub(crate) graph: Arc<RwLock<CallGraph>>,
    annotations: Arc<RwLock<AnnotationStore>>,
    pub(crate) project_path: PathBuf,
    use_toon: bool,
    max_cache_entries: usize,
    tool_router: ToolRouter<Self>,
}

impl CofeGraph {
    pub fn new(path: PathBuf, use_toon: bool, max_cache_entries: usize) -> Self {
        let graph = Arc::new(RwLock::new(CallGraph::default()));
        let annotations = Arc::new(RwLock::new(AnnotationStore::load(&path)));
        let g = Arc::clone(&graph);
        let p = path.clone();
        tokio::spawn(async move {
            let _ = crate::tools::index::index_project(g, &p, max_cache_entries).await;
        });
        Self {
            graph,
            annotations,
            project_path: path,
            use_toon,
            max_cache_entries,
            tool_router: Self::tool_router(),
        }
    }

    fn call_result(&self, result: Result<serde_json::Value, String>) -> CallToolResult {
        match result {
            Ok(value) => {
                let text = match value {
                    serde_json::Value::String(s) => s,
                    ref v if self.use_toon => {
                        toon_format::encode_default(v).unwrap_or_else(|_| v.to_string())
                    }
                    ref v => v.to_string(),
                };
                CallToolResult::success(vec![Content::text(text)])
            }
            Err(e) => CallToolResult::error(vec![Content::text(e)]),
        }
    }
}

#[tool_router]
impl CofeGraph {
    #[tool(description = "Re-index the project directory (use when source files have changed)")]
    async fn index_project(&self) -> CallToolResult {
        self.call_result(
            crate::tools::index::index_project(
                Arc::clone(&self.graph),
                &self.project_path,
                self.max_cache_entries,
            )
            .await,
        )
    }

    #[tool(
        description = "Search for functions, types (struct/union/enum/typedef), and symbols (#define, macros, enum values) by name (case-insensitive substring match). Use kind=function|type|symbol to filter."
    )]
    async fn search(&self, params: Parameters<SearchParams>) -> CallToolResult {
        let Parameters(p) = params;
        self.call_result(crate::tools::search::search(
            &*self.graph.read().await,
            &self.project_path,
            p,
        ))
    }

    #[tool(
        description = "Get all functions that call the given functions (BFS up to depth). Pass multiple names to batch related lookups into one call."
    )]
    async fn get_callers(&self, params: Parameters<CallersParams>) -> CallToolResult {
        let Parameters(p) = params;
        self.call_result(crate::tools::traverse::get_callers(
            &*self.graph.read().await,
            p,
        ))
    }

    #[tool(
        description = "Get all functions called by the given functions (BFS up to depth). Pass multiple names to batch related lookups into one call."
    )]
    async fn get_callees(&self, params: Parameters<CalleesParams>) -> CallToolResult {
        let Parameters(p) = params;
        self.call_result(crate::tools::traverse::get_callees(
            &*self.graph.read().await,
            p,
        ))
    }

    #[tool(description = "Get the source code of a function by exact name")]
    async fn get_source(&self, params: Parameters<GetSourceParams>) -> CallToolResult {
        let Parameters(p) = params;
        self.call_result(crate::tools::functions::get_source(
            &*self.graph.read().await,
            p,
        ))
    }

    #[tool(description = "Find the shortest call path from one function to another")]
    async fn get_path(&self, params: Parameters<GetPathParams>) -> CallToolResult {
        let Parameters(p) = params;
        self.call_result(crate::tools::traverse::get_path(
            &*self.graph.read().await,
            p,
        ))
    }

    #[tool(
        description = "List functions that are never called (potential dead code), classified by likely reason"
    )]
    async fn find_dead_code(&self) -> CallToolResult {
        self.call_result(crate::tools::analysis::find_dead_code(
            &*self.graph.read().await,
        ))
    }

    #[tool(
        description = "List all functions in files matching a filename substring, grouped by file path. Each entry contains name/line/is_static."
    )]
    async fn find_functions_in_file(&self, params: Parameters<FindInFileParams>) -> CallToolResult {
        let Parameters(p) = params;
        self.call_result(crate::tools::functions::find_functions_in_file(
            &*self.graph.read().await,
            &self.project_path,
            p,
        ))
    }

    #[tool(
        description = "List file-scope (global) variable declarations in files matching a filename substring"
    )]
    async fn get_globals(&self, params: Parameters<GetIncludesParams>) -> CallToolResult {
        let Parameters(p) = params;
        self.call_result(crate::tools::globals::get_globals(
            &*self.graph.read().await,
            &self.project_path,
            p,
        ))
    }

    #[tool(
        description = "Find all functions that reference a global variable by name (word-boundary match in function source)"
    )]
    async fn get_global_users(&self, params: Parameters<GetTypeUsersParams>) -> CallToolResult {
        let Parameters(p) = params;
        self.call_result(crate::tools::globals::get_global_users(
            &*self.graph.read().await,
            &self.project_path,
            p,
        ))
    }

    #[tool(
        description = "Find all functions that reference a given type name in their source (exact word match)"
    )]
    async fn get_type_users(&self, params: Parameters<GetTypeUsersParams>) -> CallToolResult {
        let Parameters(p) = params;
        self.call_result(crate::tools::types::get_type_users(
            &*self.graph.read().await,
            &self.project_path,
            p,
        ))
    }

    #[tool(description = "List all #include directives in files matching a filename substring")]
    async fn get_includes(&self, params: Parameters<GetIncludesParams>) -> CallToolResult {
        let Parameters(p) = params;
        self.call_result(crate::tools::includes::get_includes(
            &*self.graph.read().await,
            p,
        ))
    }

    #[tool(
        description = "Find all files that #include a given header (substring match on the include path)"
    )]
    async fn get_includers(&self, params: Parameters<GetIncludersParams>) -> CallToolResult {
        let Parameters(p) = params;
        self.call_result(crate::tools::includes::get_includers(
            &*self.graph.read().await,
            p,
        ))
    }

    #[tool(
        description = "List the top N functions by fan-in (most callers) — these are shared utilities or hotspots"
    )]
    async fn find_high_fan_in(&self, params: Parameters<TopNParams>) -> CallToolResult {
        let Parameters(p) = params;
        self.call_result(crate::tools::functions::find_high_fan_in(
            &*self.graph.read().await,
            p,
        ))
    }

    #[tool(
        description = "Write a semantic annotation for a file (subsystem, summary, key functions). Stores the current file hash for staleness detection. Path must match exactly one indexed file."
    )]
    async fn annotate_file(&self, params: Parameters<AnnotateFileParams>) -> CallToolResult {
        let Parameters(p) = params;
        let graph = self.graph.read().await;
        let mut store = self.annotations.write().await;
        self.call_result(crate::tools::annotate::annotate_file(&graph, &mut store, p))
    }

    #[tool(
        description = "Retrieve the stored annotation(s) for files matching a path substring, with a stale flag if the file has changed since annotation"
    )]
    async fn get_file_annotation(&self, params: Parameters<FilePathParams>) -> CallToolResult {
        let Parameters(p) = params;
        let graph = self.graph.read().await;
        let store = self.annotations.read().await;
        self.call_result(crate::tools::annotate::get_file_annotation(
            &graph, &store, p,
        ))
    }

    #[tool(description = "List all annotated files with their subsystem and staleness status")]
    async fn list_file_annotations(&self) -> CallToolResult {
        let graph = self.graph.read().await;
        let store = self.annotations.read().await;
        self.call_result(crate::tools::annotate::list_file_annotations(
            &graph, &store,
        ))
    }

    #[tool(
        description = "Return a full analysis bundle for a single file: all functions with source and call statistics, globals, types, and any existing annotation. Use this to gather context before calling annotate_file. Path must match exactly one indexed file."
    )]
    async fn get_file_context(&self, params: Parameters<FileContextParams>) -> CallToolResult {
        let Parameters(p) = params;
        let graph = self.graph.read().await;
        let store = self.annotations.read().await;
        self.call_result(crate::tools::annotate::get_file_context(&graph, &store, p))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for CofeGraph {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions("GraphRAG tools for C code analysis. The project is indexed automatically at startup. Use index_project to re-index after source changes.")
    }
}
