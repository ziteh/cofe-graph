//! MCP server

use std::path::PathBuf;
use std::sync::Arc;

use rmcp::handler::server::router::prompt::PromptRouter;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, Content, GetPromptRequestParams, GetPromptResult, ListPromptsResult,
    ListResourcesResult, PaginatedRequestParams, PromptMessage, PromptMessageRole, RawResource,
    ReadResourceRequestParams, ReadResourceResult, Resource, ResourceContents, ServerCapabilities,
    ServerInfo,
};
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{ServerHandler, prompt, prompt_handler, prompt_router, tool, tool_handler, tool_router};
use tokio::sync::RwLock;

const EXPLORE_TEMPLATE: &str = include_str!("prompts/explore_codebase.md");

use crate::annotations::AnnotationStore;
use crate::graph::CodebaseGraph;
use crate::tools::annotations::{
    AnnotateModuleParams, AnnotateParams, GetAnnotationsParams, ListUnannotatedParams,
};
use crate::tools::functions::{FindInFileParams, GetSourceParams};
use crate::tools::globals::FindUsersParams;
use crate::tools::includes::IncludesParams;
use crate::tools::search::SearchParams;
use crate::tools::traverse::{GetPathParams, TraverseParams};

#[derive(Clone)]
pub struct GraphAnalyzer {
    pub(crate) graph: Arc<RwLock<CodebaseGraph>>,
    pub(crate) annotation_store: Arc<RwLock<AnnotationStore>>,
    pub(crate) project_path: PathBuf,
    use_toon: bool,
    cache_overhead_pct: usize,
    /// Timestamp captured just before each index run starts. UNIX_EPOCH means
    /// not yet indexed — any mtime comparison will appear stale.
    indexed_at: Arc<std::sync::Mutex<std::time::SystemTime>>,
    /// Ensures only one re-index runs at a time.
    index_lock: Arc<tokio::sync::Mutex<()>>,
    tool_router: ToolRouter<Self>,
    prompt_router: PromptRouter<Self>,
}

impl GraphAnalyzer {
    pub fn new(path: PathBuf, use_toon: bool, cache_overhead_pct: usize) -> Self {
        let graph = Arc::new(RwLock::new(CodebaseGraph::default()));
        let annotation_store = Arc::new(RwLock::new(AnnotationStore::load(&path)));
        let indexed_at = Arc::new(std::sync::Mutex::new(std::time::SystemTime::UNIX_EPOCH));
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
            annotation_store,
            project_path: path,
            use_toon,
            cache_overhead_pct,
            indexed_at,
            index_lock,
            tool_router: Self::tool_router(),
            prompt_router: Self::prompt_router(),
        }
    }

    /// Re-index if any .c/.h file has been modified since the last index run.
    /// Uses a mutex to ensure only one re-index runs concurrently.
    async fn ensure_fresh(&self) {
        let since = *self.indexed_at.lock().unwrap();
        if !crate::tools::index::has_stale_files(&self.project_path, since) {
            return;
        }
        let _guard = self.index_lock.lock().await;
        // Re-check after acquiring lock — another task may have just re-indexed.
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

#[prompt_router]
impl GraphAnalyzer {
    #[prompt(
        name = "explore_codebase",
        description = "Step-by-step instructions for an AI agent to explore an indexed C codebase with analysis tools and produce a structured summary."
    )]
    async fn explore_codebase(&self) -> Result<Vec<PromptMessage>, rmcp::ErrorData> {
        Ok(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            EXPLORE_TEMPLATE,
        )])
    }
}

#[tool_router]
impl GraphAnalyzer {
    #[tool(
        description = "Force a full re-index and return indexing statistics (files_ok, files_err, l2_hits, functions, call_edges). Normally not needed — every tool call re-indexes automatically when source files have changed."
    )]
    async fn index_project(&self) -> CallToolResult {
        let _guard = self.index_lock.lock().await;
        let t = std::time::SystemTime::now();
        let result = crate::tools::index::index_project(
            Arc::clone(&self.graph),
            &self.project_path,
            self.cache_overhead_pct,
        )
        .await;
        *self.indexed_at.lock().unwrap() = t;
        self.call_result(result)
    }

    #[tool(
        description = "Search for functions, types (struct/union/enum/typedef), and symbols (#define, macros, enum values) by name (case-insensitive substring match). Use kind=function|type|symbol to filter."
    )]
    async fn search(&self, params: Parameters<SearchParams>) -> CallToolResult {
        self.ensure_fresh().await;
        let Parameters(p) = params;
        let store = self.annotation_store.read().await;
        self.call_result(crate::tools::search::search(
            &*self.graph.read().await,
            &store,
            &self.project_path,
            p,
        ))
    }

    #[tool(
        description = "Get callers or callees of the given functions (BFS up to depth). direction=\"callers\" returns who calls them; direction=\"callees\" returns what they call. Pass multiple names to batch."
    )]
    async fn traverse(&self, params: Parameters<TraverseParams>) -> CallToolResult {
        self.ensure_fresh().await;
        let Parameters(p) = params;
        self.call_result(crate::tools::traverse::traverse(
            &*self.graph.read().await,
            p,
        ))
    }

    #[tool(description = "Get the source code of a function by exact name")]
    async fn get_source(&self, params: Parameters<GetSourceParams>) -> CallToolResult {
        self.ensure_fresh().await;
        let Parameters(p) = params;
        let store = self.annotation_store.read().await;
        self.call_result(crate::tools::functions::get_source(
            &*self.graph.read().await,
            &store,
            &self.project_path,
            p,
        ))
    }

    #[tool(description = "Find the shortest call path from one function to another")]
    async fn get_path(&self, params: Parameters<GetPathParams>) -> CallToolResult {
        self.ensure_fresh().await;
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
        self.ensure_fresh().await;
        self.call_result(crate::tools::analysis::find_dead_code(
            &*self.graph.read().await,
            &self.project_path,
        ))
    }

    #[tool(
        description = "List all functions in files matching a filename substring, grouped by file path. Each entry contains name/line/is_static."
    )]
    async fn find_functions_in_file(&self, params: Parameters<FindInFileParams>) -> CallToolResult {
        self.ensure_fresh().await;
        let Parameters(p) = params;
        let store = self.annotation_store.read().await;
        self.call_result(crate::tools::functions::find_functions_in_file(
            &*self.graph.read().await,
            &store,
            &self.project_path,
            p,
        ))
    }

    #[tool(
        description = "List file-scope (global) variable declarations in files matching a filename substring"
    )]
    async fn get_globals(&self, params: Parameters<FindInFileParams>) -> CallToolResult {
        self.ensure_fresh().await;
        let Parameters(p) = params;
        let store = self.annotation_store.read().await;
        self.call_result(crate::tools::globals::get_globals(
            &*self.graph.read().await,
            &store,
            &self.project_path,
            p,
        ))
    }

    #[tool(
        description = "Create or update a module annotation. A module is a logical grouping of files with a human-readable summary. Module annotations are not commit-aware and persist across commits."
    )]
    async fn annotate_module(&self, params: Parameters<AnnotateModuleParams>) -> CallToolResult {
        let Parameters(p) = params;
        let mut store = self.annotation_store.write().await;
        self.call_result(crate::tools::annotations::annotate_module(
            &mut store,
            &self.project_path,
            p,
        ))
    }

    #[tool(
        description = "Add or update a summary for a file or a specific symbol within a file. Omit `symbol` to annotate the file itself; include `symbol` to annotate a function, global variable, or #define. Annotations are keyed by git blob SHA and automatically disappear when the file changes on a different commit."
    )]
    async fn annotate(&self, params: Parameters<AnnotateParams>) -> CallToolResult {
        let Parameters(p) = params;
        let mut store = self.annotation_store.write().await;
        self.call_result(crate::tools::annotations::annotate(
            &mut store,
            &self.project_path,
            p,
        ))
    }

    #[tool(
        description = "Read annotations. kind=\"module\": list all module annotations. kind=\"file\": get annotation for a single file (requires `file`). kind=\"symbol\": get annotation for a symbol in a file (requires `file` and `symbol`). Returns null annotation when none exists or when the file has changed since the annotation was written."
    )]
    async fn get_annotations(&self, params: Parameters<GetAnnotationsParams>) -> CallToolResult {
        self.ensure_fresh().await;
        let Parameters(p) = params;
        let store = self.annotation_store.read().await;
        self.call_result(crate::tools::annotations::get_annotations(
            &store,
            &*self.graph.read().await,
            &self.project_path,
            p,
        ))
    }

    #[tool(
        description = "List items that have no annotation in the current index snapshot. kind=\"file\": unannotated source files. kind=\"function\": unannotated functions. kind=\"global\": unannotated global variables. Optional `filename_filter` narrows results to files whose path contains the substring (case-insensitive)."
    )]
    async fn list_unannotated(&self, params: Parameters<ListUnannotatedParams>) -> CallToolResult {
        self.ensure_fresh().await;
        let Parameters(p) = params;
        let store = self.annotation_store.read().await;
        self.call_result(crate::tools::annotations::list_unannotated(
            &store,
            &*self.graph.read().await,
            &self.project_path,
            p,
        ))
    }

    #[tool(
        description = "Find all functions that reference a global variable or type name in their source. kind=\"global\" for variable references, kind=\"type\" for struct/enum/typedef references (exact word match)."
    )]
    async fn find_users(&self, params: Parameters<FindUsersParams>) -> CallToolResult {
        self.ensure_fresh().await;
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
        self.ensure_fresh().await;
        let Parameters(p) = params;
        self.call_result(crate::tools::includes::includes(
            &*self.graph.read().await,
            p,
        ))
    }
}

#[prompt_handler(router = self.prompt_router)]
#[tool_handler(router = self.tool_router)]
impl ServerHandler for GraphAnalyzer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_prompts()
                .build(),
        )
        .with_instructions("GraphRAG tools for C code analysis. The project is indexed at startup and automatically re-indexed when source files change. Use index_project only to force a re-index or to check indexing statistics. Usage guides are available as MCP resources: graph://quick-reference, graph://rules-of-thumb, graph://workflows")
    }

    async fn list_resources(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> Result<ListResourcesResult, rmcp::ErrorData> {
        let items: Vec<Resource> = crate::resources::RESOURCES
            .iter()
            .map(|(uri, name, desc, _)| {
                Resource::new(
                    RawResource::new(*uri, *name)
                        .with_description(*desc)
                        .with_mime_type("text/markdown"),
                    None,
                )
            })
            .collect();
        Ok(ListResourcesResult::with_all_items(items))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> Result<ReadResourceResult, rmcp::ErrorData> {
        let body = crate::resources::RESOURCES
            .iter()
            .find(|(uri, _, _, _)| *uri == request.uri)
            .map(|(_, _, _, body)| *body)
            .ok_or_else(|| {
                rmcp::model::ErrorData::new(
                    rmcp::model::ErrorCode::RESOURCE_NOT_FOUND,
                    format!("Unknown resource: {}", request.uri),
                    None,
                )
            })?;
        let contents = vec![
            ResourceContents::text(body.to_owned(), &request.uri).with_mime_type("text/markdown"),
        ];
        Ok(ReadResourceResult::new(contents))
    }
}
