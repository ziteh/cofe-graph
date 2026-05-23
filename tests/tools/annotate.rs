use serde_json::Value;

use cofe_graph::annotations::AnnotationStore;
use cofe_graph::tools::annotate::{
    AnnotateFileParams, FileContextParams, FilePathParams, annotate_file, get_file_annotation,
    get_file_context, list_file_annotations, list_unannotated_files,
};

fn empty_store() -> AnnotationStore {
    AnnotationStore::default()
}

fn annotate_params(path: &str, subsystem: &str) -> AnnotateFileParams {
    AnnotateFileParams {
        path: path.to_string(),
        subsystem: subsystem.to_string(),
        summary: format!("{subsystem} module"),
        key_functions: vec![],
        notes: None,
    }
}

#[tokio::test]
async fn test_annotate_file_ok() {
    let g = super::build_graph(&[("sensor.c", "void read(void) {}\nvoid write(int v) {}")]).await;
    let graph = g.read().await;
    let mut store = empty_store();

    let result = annotate_file(&graph, &mut store, annotate_params("sensor.c", "Sensor"));
    let v: Value = serde_json::from_str(&result).unwrap();

    assert_eq!(v["isError"], false);
    assert_eq!(v["content"]["ok"], true);
    assert_eq!(store.files.len(), 1);

    let ann = store.files.get("sensor.c").unwrap();
    assert_eq!(ann.subsystem, "Sensor");
    assert!(!ann.file_hash.is_empty());
}

#[tokio::test]
async fn test_annotate_file_upsert() {
    let g = super::build_graph(&[("mod.c", "void foo(void) {}")]).await;
    let graph = g.read().await;
    let mut store = empty_store();

    annotate_file(&graph, &mut store, annotate_params("mod.c", "First"));
    annotate_file(&graph, &mut store, annotate_params("mod.c", "Second"));

    assert_eq!(store.files.len(), 1, "upsert should not create duplicates");
    assert_eq!(store.files["mod.c"].subsystem, "Second");
}

#[tokio::test]
async fn test_annotate_file_not_found() {
    let g = super::build_graph(&[("sensor.c", "void read(void) {}")]).await;
    let graph = g.read().await;
    let mut store = empty_store();

    let result = annotate_file(&graph, &mut store, annotate_params("nonexistent", "X"));
    let v: Value = serde_json::from_str(&result).unwrap();

    assert_eq!(v["isError"], true);
    assert!(store.files.is_empty());
}

#[tokio::test]
async fn test_annotate_file_ambiguous() {
    let g = super::build_graph(&[
        ("driver_uart.c", "void uart_init(void) {}"),
        ("driver_spi.c", "void spi_init(void) {}"),
    ])
    .await;
    let graph = g.read().await;
    let mut store = empty_store();

    let result = annotate_file(&graph, &mut store, annotate_params("driver", "X"));
    let v: Value = serde_json::from_str(&result).unwrap();

    assert_eq!(v["isError"], true);
    assert!(
        v["content"]["error"]
            .as_str()
            .unwrap()
            .contains("ambiguous")
    );
    assert!(store.files.is_empty());
}

#[tokio::test]
async fn test_get_file_annotation_found() {
    let g = super::build_graph(&[("app.c", "void run(void) {}")]).await;
    let graph = g.read().await;
    let mut store = empty_store();

    annotate_file(&graph, &mut store, annotate_params("app.c", "App"));

    let result = get_file_annotation(
        &graph,
        &store,
        FilePathParams {
            path: "app".to_string(),
        },
    );
    let v: Value = serde_json::from_str(&result).unwrap();

    assert_eq!(v["isError"], false);
    assert_eq!(v["content"]["found"], true);
    let results = v["content"]["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["subsystem"], "App");
    assert_eq!(results[0]["stale"], false);
}

#[tokio::test]
async fn test_get_file_annotation_not_found() {
    let g = super::build_graph(&[("app.c", "void run(void) {}")]).await;
    let graph = g.read().await;
    let store = empty_store();

    let result = get_file_annotation(
        &graph,
        &store,
        FilePathParams {
            path: "app".to_string(),
        },
    );
    let v: Value = serde_json::from_str(&result).unwrap();

    assert_eq!(v["isError"], false);
    assert_eq!(v["content"]["found"], false);
}

#[tokio::test]
async fn test_stale_detection() {
    let mut store = empty_store();

    // Annotate on original source
    {
        let g = super::build_graph(&[("app.c", "void run(void) {}")]).await;
        let graph = g.read().await;
        annotate_file(&graph, &mut store, annotate_params("app.c", "App"));

        let result = get_file_annotation(
            &graph,
            &store,
            FilePathParams {
                path: "app.c".to_string(),
            },
        );
        let v: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            v["content"]["results"][0]["stale"], false,
            "should be fresh right after annotation"
        );
    }

    // Same key, different function body → stale
    {
        let g = super::build_graph(&[("app.c", "void run(void) { int x = 42; }")]).await;
        let graph = g.read().await;

        let result = get_file_annotation(
            &graph,
            &store,
            FilePathParams {
                path: "app.c".to_string(),
            },
        );
        let v: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            v["content"]["results"][0]["stale"], true,
            "should be stale after source change"
        );
        // annotation content still accessible
        assert_eq!(v["content"]["results"][0]["subsystem"], "App");
    }
}

#[tokio::test]
async fn test_list_file_annotations() {
    let g = super::build_graph(&[
        ("alpha.c", "void a(void) {}"),
        ("beta.c", "void b(void) {}"),
    ])
    .await;
    let graph = g.read().await;
    let mut store = empty_store();

    annotate_file(&graph, &mut store, annotate_params("alpha.c", "Alpha"));
    annotate_file(&graph, &mut store, annotate_params("beta.c", "Beta"));

    let result = list_file_annotations(&graph, &store);
    let v: Value = serde_json::from_str(&result).unwrap();

    assert_eq!(v["isError"], false);
    assert_eq!(v["content"]["count"], 2);

    let files = v["content"]["files"].as_array().unwrap();
    assert!(files.iter().any(|f| f["subsystem"] == "Alpha"));
    assert!(files.iter().any(|f| f["subsystem"] == "Beta"));
    assert!(files.iter().all(|f| f["stale"] == false));
}

#[tokio::test]
async fn test_list_unannotated_files_all_missing() {
    let g = super::build_graph(&[
        ("big.c", "void a(void) {}\nvoid b(void) {}\nvoid c(void) {}"),
        ("small.c", "void z(void) {}"),
    ])
    .await;
    let graph = g.read().await;
    let store = empty_store();

    let result = list_unannotated_files(&graph, &store);
    let v: Value = serde_json::from_str(&result).unwrap();

    assert_eq!(v["isError"], false);
    assert_eq!(v["content"]["count"], 2);

    let files = v["content"]["files"].as_array().unwrap();
    // big.c (3 fns) should come before small.c (1 fn)
    assert!(files[0]["file"].as_str().unwrap().contains("big"));
    assert_eq!(files[0]["function_count"], 3);
}

#[tokio::test]
async fn test_list_unannotated_files_partial() {
    let g = super::build_graph(&[
        ("foo.c", "void foo(void) {}"),
        ("bar.c", "void bar(void) {}"),
    ])
    .await;
    let graph = g.read().await;
    let mut store = empty_store();

    annotate_file(&graph, &mut store, annotate_params("foo.c", "Foo"));

    let result = list_unannotated_files(&graph, &store);
    let v: Value = serde_json::from_str(&result).unwrap();

    assert_eq!(v["content"]["count"], 1);
    assert!(
        v["content"]["files"][0]["file"]
            .as_str()
            .unwrap()
            .contains("bar")
    );
}

#[tokio::test]
async fn test_get_file_context_structure() {
    let g = super::build_graph(&[(
        "ctrl.c",
        concat!(
            "static int g_state = 0;\n",
            "static void reset(void) { g_state = 0; }\n",
            "void run(void) { reset(); }\n",
        ),
    )])
    .await;
    let graph = g.read().await;
    let store = empty_store();

    let result = get_file_context(
        &graph,
        &store,
        FileContextParams {
            filename: "ctrl.c".to_string(),
        },
    );
    let v: Value = serde_json::from_str(&result).unwrap();

    assert_eq!(v["isError"], false);
    assert_eq!(v["content"]["function_count"], 2);

    let fns = v["content"]["functions"].as_array().unwrap();
    let run_fn = fns.iter().find(|f| f["name"] == "run").unwrap();
    assert_eq!(run_fn["caller_count"], 0);
    assert_eq!(run_fn["callee_count"], 1);
    assert_eq!(run_fn["callees"][0], "reset");

    // no annotation yet
    assert!(v["content"]["existing_annotation"].is_null());
}

#[tokio::test]
async fn test_get_file_context_includes_annotation() {
    let g = super::build_graph(&[("ctrl.c", "void run(void) {}")]).await;
    let graph = g.read().await;
    let mut store = empty_store();

    annotate_file(&graph, &mut store, annotate_params("ctrl.c", "Control"));

    let result = get_file_context(
        &graph,
        &store,
        FileContextParams {
            filename: "ctrl.c".to_string(),
        },
    );
    let v: Value = serde_json::from_str(&result).unwrap();

    let ann = &v["content"]["existing_annotation"];
    assert!(!ann.is_null());
    assert_eq!(ann["subsystem"], "Control");
    assert_eq!(ann["stale"], false);
}

#[tokio::test]
async fn test_get_file_context_ambiguous() {
    let g = super::build_graph(&[
        ("driver_i2c.c", "void i2c_init(void) {}"),
        ("driver_spi.c", "void spi_init(void) {}"),
    ])
    .await;
    let graph = g.read().await;
    let store = empty_store();

    let result = get_file_context(
        &graph,
        &store,
        FileContextParams {
            filename: "driver".to_string(),
        },
    );
    let v: Value = serde_json::from_str(&result).unwrap();
    assert_eq!(v["isError"], true);
}
