mod tools;

use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{ServerCapabilities, ServerInfo};
use rmcp::{ServerHandler, tool, tool_handler, tool_router};
use tokio::sync::RwLock;

use crate::graph::CallGraph;
use tools::functions::{FindFunctionParams, FindInFileParams, GetSourceParams, TopNParams};
use tools::includes::{GetIncludersParams, GetIncludesParams};
use tools::index::IndexProjectParams;
use tools::symbols::FindSymbolParams;
use tools::traverse::{GetPathParams, TraverseParams};
use tools::types::{FindTypeParams, GetTypeUsersParams};

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
        let Parameters(p) = params;
        tools::index::index_project(Arc::clone(&self.graph), p).await
    }

    #[tool(description = "Search for functions by name (case-insensitive substring match)")]
    async fn find_function(&self, params: Parameters<FindFunctionParams>) -> String {
        let Parameters(p) = params;
        tools::functions::find_function(&*self.graph.read().await, p)
    }

    #[tool(description = "Get all functions that call the given function (BFS up to depth)")]
    async fn get_callers(&self, params: Parameters<TraverseParams>) -> String {
        let Parameters(p) = params;
        tools::traverse::get_callers(&*self.graph.read().await, p)
    }

    #[tool(description = "Get all functions called by the given function (BFS up to depth)")]
    async fn get_callees(&self, params: Parameters<TraverseParams>) -> String {
        let Parameters(p) = params;
        tools::traverse::get_callees(&*self.graph.read().await, p)
    }

    #[tool(description = "Get the source code of a function by exact name")]
    async fn get_source(&self, params: Parameters<GetSourceParams>) -> String {
        let Parameters(p) = params;
        tools::functions::get_source(&*self.graph.read().await, p)
    }

    #[tool(description = "Find the shortest call path from one function to another")]
    async fn get_path(&self, params: Parameters<GetPathParams>) -> String {
        let Parameters(p) = params;
        tools::traverse::get_path(&*self.graph.read().await, p)
    }

    #[tool(
        description = "List functions that are never called (potential dead code), classified by likely reason"
    )]
    async fn find_dead_code(&self) -> String {
        tools::analysis::find_dead_code(&*self.graph.read().await)
    }

    #[tool(
        description = "Show overall call graph statistics: function count, edge count, and top fan-in/fan-out functions"
    )]
    async fn get_stats(&self) -> String {
        tools::analysis::get_stats(&*self.graph.read().await)
    }

    #[tool(
        description = "List all functions defined in files matching a filename substring (case-insensitive)"
    )]
    async fn find_functions_in_file(&self, params: Parameters<FindInFileParams>) -> String {
        let Parameters(p) = params;
        tools::functions::find_functions_in_file(&*self.graph.read().await, p)
    }

    #[tool(
        description = "List non-static (public) functions in files matching a filename substring — shows the API surface of a module"
    )]
    async fn get_public_api(&self, params: Parameters<FindInFileParams>) -> String {
        let Parameters(p) = params;
        tools::functions::get_public_api(&*self.graph.read().await, p)
    }

    #[tool(
        description = "List file-scope (global) variable declarations in files matching a filename substring"
    )]
    async fn get_globals(&self, params: Parameters<GetIncludesParams>) -> String {
        let Parameters(p) = params;
        tools::globals::get_globals(&*self.graph.read().await, p)
    }

    #[tool(
        description = "Find all functions that reference a global variable by name (word-boundary match in function source)"
    )]
    async fn get_global_users(&self, params: Parameters<GetTypeUsersParams>) -> String {
        let Parameters(p) = params;
        tools::globals::get_global_users(&*self.graph.read().await, p)
    }

    #[tool(
        description = "List all global variables referenced in a function's source (word-boundary match)"
    )]
    async fn get_fn_globals(&self, params: Parameters<GetSourceParams>) -> String {
        let Parameters(p) = params;
        tools::globals::get_fn_globals(&*self.graph.read().await, p)
    }

    #[tool(
        description = "Look up struct, union, enum, and typedef definitions by name (case-insensitive substring match)"
    )]
    async fn find_type(&self, params: Parameters<FindTypeParams>) -> String {
        let Parameters(p) = params;
        tools::types::find_type(&*self.graph.read().await, p)
    }

    #[tool(
        description = "Find all functions that reference a given type name in their source (exact word match)"
    )]
    async fn get_type_users(&self, params: Parameters<GetTypeUsersParams>) -> String {
        let Parameters(p) = params;
        tools::types::get_type_users(&*self.graph.read().await, p)
    }

    #[tool(description = "List all #include directives in files matching a filename substring")]
    async fn get_includes(&self, params: Parameters<GetIncludesParams>) -> String {
        let Parameters(p) = params;
        tools::includes::get_includes(&*self.graph.read().await, p)
    }

    #[tool(
        description = "Find all files that #include a given header (substring match on the include path)"
    )]
    async fn get_includers(&self, params: Parameters<GetIncludersParams>) -> String {
        let Parameters(p) = params;
        tools::includes::get_includers(&*self.graph.read().await, p)
    }

    #[tool(
        description = "Look up #define constants, function-like macros, and enum values by name (case-insensitive substring match)"
    )]
    async fn find_symbol(&self, params: Parameters<FindSymbolParams>) -> String {
        let Parameters(p) = params;
        tools::symbols::find_symbol(&*self.graph.read().await, p)
    }

    #[tool(
        description = "List the top N functions by fan-in (most callers) — these are shared utilities or hotspots"
    )]
    async fn find_high_fan_in(&self, params: Parameters<TopNParams>) -> String {
        let Parameters(p) = params;
        tools::functions::find_high_fan_in(&*self.graph.read().await, p)
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for CofeGraph {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions("GraphRAG tools for C code analysis. Call index_project first, then use: find_function / get_callers / get_callees / get_source / get_path / find_dead_code / get_stats / find_functions_in_file / find_high_fan_in.")
    }
}
