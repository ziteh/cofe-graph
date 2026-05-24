use cofe_graph::tools::symbols::{FindSymbolParams, find_symbol};

#[tokio::test]
async fn test_find_symbol() {
    let graph_lock = super::build_graph(&[("header.h", "#define CONST_VAL 5")]).await;
    let graph = graph_lock.read().await;

    let params = FindSymbolParams {
        name: "CONST_VAL".to_string(),
    };
    let matches = find_symbol(&graph, params).unwrap();
    let content = matches.as_array().expect("Expected array");

    assert_eq!(content.len(), 1);
    assert_eq!(content[0]["name"], "CONST_VAL");
    assert!(content[0]["file"].as_str().unwrap().ends_with("header.h"));
}
