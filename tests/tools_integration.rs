mod tools {
    use std::path::Path;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    use cofe_graph::graph::CallGraph;
    use cofe_graph::tools::index::index_sources;

    async fn build_graph(sources: &[(&str, &str)]) -> Arc<RwLock<CallGraph>> {
        let graph = Arc::new(RwLock::new(CallGraph::default()));
        let pairs: Vec<(&Path, &str)> = sources.iter().map(|(p, s)| (Path::new(p), *s)).collect();
        index_sources(Arc::clone(&graph), &pairs).await;
        graph
    }

    mod analysis;
    mod annotate;
    mod functions;
    mod globals;
    mod includes;
    mod symbols;
    mod traverse;
    mod types;
}
