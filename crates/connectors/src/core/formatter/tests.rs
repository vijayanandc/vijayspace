//! Unit tests for `formatter`.

use super::*;

#[test]
fn text_actions() {
    assert_eq!(
        run(
            "text_transform",
            &json!({"text": "ann LEE", "operation": "title"})
        )
        .unwrap(),
        json!({"result": "Ann Lee"})
    );
    assert_eq!(
        run("split_name", &json!({"full_name": "Mary Ann Lee"})).unwrap(),
        json!({"first_name": "Mary Ann", "last_name": "Lee"})
    );
    assert_eq!(
        run("text_split", &json!({"text": "a, b,c", "index": 1})).unwrap(),
        json!({"result": ["a", "b", "c"], "item": "b"})
    );
    assert_eq!(
        run(
            "extract",
            &json!({"text": "mail ann@x.io now", "pattern": "email"})
        )
        .unwrap()["result"],
        json!("ann@x.io")
    );
    assert_eq!(
        run(
            "text_replace",
            &json!({"text": "a1b2", "find": "[0-9]", "replace": "", "regex": true})
        )
        .unwrap(),
        json!({"result": "ab"})
    );
}

#[test]
fn number_and_date_actions() {
    assert_eq!(
        run("number_format", &json!({"value": "1234.5", "decimals": 2})).unwrap(),
        json!({"result": "1,234.50"})
    );
    assert_eq!(
        run(
            "number_format",
            &json!({"value": -1_234_567, "decimals": 0})
        )
        .unwrap(),
        json!({"result": "-1,234,567"})
    );
    assert_eq!(
        run("math", &json!({"a": "2", "operation": "multiply", "b": 21})).unwrap(),
        json!({"result": 42})
    );
    assert!(run("math", &json!({"a": 1, "operation": "divide", "b": 0})).is_err());
    assert_eq!(
        run(
            "date_add",
            &json!({"date": "2026-01-31T00:00:00Z", "amount": 1, "unit": "days"})
        )
        .unwrap(),
        json!({"result": "2026-02-01T00:00:00Z"})
    );
    assert_eq!(run("date_format", &json!({"date": "2026-03-08T07:30:00Z", "format": "%d/%m %H:%M", "timezone": "Asia/Kolkata"})).unwrap(), json!({"result": "08/03 13:00"}));
}

#[test]
fn data_actions() {
    let table = json!({"value": "Closed Won", "table": {"Closed Won": "Open"}, "default": "New"});
    assert_eq!(
        run("lookup_table", &table).unwrap(),
        json!({"result": "Open", "found": true})
    );
    assert_eq!(
        run(
            "lookup_table",
            &json!({"value": "X", "table": [{"key": "A", "value": "B"}], "default": "New"})
        )
        .unwrap(),
        json!({"result": "New", "found": false})
    );
    assert_eq!(
        run("parse_json", &json!({"text": "{\"a\": [1]}"})).unwrap(),
        json!({"result": {"a": [1]}})
    );
}
