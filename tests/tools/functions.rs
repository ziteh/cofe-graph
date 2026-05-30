use cofe_graph::annotations::AnnotationStore;
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
    let matches = find_function(&graph, std::path::Path::new(""), params).unwrap();
    let content = matches.as_array().unwrap();

    assert_eq!(
        content.len(),
        1,
        "Should find exactly one 'process_data' function"
    );
    assert_eq!(content[0]["name"], "process_data");
    assert_eq!(content[0]["static"], false);
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
    // Single-file query: value is an object {file, functions:[...]}
    let store = AnnotationStore::default();
    let v = find_functions_in_file(&graph, &store, std::path::Path::new(""), params).unwrap();
    let fns = v["module.c"].as_array().unwrap();
    assert_eq!(fns.len(), 3, "Should find helper, init, work");

    let helper = fns.iter().find(|m| m["name"] == "helper").unwrap();
    assert_eq!(helper["static"], true);
    assert!(helper.get("file").is_none(), "file should not be in entry");

    let init = fns.iter().find(|m| m["name"] == "init").unwrap();
    assert_eq!(init["static"], false);

    let work = fns.iter().find(|m| m["name"] == "work").unwrap();
    assert_eq!(work["static"], false);
}
