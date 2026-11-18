//! Custom functions in child processes: normal scripts work, and scripts that defeat the
//! engine's size accounting (map keys) only kill the child.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{path::PathBuf, time::Duration};

use serde_json::json;
use weft_connector_sdk::ScriptRunner;
use weft_infra::ProcessScriptRunner;

fn runner() -> ProcessScriptRunner {
    ProcessScriptRunner::new(
        PathBuf::from(env!("CARGO_BIN_EXE_weft")),
        2,
        256,
        Duration::from_secs(30),
    )
}

#[tokio::test]
async fn scripts_run_in_a_child_process() {
    let out = runner()
        .run(
            "print(\"hi\"); #{ total: input.a + input.b }",
            json!({"a": 1, "b": 2}),
            json!({}),
        )
        .await
        .unwrap();
    assert_eq!(out.output, json!({"total": 3}));
    assert_eq!(out.logs, vec!["hi"]);
    let err = runner()
        .run("1;\nthrow \"bad\"", json!({}), json!({}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "script.error");
    assert!(err.to_string().starts_with("line 2"), "{err}");
}

#[tokio::test]
async fn memory_bombs_only_kill_the_child() {
    let bombs = [
        // Map keys are not counted by the engine's size limits.
        "let s = \"x\"; for i in 0..19 { s += s; } let m = #{}; for i in 0..100000 { m[s + i] = i; } m.len()",
        // Nor are keys of maps nested in arrays.
        "let s = \"x\"; for i in 0..19 { s += s; } let a = []; for i in 0..5000 { let m = #{}; m[s + i] = 1; a.push(m); } a.len()",
    ];
    for code in bombs {
        let err = runner().run(code, json!({}), json!({})).await.unwrap_err();
        assert_eq!(err.code(), "script.limit_exceeded", "{code}: {err}");
    }
    // The parent is unharmed and keeps running scripts.
    let ok = runner()
        .run("#{ ok: true }", json!({}), json!({}))
        .await
        .unwrap();
    assert_eq!(ok.output, json!({"ok": true}));
}
