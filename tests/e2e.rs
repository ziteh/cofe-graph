use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::time::timeout;

struct McpClient {
    stdin: tokio::process::ChildStdin,
    reader: BufReader<tokio::process::ChildStdout>,
    child: tokio::process::Child,
}

impl McpClient {
    async fn spawn() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_cofe-graph"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("failed to spawn cofe-graph binary");

        let stdin = child.stdin.take().unwrap();
        let reader = BufReader::new(child.stdout.take().unwrap());
        McpClient {
            stdin,
            reader,
            child,
        }
    }

    async fn send(&mut self, msg: &Value) {
        let mut line = serde_json::to_string(msg).unwrap();
        line.push('\n');
        self.stdin.write_all(line.as_bytes()).await.unwrap();
        self.stdin.flush().await.unwrap();
    }

    // Read lines until we get a response whose "id" matches; skip notifications.
    async fn recv_id(&mut self, id: u64) -> Value {
        timeout(Duration::from_secs(5), async {
            loop {
                let mut line = String::new();
                self.reader
                    .read_line(&mut line)
                    .await
                    .expect("server closed stdout");
                let v: Value = serde_json::from_str(line.trim()).expect("invalid JSON from server");
                if v.get("id") == Some(&json!(id)) {
                    return v;
                }
            }
        })
        .await
        .expect("timed out waiting for server response")
    }

    async fn initialize(&mut self) {
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "0.1"}
            }
        }))
        .await;
        self.recv_id(0).await;

        self.send(&json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        }))
        .await;
    }

    async fn call_tool(&mut self, id: u64, name: &str, args: Value) -> String {
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {"name": name, "arguments": args}
        }))
        .await;
        let resp = self.recv_id(id).await;
        resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("unexpected response shape: {resp}"))
            .to_string()
    }

    async fn shutdown(mut self) {
        let _ = self.child.kill().await;
    }
}

fn fixtures_path() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .to_string_lossy()
        .into_owned()
}

// tests

#[tokio::test]
async fn index_reports_file_and_function_counts() {
    let mut c = McpClient::spawn().await;
    c.initialize().await;

    let out = c
        .call_tool(1, "index_project", json!({"path": fixtures_path()}))
        .await;
    assert!(out.contains("Indexed 2 files"), "got: {out}");
    assert!(out.contains("(0 errors)"), "got: {out}");

    c.shutdown().await;
}

#[tokio::test]
async fn find_function_returns_location() {
    let mut c = McpClient::spawn().await;
    c.initialize().await;
    c.call_tool(1, "index_project", json!({"path": fixtures_path()}))
        .await;

    let out = c
        .call_tool(2, "find_function", json!({"name": "add"}))
        .await;
    assert!(out.contains("add"), "got: {out}");
    assert!(out.contains("math.c"), "got: {out}");

    c.shutdown().await;
}

#[tokio::test]
async fn find_function_case_insensitive() {
    let mut c = McpClient::spawn().await;
    c.initialize().await;
    c.call_tool(1, "index_project", json!({"path": fixtures_path()}))
        .await;

    let out = c
        .call_tool(2, "find_function", json!({"name": "MULTIPLY"}))
        .await;
    assert!(out.contains("multiply"), "got: {out}");

    c.shutdown().await;
}

#[tokio::test]
async fn find_function_no_match() {
    let mut c = McpClient::spawn().await;
    c.initialize().await;
    c.call_tool(1, "index_project", json!({"path": fixtures_path()}))
        .await;

    let out = c
        .call_tool(2, "find_function", json!({"name": "nonexistent_xyz"}))
        .await;
    assert!(out.contains("No functions matching"), "got: {out}");

    c.shutdown().await;
}

#[tokio::test]
async fn get_callees_of_main() {
    let mut c = McpClient::spawn().await;
    c.initialize().await;
    c.call_tool(1, "index_project", json!({"path": fixtures_path()}))
        .await;

    let out = c
        .call_tool(2, "get_callees", json!({"name": "main", "depth": 1}))
        .await;
    assert!(out.contains("print_result"), "got: {out}");

    c.shutdown().await;
}

#[tokio::test]
async fn get_callees_deep_traversal() {
    let mut c = McpClient::spawn().await;
    c.initialize().await;
    c.call_tool(1, "index_project", json!({"path": fixtures_path()}))
        .await;

    // main -> multiply -> add  (depth 2 should reach add)
    let out = c
        .call_tool(2, "get_callees", json!({"name": "main", "depth": 2}))
        .await;
    assert!(out.contains("multiply"), "got: {out}");
    assert!(out.contains("add"), "got: {out}");

    c.shutdown().await;
}

#[tokio::test]
async fn get_callers_of_add() {
    let mut c = McpClient::spawn().await;
    c.initialize().await;
    c.call_tool(1, "index_project", json!({"path": fixtures_path()}))
        .await;

    let out = c.call_tool(2, "get_callers", json!({"name": "add"})).await;
    assert!(out.contains("multiply"), "got: {out}");

    c.shutdown().await;
}

#[tokio::test]
async fn get_callers_of_print_result() {
    let mut c = McpClient::spawn().await;
    c.initialize().await;
    c.call_tool(1, "index_project", json!({"path": fixtures_path()}))
        .await;

    let out = c
        .call_tool(2, "get_callers", json!({"name": "print_result"}))
        .await;
    assert!(out.contains("main"), "got: {out}");

    c.shutdown().await;
}

#[tokio::test]
async fn index_invalid_path_returns_error() {
    let mut c = McpClient::spawn().await;
    c.initialize().await;

    let out = c
        .call_tool(1, "index_project", json!({"path": "/nonexistent/path"}))
        .await;
    assert!(out.contains("error"), "got: {out}");

    c.shutdown().await;
}
