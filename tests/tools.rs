mod tools {
    use std::path::Path;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    use cofe_graph::graph::CodebaseGraph;
    use cofe_graph::tools::index::index_sources;

    async fn build_graph(sources: &[(&str, &str)]) -> Arc<RwLock<CodebaseGraph>> {
        let graph = Arc::new(RwLock::new(CodebaseGraph::default()));
        let pairs: Vec<(&Path, &str)> = sources.iter().map(|(p, s)| (Path::new(p), *s)).collect();
        let _ = index_sources(Arc::clone(&graph), &pairs).await;
        graph
    }

    mod analysis;
    mod annotate;
    mod functions;
    mod globals;
    mod includes;
    mod search;
    mod symbols;
    mod traverse;
    mod types;
}
