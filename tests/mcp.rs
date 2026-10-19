//! `custodian mcp serve` through rmcp's own client: in this process over a pipe, and over the built program's
//! stdin and stdout; a call is the command line's run (a receipt, the resident when one answers), a reading is a
//! resource, a bad call carries its liaise code.

#![cfg(feature = "mcp")]

mod common;

use common::*;
use custodian::mcp::{Server, TOOLS};
use rmcp::model::{CallToolRequestParams, CallToolResult, ReadResourceRequestParams, ResourceContents};
use rmcp::service::RunningService;
use rmcp::{RoleClient, ServiceError, ServiceExt};
use serde_json::{json, Value};
use std::path::Path;

type Client = RunningService<RoleClient, ()>;

/// The server for `d`, in this process, and a client on the other end of a pipe.
async fn connect(d: &Path) -> Client {
    let server = Server::with_program(d, BIN.into()).unwrap();
    let (s, c) = tokio::io::duplex(1 << 20);
    tokio::spawn(async move {
        if let Ok(running) = server.serve(s).await {
            let _ = running.waiting().await;
        }
    });
    ().serve(c).await.unwrap()
}

async fn call(client: &Client, tool: &str, args: Value) -> Result<CallToolResult, ServiceError> {
    client.call_tool(CallToolRequestParams::new(tool.to_string()).with_arguments(args.as_object().unwrap().clone())).await
}

/// The result's JSON, and its markdown when inline.
fn parts(r: &CallToolResult) -> (Value, Option<String>) {
    let v = r.structured_content.clone().expect("structured content");
    let text: Vec<String> = r.content.iter().filter_map(|c| c.as_text().map(|t| t.text.clone())).collect();
    assert_eq!(serde_json::from_str::<Value>(&text[0]).unwrap(), v, "the first block is the same JSON");
    (v, text.get(1).cloned())
}

fn text_of(c: &ResourceContents) -> (String, Option<String>) {
    match c {
        ResourceContents::TextResourceContents { text, mime_type, .. } => (text.clone(), mime_type.clone()),
        other => panic!("{other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn tools_run_the_declared_lines_and_readings_are_resources() {
    let d = fixture("mcp");
    ok(&d, &["init"], 0);
    let client = connect(&d).await;

    let tools = client.list_all_tools().await.unwrap();
    assert_eq!(tools.iter().map(|t| t.name.to_string()).collect::<Vec<_>>(), TOOLS.iter().map(|t| t.name.to_string()).collect::<Vec<_>>());
    let run = tools.iter().find(|t| t.name == "run").unwrap();
    assert_eq!(run.input_schema["properties"]["again"]["type"], "boolean");
    assert!(run.input_schema["properties"]["kind"]["enum"].as_array().unwrap().contains(&json!("reach")));

    // count: a receipt in the journal, its page inline
    let (v, md) = parts(&call(&client, "count", json!({})).await.unwrap());
    assert_eq!((v["code"].as_i64(), v["status"].as_str()), (Some(0), Some("done")), "{v:#}");
    let seq = v["receipt"]["seq"].as_i64().unwrap();
    assert_eq!(v["receipt"]["line"], "custodian count");
    assert_eq!(v["receipt_file"], format!(".custodian/data/receipts/{seq}.json"));
    assert_eq!(receipt(&d, seq)["receipt_digest"], v["receipt"]["receipt_digest"], "the receipt the journal keeps");
    assert!(v["said"].as_array().unwrap().iter().any(|s| s.as_str().unwrap().starts_with("read count-workspace (count *)")), "{v:#}");
    let page = v["reading"]["path"].as_str().unwrap();
    assert_eq!(md.as_deref(), Some(std::fs::read_to_string(d.join(page)).unwrap().as_str()), "the count's page, inline");

    // run --kind reach: the reading's page, inline and as a resource
    let (v, md) = parts(&call(&client, "run", json!({"target": "crate:core_lib", "kind": "reach"})).await.unwrap());
    assert_eq!(v["code"], 0, "{v:#}");
    let seq = v["receipt"]["seq"].as_i64().unwrap();
    let page = v["reading"]["path"].as_str().unwrap().to_string();
    assert!(page.starts_with(".custodian/data/readings/") && page.ends_with(".md"), "{page}");
    let on_disk = std::fs::read_to_string(d.join(&page)).unwrap();
    assert_eq!(md.as_deref(), Some(on_disk.as_str()));
    assert!(on_disk.contains("## binaries"), "{on_disk}");
    let uri = format!("custodian://readings/{seq}");
    assert_eq!(v["reading"]["resource"], uri);

    let listed = client.list_all_resources().await.unwrap();
    assert!(listed.iter().any(|r| r.uri == uri), "{listed:?}");
    let read = client.read_resource(ReadResourceRequestParams::new(uri.clone())).await.unwrap();
    assert_eq!(text_of(&read.contents[0]), (on_disk.clone(), Some("text/markdown".into())));

    // the same search again is the same reading: the resource is the first run's
    let (v, _) = parts(&call(&client, "run", json!({"target": "crate:core_lib", "kind": "reach"})).await.unwrap());
    assert_eq!((v["receipt"]["reused_seq"].as_i64(), v["reading"]["resource"].as_str()), (Some(seq), Some(uri.as_str())), "{v:#}");

    // words that do not make the line: a protocol error with the liaise code, no receipt
    let before = receipt_count(&d);
    match call(&client, "run", json!({"target": "x", "kind": "nonsense"})).await {
        Err(ServiceError::McpError(e)) => assert_eq!(e.data.unwrap()["code"], "CUST1001"),
        other => panic!("{other:?}"),
    }
    match call(&client, "annotate", json!({"apply": true})).await {
        Err(ServiceError::McpError(e)) => assert_eq!(e.data.unwrap()["code"], "CUST1904"),
        other => panic!("{other:?}"),
    }
    match client.read_resource(ReadResourceRequestParams::new("custodian://readings/9999")).await {
        Err(ServiceError::McpError(e)) => assert_eq!(e.data.unwrap()["code"], "CUST1905"),
        other => panic!("{other:?}"),
    }
    assert_eq!(receipt_count(&d), before);

    // a line that ran and was refused: a tool error with the code and its receipt
    let r = call(&client, "diff", json!({"spec": "nothing"})).await.unwrap();
    assert_eq!(r.is_error, Some(true));
    let (v, _) = parts(&r);
    assert_eq!((v["code"].as_i64(), v["refusal"]["code"].as_str()), (Some(2), Some("CUST1202")), "{v:#}");
    assert_eq!(receipt_count(&d), before + 1);

    client.cancel().await.unwrap();
}

fn receipt_count(d: &Path) -> usize {
    std::fs::read_dir(d.join(".custodian/data/receipts")).unwrap().count()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_program_serves_on_its_stdio_and_a_resident_answers_its_lines() {
    let d = fixture("mcp-stdio");
    ok(&d, &["agent", "init"], 0);
    let mut resident = std::process::Command::new(BIN).arg("serve").current_dir(&d).stdout(std::process::Stdio::null()).spawn().unwrap();

    let mut child = tokio::process::Command::new(BIN)
        .args(["mcp", "serve", "--root"])
        .arg(&d)
        .current_dir(std::env::temp_dir())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .env_remove("XDG_RUNTIME_DIR")
        .spawn()
        .unwrap();
    let (stdout, stdin) = (child.stdout.take().unwrap(), child.stdin.take().unwrap());
    let client: Client = ().serve((stdout, stdin)).await.unwrap();
    assert_eq!(client.list_all_tools().await.unwrap().len(), TOOLS.len());

    // run is a line the resident answers; wait for it to bind
    let mut answered = None;
    for _ in 0..100 {
        let (v, _) = parts(&call(&client, "run", json!({"target": "crate:app", "kind": "reach"})).await.unwrap());
        if v["said"].as_array().unwrap().iter().any(|s| s.as_str().unwrap().starts_with("answered by the resident")) {
            answered = Some(v);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let v = answered.expect("the resident answered");
    assert_eq!(v["code"], 0, "{v:#}");
    // count is not a resident's line: this process runs it, as the command line does
    let (v, _) = parts(&call(&client, "count", json!({"args": ["crate:core_lib"], "by": "language"})).await.unwrap());
    assert_eq!(v["receipt"]["line"], "custodian count crate:core_lib --by language", "{v:#}");
    assert!(!v["said"].to_string().contains("answered by the resident"));

    client.cancel().await.unwrap();
    assert!(child.wait().await.unwrap().success(), "the server ends when the host closes stdin");
    ok(&d, &["stop"], 0);
    assert!(resident.wait().unwrap().success());
}
