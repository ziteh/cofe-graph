use serde_json::Value;

use cofe_graph::tools::functions::{
    FindFunctionParams, FindInFileParams, find_function, find_functions_in_file,
};

#[tokio::test]
async fn test_find_function() {
    let graph_lock = super::build_graph(&[(
        "test.c",
        concat!(
            "void process_data(void) {}\n",
            "int main(void) { process_data(); return 0; }"
        ),
    )])
    .await;
    let graph = graph_lock.read().await;

    let params = FindFunctionParams {
        name: "process_data".to_string(),
    };
    let result = find_function(&graph, params);

    let v: Value = serde_json::from_str(&result).unwrap();
    assert_eq!(v["isError"], false);
    let content = v["content"].as_array().unwrap();

    assert_eq!(
        content.len(),
        1,
        "Should find exactly one 'process_data' function"
    );
    assert_eq!(content[0]["name"], "process_data");
    assert_eq!(content[0]["is_static"], false);
}

#[tokio::test]
async fn test_find_functions_in_file() {
    let graph_lock = super::build_graph(&[(
        "module.c",
        concat!(
            "static void helper(int n) {}\n",
            "void init(void) { helper(100); }\n",
            "void work(int x) {}"
        ),
    )])
    .await;
    let graph = graph_lock.read().await;

    let params = FindInFileParams {
        filename: "module.c".to_string(),
    };
    let result = find_functions_in_file(&graph, params);

    let v: Value = serde_json::from_str(&result).unwrap();
    assert_eq!(v["isError"], false);
    let content = v["content"].as_array().unwrap();

    assert_eq!(content.len(), 3, "Should find helper, init, work");

    let helper = content.iter().find(|m| m["name"] == "helper").unwrap();
    assert_eq!(helper["is_static"], true, "helper should be static");

    let work = content.iter().find(|m| m["name"] == "work").unwrap();
    assert_eq!(work["is_static"], false, "work should be public");
}
