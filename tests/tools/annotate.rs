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

    let v = annotate_file(&graph, &mut store, annotate_params("sensor.c", "Sensor")).unwrap();

    assert_eq!(v["ok"], true);
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

    let _ = annotate_file(&graph, &mut store, annotate_params("mod.c", "First"));
    let _ = annotate_file(&graph, &mut store, annotate_params("mod.c", "Second"));

    assert_eq!(store.files.len(), 1, "upsert should not create duplicates");
    assert_eq!(store.files["mod.c"].subsystem, "Second");
}

#[tokio::test]
async fn test_annotate_file_not_found() {
    let g = super::build_graph(&[("sensor.c", "void read(void) {}")]).await;
    let graph = g.read().await;
    let mut store = empty_store();

    assert!(annotate_file(&graph, &mut store, annotate_params("nonexistent", "X")).is_err());
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

    let err = annotate_file(&graph, &mut store, annotate_params("driver", "X")).unwrap_err();
    assert!(err.contains("ambiguous"));
    assert!(store.files.is_empty());
}

#[tokio::test]
async fn test_get_file_annotation_found() {
    let g = super::build_graph(&[("app.c", "void run(void) {}")]).await;
    let graph = g.read().await;
    let mut store = empty_store();

    let _ = annotate_file(&graph, &mut store, annotate_params("app.c", "App"));

    let v = get_file_annotation(
        &graph,
        &store,
        FilePathParams {
            path: "app".to_string(),
        },
    )
    .unwrap();

    assert_eq!(v["found"], true);
    let results = v["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["subsystem"], "App");
    assert_eq!(results[0]["stale"], false);
}

#[tokio::test]
async fn test_get_file_annotation_not_found() {
    let g = super::build_graph(&[("app.c", "void run(void) {}")]).await;
    let graph = g.read().await;
    let store = empty_store();

    let v = get_file_annotation(
        &graph,
        &store,
        FilePathParams {
            path: "app".to_string(),
        },
    )
    .unwrap();

    assert_eq!(v["found"], false);
}

#[tokio::test]
async fn test_stale_detection() {
    let mut store = empty_store();

    {
        let g = super::build_graph(&[("app.c", "void run(void) {}")]).await;
        let graph = g.read().await;
        let _ = annotate_file(&graph, &mut store, annotate_params("app.c", "App"));

        let v = get_file_annotation(
            &graph,
            &store,
            FilePathParams {
                path: "app.c".to_string(),
            },
        )
        .unwrap();
        assert_eq!(
            v["results"][0]["stale"], false,
            "should be fresh right after annotation"
        );
    }

    {
        let g = super::build_graph(&[("app.c", "void run(void) { int x = 42; }")]).await;
        let graph = g.read().await;

        let v = get_file_annotation(
            &graph,
            &store,
            FilePathParams {
                path: "app.c".to_string(),
            },
        )
        .unwrap();
        assert_eq!(
            v["results"][0]["stale"], true,
            "should be stale after source change"
        );
        assert_eq!(v["results"][0]["subsystem"], "App");
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

    let _ = annotate_file(&graph, &mut store, annotate_params("alpha.c", "Alpha"));
    let _ = annotate_file(&graph, &mut store, annotate_params("beta.c", "Beta"));

    let v = list_file_annotations(&graph, &store).unwrap();

    assert_eq!(v["count"], 2);

    let files = v["files"].as_array().unwrap();
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

    let v = list_unannotated_files(&graph, &store).unwrap();

    assert_eq!(v["count"], 2);

    let files = v["files"].as_array().unwrap();
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

    let _ = annotate_file(&graph, &mut store, annotate_params("foo.c", "Foo"));

    let v = list_unannotated_files(&graph, &store).unwrap();

    assert_eq!(v["count"], 1);
    assert!(v["files"][0]["file"].as_str().unwrap().contains("bar"));
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

    let v = get_file_context(
        &graph,
        &store,
        FileContextParams {
            filename: "ctrl.c".to_string(),
        },
    )
    .unwrap();

    assert_eq!(v["function_count"], 2);

    let fns = v["functions"].as_array().unwrap();
    let run_fn = fns.iter().find(|f| f["name"] == "run").unwrap();
    assert_eq!(run_fn["caller_count"], 0);
    assert_eq!(run_fn["callee_count"], 1);
    assert_eq!(run_fn["callees"][0], "reset");

    assert!(v["existing_annotation"].is_null());
}

#[tokio::test]
async fn test_get_file_context_includes_annotation() {
    let g = super::build_graph(&[("ctrl.c", "void run(void) {}")]).await;
    let graph = g.read().await;
    let mut store = empty_store();

    let _ = annotate_file(&graph, &mut store, annotate_params("ctrl.c", "Control"));

    let v = get_file_context(
        &graph,
        &store,
        FileContextParams {
            filename: "ctrl.c".to_string(),
        },
    )
    .unwrap();

    let ann = &v["existing_annotation"];
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

    assert!(
        get_file_context(
            &graph,
            &store,
            FileContextParams {
                filename: "driver".to_string(),
            },
        )
        .is_err()
    );
}
