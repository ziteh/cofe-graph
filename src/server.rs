use std::path::PathBuf;
use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Content, ServerCapabilities, ServerInfo};
use rmcp::{ServerHandler, tool, tool_handler, tool_router};
use tokio::sync::RwLock;

use crate::annotations::AnnotationStore;
use crate::graph::CallGraph;
use crate::tools::annotate::{
    AnnotateFileParams, AnnotateSymbolParams, FileContextParams, GetAnnotationsParams,
    ListUnannotatedParams,
};
use crate::tools::functions::{FindInFileParams, GetSourceParams};
use crate::tools::globals::FindUsersParams;
use crate::tools::includes::IncludesParams;
use crate::tools::search::SearchParams;
use crate::tools::traverse::{GetPathParams, TraverseParams};

#[derive(Clone)]
pub struct CofeGraph {
    pub(crate) graph: Arc<RwLock<CallGraph>>,
    pub(crate) annotations: Arc<RwLock<AnnotationStore>>,
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
        description = "Get callers or callees of the given functions (BFS up to depth). direction=\"callers\" returns who calls them; direction=\"callees\" returns what they call. Pass multiple names to batch."
    )]
    async fn traverse(&self, params: Parameters<TraverseParams>) -> CallToolResult {
        let Parameters(p) = params;
        self.call_result(crate::tools::traverse::traverse(
            &*self.graph.read().await,
            p,
        ))
    }

    #[tool(
        description = "Get the source code of a function by exact name; includes any stored semantic annotation"
    )]
    async fn get_source(&self, params: Parameters<GetSourceParams>) -> CallToolResult {
        let Parameters(p) = params;
        let store = self.annotations.read().await;
        self.call_result(crate::tools::functions::get_source(
            &*self.graph.read().await,
            &store,
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
    async fn get_globals(&self, params: Parameters<FindInFileParams>) -> CallToolResult {
        let Parameters(p) = params;
        self.call_result(crate::tools::globals::get_globals(
            &*self.graph.read().await,
            &self.project_path,
            p,
        ))
    }

    #[tool(
        description = "Find all functions that reference a global variable or type name in their source. kind=\"global\" for variable references, kind=\"type\" for struct/enum/typedef references (exact word match)."
    )]
    async fn find_users(&self, params: Parameters<FindUsersParams>) -> CallToolResult {
        let Parameters(p) = params;
        self.call_result(crate::tools::globals::find_users(
            &*self.graph.read().await,
            &self.project_path,
            p,
        ))
    }

    #[tool(
        description = "Query #include relationships. direction=\"outbound\": list all #includes in files matching target filename; direction=\"inbound\": find all files that #include the target header."
    )]
    async fn includes(&self, params: Parameters<IncludesParams>) -> CallToolResult {
        let Parameters(p) = params;
        self.call_result(crate::tools::includes::includes(
            &*self.graph.read().await,
            p,
        ))
    }

    #[tool(
        description = "Write a semantic annotation for a file (subsystem, summary, notes). Stores the current file hash for staleness detection. Path must match exactly one indexed file."
    )]
    async fn annotate_file(&self, params: Parameters<AnnotateFileParams>) -> CallToolResult {
        let Parameters(p) = params;
        let graph = self.graph.read().await;
        let mut store = self.annotations.write().await;
        self.call_result(crate::tools::annotate::annotate_file(&graph, &mut store, p))
    }

    #[tool(
        description = "Write a semantic annotation for a specific function, type, or global variable. Use insight to capture interrupt context, ownership rules, invariants, or design intent that tree-sitter cannot infer."
    )]
    async fn annotate_symbol(&self, params: Parameters<AnnotateSymbolParams>) -> CallToolResult {
        let Parameters(p) = params;
        let graph = self.graph.read().await;
        let mut store = self.annotations.write().await;
        self.call_result(crate::tools::annotate::annotate_symbol(
            &graph, &mut store, p,
        ))
    }

    #[tool(
        description = "Get annotation(s) for matching files with stale flag, or list all annotated files if path is omitted"
    )]
    async fn get_annotations(&self, params: Parameters<GetAnnotationsParams>) -> CallToolResult {
        let Parameters(p) = params;
        let graph = self.graph.read().await;
        let store = self.annotations.read().await;
        self.call_result(crate::tools::annotate::get_annotations(&graph, &store, p))
    }

    #[tool(
        description = "List unannotated items. Without 'file': list files with no annotation yet, sorted by function count. With 'file': list functions in that file that have no symbol annotation for their current source."
    )]
    async fn list_unannotated(&self, params: Parameters<ListUnannotatedParams>) -> CallToolResult {
        let Parameters(p) = params;
        let graph = self.graph.read().await;
        let store = self.annotations.read().await;
        self.call_result(crate::tools::annotate::list_unannotated(&graph, &store, p))
    }

    #[tool(
        description = "Return a full analysis bundle for a single file: all functions with call statistics, globals, types, and any existing annotation. Use get_source to fetch individual function source. Path must match exactly one indexed file."
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
