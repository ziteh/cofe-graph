use serde_json::Value;

use cofe_graph::tools::traverse::{GetPathParams, get_path};

#[tokio::test]
async fn test_get_path() {
    // Call chain: main -> task -> helper -> work
    let graph_lock = super::build_graph(&[(
        "code.c",
        concat!(
            "void work(int x) {}\n",
            "static void helper(void) { work(1); }\n",
            "void task(void) { helper(); }\n",
            "int main(void) { task(); return 0; }"
        ),
    )])
    .await;
    let graph = graph_lock.read().await;

    let params = GetPathParams {
        from: "main".to_string(),
        to: "work".to_string(),
    };
    let result = get_path(&graph, params);

    let v: Value = serde_json::from_str(&result).unwrap();
    assert_eq!(v["isError"], false);
    let path = v["content"]["path"].as_array().unwrap();

    assert!(path.len() >= 4, "Path should contain at least 4 nodes");
    assert_eq!(path[0], "main");
    assert_eq!(path[1], "task");
    assert_eq!(path[2], "helper");
    assert_eq!(path[3], "work");
}
