use serde_json::Value;

use cofe_graph::tools::types::{FindTypeParams, GetTypeUsersParams, find_type, get_type_users};

const TYPES_SRC: (&str, &str) = ("src.c", "typedef struct { int x; int y; } state_t;");

#[tokio::test]
async fn test_find_type() {
    let graph_lock = super::build_graph(&[TYPES_SRC]).await;
    let graph = graph_lock.read().await;

    let params = FindTypeParams {
        name: "state_t".to_string(),
    };
    let result = find_type(&graph, params);

    let v: Value = serde_json::from_str(&result).unwrap();
    let matches = v["matches"].as_array().expect("Expected 'matches' array");

    assert!(matches.iter().any(|m| m["name"] == "state_t"));
}

#[tokio::test]
async fn test_get_type_users() {
    let graph_lock = super::build_graph(&[TYPES_SRC]).await;
    let graph = graph_lock.read().await;

    let params = GetTypeUsersParams {
        name: "state_t".to_string(),
    };
    let result = get_type_users(&graph, params);

    let v: Value = serde_json::from_str(&result).unwrap();
    if v.get("error").is_some() {
        assert_eq!(
            v["error"].as_str().unwrap(),
            "No functions reference type 'state_t'"
        );
    } else {
        assert!(v.get("matches").is_some() || v.get("users").is_some());
    }
}
