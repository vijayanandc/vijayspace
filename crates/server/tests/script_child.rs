//! Custom functions in child processes: normal scripts work, and scripts that defeat the
//! engine's size accounting (map keys) only kill the child; an engine panic is a script error.
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
    // Scripts change their own copy of the input; an engine panic is a script error, not a crash.
    let out = runner()
        .run(
            "input.total = input.a + input.b; input",
            json!({"a": 1, "b": 2}),
            json!({}),
        )
        .await
        .unwrap();
    assert_eq!(out.output, json!({"a": 1, "b": 2, "total": 3}));
    let err = runner()
        .run("const C = #{a: 1}; C.a = 2; C", json!({}), json!({}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "script.error", "{err}");
    // Deeply nested values need more than a default thread stack; results stay shallow.
    let deep = "let a = #{}; for i in 0..1000 { a = #{x: a}; }";
    let nested = runner()
        .run(&format!("{deep} a.len()"), json!({}), json!({}))
        .await
        .unwrap();
    assert_eq!(nested.output, json!({"result": 1}));
    let err = runner()
        .run(&format!("{deep} a"), json!({}), json!({}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "script.limit_exceeded", "{err}");
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
    // A child cannot flood the parent: a huge thrown map is cut short in the child.
    let flood = "let s = \"x\"; for i in 0..19 { s += s; } let m = #{}; for i in 0..150 { m[s + i] = i; } throw m;";
    let err = runner().run(flood, json!({}), json!({})).await.unwrap_err();
    assert!(
        err.to_string().len() < 4_000,
        "{} bytes",
        err.to_string().len()
    );
    // The parent is unharmed and keeps running scripts.
    let ok = runner()
        .run("#{ ok: true }", json!({}), json!({}))
        .await
        .unwrap();
    assert_eq!(ok.output, json!({"ok": true}));
}
