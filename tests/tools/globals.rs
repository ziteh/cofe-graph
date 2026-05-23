use serde_json::Value;

use cofe_graph::tools::globals::get_globals;
use cofe_graph::tools::includes::GetIncludesParams;

#[tokio::test]
async fn test_get_globals() {
    let graph_lock = super::build_graph(&[(
        "app.c",
        concat!("int g_var = 0;\n", "int main(void) { return 0; }"),
    )])
    .await;
    let graph = graph_lock.read().await;

    let params = GetIncludesParams {
        file: "app.c".to_string(),
    };
    let result = get_globals(&graph, params);

    let v: Value = serde_json::from_str(&result).unwrap();
    if let Some(matches) = v["matches"].as_array() {
        assert!(matches.iter().any(|m| m["name"] == "g_var"));
    } else {
        panic!("Expected 'matches' array with global variable");
    }
}
