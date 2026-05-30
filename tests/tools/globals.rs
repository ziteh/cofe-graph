use cofe_graph::annotations::AnnotationStore;
use cofe_graph::tools::functions::FindInFileParams;
use cofe_graph::tools::globals::get_globals;

#[tokio::test]
async fn test_get_globals() {
    let graph_lock = super::build_graph(&[(
        "app.c",
        concat!("int g_var = 0;\n", "int main(void) { return 0; }"),
    )])
    .await;
    let graph = graph_lock.read().await;

    let params = FindInFileParams {
        filename: "app.c".to_string(),
    };
    let v = get_globals(
        &graph,
        &AnnotationStore::default(),
        std::path::Path::new(""),
        params,
    )
    .unwrap();
    let content = v.as_array().expect("Expected array with global variable");
    assert!(content.iter().any(|m| m["name"] == "g_var"));
}
