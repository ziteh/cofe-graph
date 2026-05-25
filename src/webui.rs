use std::sync::Arc;

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

use crate::server::CofeGraph;
use crate::tools::functions::{FindFunctionParams, GetSourceParams};

const HTML: &str = include_str!("webui.html");

pub async fn start(cofe: CofeGraph, port: u16) {
    let app = Router::new()
        .route("/", get(|| async { axum::response::Html(HTML) }))
        .route("/api/search", get(search_handler))
        .route("/api/source", get(source_handler))
        .with_state(Arc::new(cofe));

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .unwrap_or_else(|_| panic!("failed to bind webui port: {}", port));

    axum::serve(listener, app).await.unwrap();
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
}

#[derive(Deserialize)]
struct SourceQuery {
    name: String,
}

async fn search_handler(
    State(cofe): State<Arc<CofeGraph>>,
    Query(params): Query<SearchQuery>,
) -> Json<serde_json::Value> {
    let graph = cofe.graph.read().await;
    let result = crate::tools::functions::find_function(
        &graph,
        &cofe.project_path,
        FindFunctionParams { name: params.q },
    );
    Json(result.unwrap_or_else(|e| serde_json::json!({ "error": e })))
}

async fn source_handler(
    State(cofe): State<Arc<CofeGraph>>,
    Query(params): Query<SourceQuery>,
) -> Json<serde_json::Value> {
    let graph = cofe.graph.read().await;
    let result = crate::tools::functions::get_source(&graph, GetSourceParams { name: params.name });
    Json(result.unwrap_or_else(|e| serde_json::json!({ "error": e })))
}
