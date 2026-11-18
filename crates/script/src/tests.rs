//! Table tests: the documented example, every helper, output shapes and hostile scripts.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use serde_json::{Value, json};

use crate::{Limits, ScriptError, check, run};

fn ok(code: &str, input: Value) -> Value {
    run(
        code,
        &input,
        &json!({"now": "2026-01-01T00:00:00Z"}),
        &Limits::default(),
    )
    .unwrap_or_else(|e| panic!("{code}: {e}"))
    .value
}

fn err(code: &str) -> ScriptError {
    run(code, &json!({}), &json!({}), &Limits::default()).expect_err(code)
}

/// The example from doc 07 §9, kept here so the documentation cannot go stale.
const DOC_EXAMPLE: &str = r#"
// Sum the line items and flag big orders.
let total = 0.0;
for item in input.items {
    total += item.price * item.qty.to_float();
}
if total < 0.0 { throw "negative total"; }
#{ total: total, is_big: total > 100.0, currency: input.currency }
"#;

#[test]
fn the_documented_example_runs() {
    let out = ok(
        DOC_EXAMPLE,
        json!({"currency": "INR", "items": [{"price": 40.0, "qty": 2}, {"price": 25.5, "qty": 1}]}),
    );
    assert_eq!(
        out,
        json!({"total": 105.5, "is_big": true, "currency": "INR"})
    );
}

#[test]
fn maps_are_the_output_and_other_values_are_wrapped() {
    assert_eq!(ok("#{ a: 1 }", json!({})), json!({"a": 1}));
    assert_eq!(ok("40 + 2", json!({})), json!({"result": 42}));
    assert_eq!(ok("[1, \"x\"]", json!({})), json!({"result": [1, "x"]}));
    assert_eq!(ok("let x = 1;", json!({})), json!({"result": null}));
    assert_eq!(
        ok("input.name.to_upper()", json!({"name": "ann"})),
        json!({"result": "ANN"})
    );
    assert_eq!(
        ok("context.now", json!({})),
        json!({"result": "2026-01-01T00:00:00Z"})
    );
}

#[test]
fn every_helper_works() {
    let cases = [
        (r#"parse_json("{\"a\": [1, 2]}").a[1]"#, json!(2)),
        ("to_json(#{ a: 1 })", json!("{\"a\":1}")),
        (
            r#"format_date("2026-01-02T03:04:05Z", "%d %b %Y")"#,
            json!("02 Jan 2026"),
        ),
        (
            r#"format_date("2026-01-01T20:00:00Z", "%Y-%m-%d", "Asia/Kolkata")"#,
            json!("2026-01-02"),
        ),
        (r#"format_date(1767225600, "%Y")"#, json!("2026")),
        (
            r#"date_add("2026-01-31T00:00:00Z", 1, "days")"#,
            json!("2026-02-01T00:00:00Z"),
        ),
        (r##"regex_replace("a1b22", "\\d+", "#")"##, json!("a#b#")),
        (r#"regex_match("Order-42", "^Order-\\d+$")"#, json!(true)),
        (r#"base64_decode(base64_encode("héllo"))"#, json!("héllo")),
        (r#"url_decode(url_encode("a b&c"))"#, json!("a b&c")),
        (r#"sha256("abc").len()"#, json!(64)),
        (r#"hmac_sha256("msg", "key").len()"#, json!(64)),
    ];
    for (code, expected) in cases {
        assert_eq!(ok(code, json!({}))["result"], expected, "{code}");
    }
}

#[test]
fn print_and_debug_are_captured_not_written_to_stdout() {
    let out = run(
        "print(\"hello\"); debug(42); 1",
        &json!({}),
        &json!({}),
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(out.logs.len(), 2);
    assert_eq!(out.logs[0], "hello");
    let many = run(
        "for i in 0..500 { print(i); } 1",
        &json!({}),
        &json!({}),
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(many.logs.len(), 101, "100 lines then a note");
    assert!(many.logs[100].contains("dropped"));
}

#[test]
fn throw_and_runtime_errors_are_script_errors_with_a_position() {
    let e = err("let a = 1;\nthrow \"bad input\";");
    assert_eq!(e.code(), "script.error");
    assert!(e.to_string().contains("bad input"), "{e}");
    assert_eq!(e.position().0, Some(2));
    assert_eq!(err("undefined_fn(1)").code(), "script.error");
    // A huge thrown value is cut short.
    let huge = err("let s = \"x\"; for i in 0..15 { s += s; } throw s;").to_string();
    assert!(huge.chars().count() < 2_100, "{} chars", huge.len());
    // The position is reported once, not repeated inside the message.
    let runtime = err("let x = 1;\nfor i in x { }").to_string();
    assert_eq!(runtime.matches("line 2").count(), 1, "{runtime}");
    assert!(!runtime.contains("position"), "{runtime}");
    assert_eq!(err("\"a\" - 1").code(), "script.error");
    assert_eq!(err(r#"regex_replace("x", "(", "")"#).code(), "script.error");
}

#[test]
fn syntax_errors_have_line_and_column_at_check_time() {
    let e = check("let x = 1;\nlet y = ;", &Limits::default()).unwrap_err();
    assert_eq!(e.code(), "script.error");
    assert_eq!(e.position().0, Some(2));
    assert!(e.position().1.is_some());
    assert!(check("#{ a: input.x }", &Limits::default()).is_ok());
    // Unknown variables are caught before running.
    assert!(check("missing + 1", &Limits::default()).is_err());
}

#[test]
fn hostile_scripts_hit_limits_instead_of_hanging_or_exhausting_memory() {
    let hostile = [
        "loop { }",
        "let s = \"x\"; loop { s += s; }",
        "let a = []; loop { a.push(1); }",
        "let m = #{}; let i = 0; loop { m[`k${i}`] = i; i += 1; }",
        "fn f(n) { f(n + 1) } f(0)",
    ];
    // A generous wall time: these must stop on the deterministic limits (operations, sizes,
    // call depth), not on the clock.
    let limits = Limits {
        max_wall_time: Duration::from_mins(10),
        ..Limits::default()
    };
    for code in hostile {
        let e = run(code, &json!({}), &json!({}), &limits).expect_err(code);
        assert_eq!(e.code(), "script.limit_exceeded", "{code}");
        assert!(
            !e.to_string().contains("too long"),
            "{code} stopped on the clock: {e}"
        );
    }
    let deep = format!("{}1{}", "(".repeat(200), ")".repeat(200));
    assert_eq!(
        check(&deep, &Limits::default()).unwrap_err().code(),
        "script.limit_exceeded"
    );
    let huge = "1;".repeat(40_000);
    assert_eq!(
        check(&huge, &Limits::default()).unwrap_err().code(),
        "script.limit_exceeded"
    );
}

#[test]
fn eval_and_imports_are_unavailable() {
    assert!(check("eval(\"1\")", &Limits::default()).is_err());
    assert!(check("import \"x\" as y;", &Limits::default()).is_err());
    assert!(
        check("timestamp()", &Limits::default()).is_err()
            || err("timestamp()").code() == "script.error"
    );
}

#[test]
fn the_same_input_always_gives_the_same_output() {
    let code = "let m = #{}; for k in input.keys { m[k] = k.len(); } m";
    let input = json!({"keys": ["b", "aa", "ccc"]});
    assert_eq!(ok(code, input.clone()), ok(code, input));
}

#[test]
fn oversized_results_are_rejected() {
    let limits = Limits {
        max_output_bytes: 100,
        ..Limits::default()
    };
    let e = run(
        "let s = \"\"; for i in 0..500 { s += \"y\"; } s",
        &json!({}),
        &json!({}),
        &limits,
    )
    .unwrap_err();
    assert_eq!(e.code(), "script.limit_exceeded");
}

#[test]
fn the_wall_time_guard_stops_a_script_as_a_safety_net() {
    let limits = Limits {
        max_wall_time: Duration::ZERO,
        ..Limits::default()
    };
    let e = run("loop { }", &json!({}), &json!({}), &limits).unwrap_err();
    assert_eq!(e.code(), "script.limit_exceeded");
    assert!(e.to_string().contains("too long"), "{e}");
}

#[test]
fn curried_values_are_unreachable_but_closures_and_functions_see_the_input() {
    for code in ["fn f(x) { x } Fn(\"f\").curry(1)", "(|x| x).curry(1)"] {
        assert!(check(code, &Limits::default()).is_err(), "{code}");
    }
    let input = json!({"items": [1, 2, 3], "factor": 10});
    let cases = [
        ("input.items.map(|x| x * input.factor)", json!([10, 20, 30])),
        ("fn g() { input.factor } g()", json!(10)),
        (
            "let f = input.factor; input.items.map(|x| x * f)",
            json!([10, 20, 30]),
        ),
        ("[1, 2, 3].map(|x| x * 2)", json!([2, 4, 6])),
    ];
    for (code, expected) in cases {
        assert_eq!(ok(code, input.clone())["result"], expected, "{code}");
    }
    // A large input mentioned several times still works.
    let big = json!({"keys": (0..2000).map(|i| format!("k{i}")).collect::<Vec<_>>()});
    let code =
        "let a = input; let b = input; let c = input; a.keys.len() + b.keys.len() + c.keys.len()";
    assert_eq!(ok(code, big), json!({"result": 6000}));
}
