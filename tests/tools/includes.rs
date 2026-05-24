use cofe_graph::tools::includes::{
    GetIncludersParams, GetIncludesParams, get_includers, get_includes,
};

#[tokio::test]
async fn test_get_includes() {
    let graph_lock = super::build_graph(&[(
        "app.c",
        concat!(
            "#include \"dep.h\"\n",
            "#include \"config.h\"\n",
            "int main(void) { return 0; }"
        ),
    )])
    .await;
    let graph = graph_lock.read().await;

    let params = GetIncludesParams {
        file: "app.c".to_string(),
    };
    let v = get_includes(&graph, params).unwrap();
    let results = v.as_array().expect("Expected array");
    let includes: Vec<&str> = results[0]["includes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["path"].as_str().unwrap())
        .collect();

    assert!(includes.contains(&"dep.h"));
    assert!(includes.contains(&"config.h"));
}

#[tokio::test]
async fn test_get_includers() {
    let graph_lock = super::build_graph(&[
        (
            "app.c",
            concat!("#include \"dep.h\"\n", "int main(void) { return 0; }"),
        ),
        (
            "mod_a.c",
            concat!("#include \"dep.h\"\n", "void func_a(void) {}"),
        ),
        (
            "mod_b.c",
            concat!("#include \"dep.h\"\n", "void func_b(void) {}"),
        ),
    ])
    .await;
    let graph = graph_lock.read().await;

    let params = GetIncludersParams {
        header: "dep.h".to_string(),
    };
    let v = get_includers(&graph, params).unwrap();
    let includers: Vec<&str> = v
        .as_array()
        .expect("Expected includers array")
        .iter()
        .map(|m| m["file"].as_str().unwrap())
        .collect();

    assert!(
        includers.iter().any(|f| f.ends_with("app.c")),
        "app.c should include dep.h"
    );
    assert!(
        includers.iter().any(|f| f.ends_with("mod_a.c")),
        "mod_a.c should include dep.h"
    );
    assert!(
        includers.iter().any(|f| f.ends_with("mod_b.c")),
        "mod_b.c should include dep.h"
    );
}
