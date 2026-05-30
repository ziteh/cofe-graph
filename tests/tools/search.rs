use cofe_graph::annotations::AnnotationStore;
use cofe_graph::tools::search::{SearchParams, search};

#[tokio::test]
async fn test_search_no_kind_finds_function() {
    let graph_lock = super::build_graph(&[(
        "test.c",
        "void render(void) {}\nvoid update(void) { render(); }\n",
    )])
    .await;
    let graph = graph_lock.read().await;

    let v = search(
        &graph,
        &AnnotationStore::default(),
        std::path::Path::new(""),
        SearchParams {
            name: "render".to_string(),
            kind: None,
        },
    )
    .unwrap();

    let fns = v["functions"].as_array().unwrap();
    assert_eq!(fns.len(), 1);
    assert_eq!(fns[0]["name"], "render");
    assert_eq!(fns[0]["static"], false);
}

#[tokio::test]
async fn test_search_kind_function_excludes_types() {
    let graph_lock = super::build_graph(&[(
        "test.c",
        "typedef struct { int x; } Vec;\nvoid vec_init(void) {}\n",
    )])
    .await;
    let graph = graph_lock.read().await;

    let v = search(
        &graph,
        &AnnotationStore::default(),
        std::path::Path::new(""),
        SearchParams {
            name: "vec".to_string(),
            kind: Some("function".to_string()),
        },
    )
    .unwrap();

    // types key should be absent (not inserted when kind=function)
    assert!(
        v["types"].is_null(),
        "types should not be present when kind=function"
    );
    let fns = v["functions"].as_array().unwrap();
    assert!(fns.iter().any(|f| f["name"] == "vec_init"));
}

#[tokio::test]
async fn test_search_kind_type() {
    let graph_lock = super::build_graph(&[(
        "test.c",
        "typedef struct { int x; } Point;\nvoid init_point(void) {}\n",
    )])
    .await;
    let graph = graph_lock.read().await;

    let v = search(
        &graph,
        &AnnotationStore::default(),
        std::path::Path::new(""),
        SearchParams {
            name: "Point".to_string(),
            kind: Some("type".to_string()),
        },
    )
    .unwrap();

    let types = v["types"].as_array().unwrap();
    assert_eq!(types.len(), 1);
    assert_eq!(types[0]["name"], "Point");
    assert!(
        v["functions"].is_null(),
        "functions should not be present when kind=type"
    );
}

#[tokio::test]
async fn test_search_unknown_kind_returns_error() {
    let graph_lock = super::build_graph(&[("test.c", "void foo(void) {}")]).await;
    let graph = graph_lock.read().await;

    let result = search(
        &graph,
        &AnnotationStore::default(),
        std::path::Path::new(""),
        SearchParams {
            name: "foo".to_string(),
            kind: Some("banana".to_string()),
        },
    );
    assert!(result.is_err());
}

#[tokio::test]
async fn test_search_no_match_returns_error() {
    let graph_lock = super::build_graph(&[("test.c", "void foo(void) {}")]).await;
    let graph = graph_lock.read().await;

    let result = search(
        &graph,
        &AnnotationStore::default(),
        std::path::Path::new(""),
        SearchParams {
            name: "zzz_nonexistent".to_string(),
            kind: None,
        },
    );
    assert!(result.is_err());
}
