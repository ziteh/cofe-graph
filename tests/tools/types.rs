use cofe_graph::tools::types::{FindTypeParams, GetTypeUsersParams, find_type, get_type_users};

const TYPES_SRC: (&str, &str) = ("src.c", "typedef struct { int x; int y; } state_t;");

#[tokio::test]
async fn test_find_type() {
    let graph_lock = super::build_graph(&[TYPES_SRC]).await;
    let graph = graph_lock.read().await;

    let params = FindTypeParams {
        name: "state_t".to_string(),
    };
    let matches = find_type(&graph, params).unwrap();
    let content = matches.as_array().expect("Expected array");

    assert!(content.iter().any(|m| m["name"] == "state_t"));
}

#[tokio::test]
async fn test_get_type_users() {
    let graph_lock = super::build_graph(&[TYPES_SRC]).await;
    let graph = graph_lock.read().await;

    let params = GetTypeUsersParams {
        name: "state_t".to_string(),
    };
    match get_type_users(&graph, params) {
        Err(e) => assert!(e.contains("No functions reference type")),
        Ok(v) => assert!(v.is_array()),
    }
}
