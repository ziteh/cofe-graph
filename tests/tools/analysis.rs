use serde_json::Value;

use cofe_graph::tools::analysis::{find_dead_code, get_stats};

#[tokio::test]
async fn test_find_dead_code() {
    // unused is never called — should appear in dead code report.
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

    let result = find_dead_code(&graph);
    let v: Value = serde_json::from_str(&result).unwrap();
    assert_eq!(v["isError"], false);
    assert!(v["content"].get("summary").is_some(), "Should contain summary");
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

    let result = get_stats(&graph);
    let v: Value = serde_json::from_str(&result).unwrap();
    assert_eq!(v["isError"], false);
    assert!(v["content"].get("functions").is_some());
}
