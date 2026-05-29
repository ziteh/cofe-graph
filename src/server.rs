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
use rmcp::schemars;
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{ServerHandler, prompt, prompt_handler, prompt_router, tool, tool_handler, tool_router};
use tokio::sync::RwLock;

use crate::annotations::AnnotationStore;
use crate::graph::CodebaseGraph;
use crate::tools::annotate::{
    AnnotateFileParams, AnnotateSymbolParams, FileContextParams, GetAnnotationsParams,
    ListUnannotatedParams,
};
use crate::tools::functions::{FindInFileParams, GetSourceParams};
use crate::tools::globals::FindUsersParams;
use crate::tools::includes::IncludesParams;
use crate::tools::search::SearchParams;
use crate::tools::traverse::{GetPathParams, TraverseParams};

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct ExploreArgs {
    /// Optional focus: a file name, subsystem, or function name to centre the exploration on.
    pub focus: Option<String>,
}

#[derive(Clone)]
pub struct GraphAnalyzer {
    pub(crate) graph: Arc<RwLock<CodebaseGraph>>,
    pub(crate) annotations: Arc<RwLock<AnnotationStore>>,
    pub(crate) project_path: PathBuf,
    use_toon: bool,
    max_l1_entries: usize,
    max_l2_entries: usize,
    tool_router: ToolRouter<Self>,
    prompt_router: PromptRouter<Self>,
}

impl GraphAnalyzer {
    pub fn new(
        path: PathBuf,
        use_toon: bool,
        max_l1_entries: usize,
        max_l2_entries: usize,
    ) -> Self {
        let graph = Arc::new(RwLock::new(CodebaseGraph::default()));
        let annotations = Arc::new(RwLock::new(AnnotationStore::load(&path)));
        let g = Arc::clone(&graph);
        let p = path.clone();
        tokio::spawn(async move {
            let _ = crate::tools::index::index_project(g, &p, max_l1_entries, max_l2_entries).await;
        });
        Self {
            graph,
            annotations,
            project_path: path,
            use_toon,
            max_l1_entries,
            max_l2_entries,
            tool_router: Self::tool_router(),
            prompt_router: Self::prompt_router(),
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

#[prompt_router]
impl GraphAnalyzer {
    #[prompt(
        name = "explore_codebase",
        description = "Step-by-step instructions for an AI agent to explore an indexed C codebase with analysis tools and produce a structured summary."
    )]
    async fn explore_codebase(
        &self,
        Parameters(args): Parameters<ExploreArgs>,
    ) -> Result<Vec<PromptMessage>, rmcp::ErrorData> {
        let focus_preamble = if let Some(ref f) = args.focus {
            format!(
                "**Focused exploration mode**: concentrate on `{f}` (file, subsystem, or \
                 function). Apply each step below with that scope in mind; call tools with \
                 arguments that match or are related to `{f}`. Still produce a full summary at \
                 the end but emphasise the focused area.\n\n"
            )
        } else {
            String::new()
        };

        let body = format!(
            r#"{focus_preamble}You have access to an MCP server that has already indexed \
a C codebase. Follow these steps in order to understand the codebase and write a \
structured summary. Call tools as you go; do not skip steps.

## Step 1 — Verify the index
Call `index_project`. Note the returned file, function, type, and symbol counts. \
If it reports 0 files, the index is empty — stop and ask the user to check the project path.

## Step 2 — Broad overview
From the Step 1 response extract: total files, total functions, total types, total symbols. \
Then call `find_dead_code`. Skim the dead-code list to get a sense of the codebase health.

## Step 3 — Module structure
Identify the top 5–10 most important source files (by function count or by filename heuristics \
such as `main`, `init`, `core`, `app`).
For each important file:
- Call `find_functions_in_file` with the filename substring.
- Call `get_file_context` with the exact relative path.
- Call `includes direction="outbound"` to see its dependencies.

## Step 4 — Entry points and call flow
Search for likely entry points: call `search` with queries like `main`, `init`, `start`, `run`, \
`task`, `handler`.
For each entry-point function found:
- Call `traverse direction="callees" depth=3` to map what it calls.
- Call `traverse direction="callers"` to confirm it is a root.
Pick the 2–3 most significant call chains and call `get_path` between distant pairs.

## Step 5 — Key types and globals
Call `search kind="type"` to list all struct/union/enum/typedef definitions.
For each frequently-used type (appears in many functions):
- Call `find_users kind="type"` to see which functions use it.
For the most important global variables:
- Call `get_globals` with a relevant filename substring.
- Call `find_users kind="global"` to see which functions read or write it.

## Step 6 — Read key source
Choose 3–5 of the most central or interesting functions identified so far.
Call `get_source` on each. Read the implementation to understand the core logic.

## Step 7 — Produce the summary
Write a structured summary with exactly these sections:

### 1. Project Overview
One paragraph: what the project does, language/platform, approximate size \
(files / functions / types).

### 2. Module Breakdown
A table or bullet list: filename → responsibility (one sentence each).

### 3. Entry Points and Startup Flow
Describe how the program starts and what the main execution paths are.

### 4. Key Data Structures
List the most important structs/enums/types with a one-sentence description of their role.

### 5. Core Algorithms and Subsystems
Describe 2–5 key algorithms or subsystems: what they do and which functions implement them.

### 6. Dead Code and Maintenance Notes
Summarise the dead-code findings: counts by category, notable suspicious entries.

### 7. Patterns and Conventions
Note any recurring patterns: naming conventions, error-handling style, memory management \
approach, use of macros, ISR/interrupt patterns, RTOS primitives, etc.
"#
        );

        Ok(vec![PromptMessage::new_text(PromptMessageRole::User, body)])
    }
}

#[tool_router]
impl GraphAnalyzer {
    #[tool(description = "Re-index the project directory (use when source files have changed)")]
    async fn index_project(&self) -> CallToolResult {
        self.call_result(
            crate::tools::index::index_project(
                Arc::clone(&self.graph),
                &self.project_path,
                self.max_l1_entries,
                self.max_l2_entries,
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
        .with_instructions("GraphRAG tools for C code analysis. The project is indexed automatically at startup. Use index_project to re-index after source changes. Usage guides are available as MCP resources: graph://quick-reference, graph://rules-of-thumb, graph://workflows")
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
