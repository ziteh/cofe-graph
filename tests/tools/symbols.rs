use serde_json::Value;

use cofe_graph::tools::symbols::{FindSymbolParams, find_symbol};

#[tokio::test]
async fn test_find_symbol() {
    let graph_lock = super::build_graph(&[("header.h", "#define CONST_VAL 5")]).await;
    let graph = graph_lock.read().await;

    let params = FindSymbolParams {
        name: "CONST_VAL".to_string(),
    };
    let result = find_symbol(&graph, params);

    let v: Value = serde_json::from_str(&result).unwrap();
    let matches = v["matches"].as_array().expect("Expected 'matches' array");

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0]["name"], "CONST_VAL");
    assert!(matches[0]["file"].as_str().unwrap().ends_with("header.h"));
}
