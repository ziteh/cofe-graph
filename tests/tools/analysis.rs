use cofe_graph::tools::analysis::{find_dead_code, get_stats};

#[tokio::test]
async fn test_find_dead_code() {
    let graph_lock = super::build_graph(&[(
        "code.c",
        concat!(
            "void unused(void) {}\n",
            "void called(void) {}\n",
            "int main(void) { called(); return 0; }"
        ),
    )])
    .await;
    let graph = graph_lock.read().await;

    let v = find_dead_code(&graph, std::path::Path::new("")).unwrap();
    assert!(v.get("summary").is_some(), "Should contain summary");
}

#[tokio::test]
async fn test_get_stats() {
    let graph_lock = super::build_graph(&[(
        "code.c",
        concat!(
            "void a(void) {}\n",
            "void b(void) { a(); }\n",
            "int main(void) { b(); return 0; }"
        ),
    )])
    .await;
    let graph = graph_lock.read().await;

    let v = get_stats(&graph).unwrap();
    assert!(v.get("functions").is_some());
}
