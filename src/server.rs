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
use crate::tools::functions::{FindFunctionParams, FindInFileParams, GetSourceParams, TopNParams};
use crate::tools::includes::{GetIncludersParams, GetIncludesParams};
use crate::tools::symbols::FindSymbolParams;
use crate::tools::traverse::{GetPathParams, TraverseParams};
use crate::tools::types::{FindTypeParams, GetTypeUsersParams};

#[derive(Clone)]
pub struct CofeGraph {
    graph: Arc<RwLock<CallGraph>>,
    annotations: Arc<RwLock<AnnotationStore>>,
    project_path: PathBuf,
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
                let text = if self.use_toon {
                    toon_format::encode_default(&value).unwrap_or_else(|_| value.to_string())
                } else {
                    value.to_string()
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

    #[tool(description = "Search for functions by name (case-insensitive substring match)")]
    async fn find_function(&self, params: Parameters<FindFunctionParams>) -> CallToolResult {
        let Parameters(p) = params;
        self.call_result(crate::tools::functions::find_function(
            &*self.graph.read().await,
            p,
        ))
    }

    #[tool(description = "Get all functions that call the given function (BFS up to depth)")]
    async fn get_callers(&self, params: Parameters<TraverseParams>) -> CallToolResult {
        let Parameters(p) = params;
        self.call_result(crate::tools::traverse::get_callers(
            &*self.graph.read().await,
            p,
        ))
    }

    #[tool(description = "Get all functions called by the given function (BFS up to depth)")]
    async fn get_callees(&self, params: Parameters<TraverseParams>) -> CallToolResult {
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
        description = "Show overall call graph statistics: function count, edge count, and top fan-in/fan-out functions"
    )]
    async fn get_stats(&self) -> CallToolResult {
        self.call_result(crate::tools::analysis::get_stats(&*self.graph.read().await))
    }

    #[tool(
        description = "List all functions defined in files matching a filename substring (case-insensitive)"
    )]
    async fn find_functions_in_file(&self, params: Parameters<FindInFileParams>) -> CallToolResult {
        let Parameters(p) = params;
        self.call_result(crate::tools::functions::find_functions_in_file(
            &*self.graph.read().await,
            p,
        ))
    }

    #[tool(
        description = "List non-static (public) functions in files matching a filename substring — shows the API surface of a module"
    )]
    async fn get_public_api(&self, params: Parameters<FindInFileParams>) -> CallToolResult {
        let Parameters(p) = params;
        self.call_result(crate::tools::functions::get_public_api(
            &*self.graph.read().await,
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
            p,
        ))
    }

    #[tool(
        description = "List all global variables referenced in a function's source (word-boundary match)"
    )]
    async fn get_fn_globals(&self, params: Parameters<GetSourceParams>) -> CallToolResult {
        let Parameters(p) = params;
        self.call_result(crate::tools::globals::get_fn_globals(
            &*self.graph.read().await,
            p,
        ))
    }

    #[tool(
        description = "Look up struct, union, enum, and typedef definitions by name (case-insensitive substring match)"
    )]
    async fn find_type(&self, params: Parameters<FindTypeParams>) -> CallToolResult {
        let Parameters(p) = params;
        self.call_result(crate::tools::types::find_type(&*self.graph.read().await, p))
    }

    #[tool(
        description = "Find all functions that reference a given type name in their source (exact word match)"
    )]
    async fn get_type_users(&self, params: Parameters<GetTypeUsersParams>) -> CallToolResult {
        let Parameters(p) = params;
        self.call_result(crate::tools::types::get_type_users(
            &*self.graph.read().await,
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
        description = "Look up #define constants, function-like macros, and enum values by name (case-insensitive substring match)"
    )]
    async fn find_symbol(&self, params: Parameters<FindSymbolParams>) -> CallToolResult {
        let Parameters(p) = params;
        self.call_result(crate::tools::symbols::find_symbol(
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
        description = "List indexed files that have no annotation yet, ordered by function count (most functions first)"
    )]
    async fn list_unannotated_files(&self) -> CallToolResult {
        let graph = self.graph.read().await;
        let store = self.annotations.read().await;
        self.call_result(crate::tools::annotate::list_unannotated_files(
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
