//! Sync layer end to end with two in-memory record apps: create, skip unchanged, update,
//! recover by match, lookups through links, and out-of-order retry.

use chrono::Duration;
use serde_json::{Value, json};

use crate::harness::TestApp;

fn sync_flow(src: &str, dst: &str, object: &str, values: Value, extra: Value) -> Value {
    let mut input = json!({
        "link_set": format!("{object}_link"),
        "source_connection_id": src, "source_object": object, "source_record_id": "{{ trigger.id }}",
        "target_connection_id": dst, "target_object": object,
        "values": values,
    });
    if let (Some(obj), Some(more)) = (input.as_object_mut(), extra.as_object()) {
        obj.extend(more.clone());
    }
    json!({"trigger": {"type": "manual"}, "steps": [
        {"key": "sync", "type": "action", "connector": "core.sync", "action": "upsert_linked_record", "input": input}
    ]})
}

async fn run(app: &TestApp, flow: &str, payload: Value) -> Value {
    let r = app
        .ok(
            "POST",
            &format!("/api/v1/flows/{flow}/run"),
            Some(json!({"payload": payload})),
        )
        .await;
    app.drain().await;
    app.ok(
        "GET",
        &format!("/api/v1/executions/{}", r["execution_id"].as_str().unwrap()),
        None,
    )
    .await
}

fn output(detail: &Value) -> Value {
    detail["steps"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .find(|s| s["step_key"] == "sync")
        .map_or(Value::Null, |s| s["output"].clone())
}

#[tokio::test]
async fn upsert_linked_record_never_duplicates_and_skips_unchanged() {
    let app = TestApp::spawn().await;
    let src = app.connection("memsrc", "Source").await;
    let dst = app.connection("memdst", "Target").await;
    let flow = app
        .live_flow(
            "Accounts",
            sync_flow(
                &src,
                &dst,
                "accounts",
                json!({"name": "{{ trigger.name }}"}),
                json!({"match_field": "name", "match_value": "{{ trigger.name }}"}),
            ),
        )
        .await;
    let flow_id = flow["id"].as_str().unwrap();

    let first = run(&app, flow_id, json!({"id": "A1", "name": "Acme"})).await;
    assert_eq!(first["status"], "succeeded", "{first}");
    assert_eq!(output(&first)["action"], "created");
    let target_id = output(&first)["target_id"].as_str().unwrap().to_owned();

    let second = run(&app, flow_id, json!({"id": "A1", "name": "Acme"})).await;
    assert_eq!(
        output(&second)["action"],
        "skipped",
        "an echo / unchanged record must not write"
    );

    let third = run(&app, flow_id, json!({"id": "A1", "name": "Acme Ltd"})).await;
    assert_eq!(output(&third)["action"], "updated");
    assert_eq!(app.dst.count("accounts"), 1);
    assert_eq!(
        app.dst.get_record("accounts", &target_id).unwrap()["name"],
        "Acme Ltd"
    );

    // A record that already exists in the target is matched and linked, not duplicated.
    app.dst
        .seed("accounts", "accounts-9", json!({"name": "Beta"}));
    let recovered = run(&app, flow_id, json!({"id": "B1", "name": "Beta"})).await;
    assert_eq!(output(&recovered)["action"], "updated");
    assert_eq!(output(&recovered)["target_id"], "accounts-9");
    assert_eq!(app.dst.count("accounts"), 2);
    app.cleanup().await;
}

#[tokio::test]
async fn lookups_resolve_through_links_and_wait_for_missing_parents() {
    let app = TestApp::spawn().await;
    let src = app.connection("memsrc", "Source").await;
    let dst = app.connection("memdst", "Target").await;
    let accounts = app
        .live_flow(
            "Accounts",
            sync_flow(
                &src,
                &dst,
                "accounts",
                json!({"name": "{{ trigger.name }}"}),
                json!({}),
            ),
        )
        .await;
    let contacts = app
        .live_flow("Contacts", sync_flow(&src, &dst, "contacts", json!({"email": "{{ trigger.email }}"}), json!({
            "lookups": [{"field": "account_id", "link_set": "accounts_link", "source_object": "accounts", "source_record_id": "{{ trigger.account }}"}]
        })))
        .await;
    // Contact arrives before its account: the step waits instead of failing.
    let early = run(
        &app,
        contacts["id"].as_str().unwrap(),
        json!({"id": "C1", "email": "c@x.io", "account": "A1"}),
    )
    .await;
    assert_eq!(early["status"], "waiting", "{early}");
    let account = run(
        &app,
        accounts["id"].as_str().unwrap(),
        json!({"id": "A1", "name": "Acme"}),
    )
    .await;
    let account_target = output(&account)["target_id"].as_str().unwrap().to_owned();
    app.clock.advance(Duration::seconds(61));
    app.drain().await;
    let done = app
        .ok(
            "GET",
            &format!("/api/v1/executions/{}", early["id"].as_str().unwrap()),
            None,
        )
        .await;
    assert_eq!(done["status"], "succeeded", "{done}");
    let contact_id = output(&done)["target_id"].as_str().unwrap().to_owned();
    assert_eq!(
        app.dst.get_record("contacts", &contact_id).unwrap()["account_id"],
        json!(account_target)
    );
    app.cleanup().await;
}

#[tokio::test]
async fn sync_pickers_list_record_connections_modules_and_fields() {
    let app = TestApp::spawn().await;
    let src = app.connection("memsrc", "Source").await;
    let conns = app
        .ok(
            "POST",
            "/api/v1/connectors/core.sync/options/connections",
            Some(json!({"input": {}})),
        )
        .await;
    assert!(
        conns["options"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["value"] == src.as_str())
    );
    let objects = app
        .ok(
            "POST",
            "/api/v1/connectors/core.sync/options/target_objects",
            Some(json!({"input": {"target_connection_id": src}})),
        )
        .await;
    assert_eq!(objects["options"][0]["value"], "accounts");
    let fields = app
        .ok(
            "POST",
            "/api/v1/connectors/core.sync/fields/target_fields",
            Some(json!({"input": {"target_connection_id": src, "target_object": "accounts"}})),
        )
        .await;
    assert_eq!(fields["fields"][0]["key"], "name");
    app.cleanup().await;
}

#[tokio::test]
async fn a_target_linked_to_another_source_is_never_taken_over() {
    let app = TestApp::spawn().await;
    let src = app.connection("memsrc", "Source").await;
    let dst = app.connection("memdst", "Target").await;
    let flow = app
        .live_flow(
            "Contacts",
            sync_flow(
                &src,
                &dst,
                "contacts",
                json!({"email": "{{ trigger.email }}", "name": "{{ trigger.name }}"}),
                json!({"match_field": "email", "match_value": "{{ trigger.email }}"}),
            ),
        )
        .await;
    let flow_id = flow["id"].as_str().unwrap();
    let first = run(
        &app,
        flow_id,
        json!({"id": "A1", "email": "x@acme.io", "name": "Ann"}),
    )
    .await;
    assert_eq!(output(&first)["action"], "created");
    let target_id = output(&first)["target_id"].as_str().unwrap().to_owned();

    // Another source record with the same email must not overwrite or steal A1's target.
    let second = run(
        &app,
        flow_id,
        json!({"id": "A2", "email": "x@acme.io", "name": "Bob"}),
    )
    .await;
    assert_eq!(second["status"], "failed", "{second}");
    assert!(
        second["error_message"]
            .as_str()
            .unwrap_or("")
            .contains("sync.ambiguous_match"),
        "{second}"
    );
    assert_eq!(
        app.dst.get_record("contacts", &target_id).unwrap()["name"],
        "Ann"
    );
    assert_eq!(app.dst.count("contacts"), 1);

    // A1's link is intact: an unchanged re-sync is still skipped.
    let again = run(
        &app,
        flow_id,
        json!({"id": "A1", "email": "x@acme.io", "name": "Ann"}),
    )
    .await;
    assert_eq!(output(&again)["action"], "skipped", "{again}");
    app.cleanup().await;
}

#[tokio::test]
async fn backfill_continues_from_its_cursor_until_every_record_is_started() {
    let app = TestApp::spawn().await;
    let src = app.connection("memsrc", "Source").await;
    for i in 0..250 {
        app.src.seed(
            "accounts",
            &format!("acc-{i:03}"),
            json!({"name": format!("Account {i}")}),
        );
    }
    let flow = app
        .live_flow("Accounts backfill", json!({
            "trigger": {"type": "app", "connector": "memsrc", "trigger": "record_event", "connection_id": src,
                "input": {"module": "accounts"}},
            "steps": [{"key": "s", "type": "stop"}]
        }))
        .await;
    let path = format!("/api/v1/flows/{}/backfill", flow["id"].as_str().unwrap());
    let mut cursor = Value::Null;
    let mut pages = Vec::new();
    loop {
        let page = app
            .ok("POST", &path, Some(json!({"limit": 100, "cursor": cursor})))
            .await;
        pages.push(page["enqueued"].as_u64().unwrap());
        cursor = page["next_cursor"].clone();
        if cursor.is_null() {
            break;
        }
        assert!(pages.len() < 10, "backfill must make progress: {pages:?}");
    }
    assert_eq!(
        pages,
        vec![100, 100, 50],
        "each call continues where the last stopped"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn a_matched_target_is_leased_so_two_sources_cannot_write_it_at_once() {
    use sha2::{Digest as _, Sha256};
    let app = TestApp::spawn().await;
    let src = app.connection("memsrc", "Source").await;
    let dst = app.connection("memdst", "Target").await;
    app.dst.seed(
        "contacts",
        "contacts-9",
        json!({"email": "y@acme.io", "name": "Old"}),
    );
    let flow = app
        .live_flow(
            "Contacts",
            sync_flow(
                &src,
                &dst,
                "contacts",
                json!({"email": "{{ trigger.email }}", "name": "{{ trigger.name }}"}),
                json!({"match_field": "email", "match_value": "{{ trigger.email }}"}),
            ),
        )
        .await;
    // Another source record's sync is writing this target right now (it holds the target lease).
    let key = Sha256::digest(format!("contacts_link|target|{dst}|contacts|contacts-9").as_bytes())
        .to_vec();
    let org = weft_domain::ids::OrgId::from_uuid(weft_server::wiring::DEFAULT_ORG);
    let now = weft_app::ports::Clock::now(&app.clock);
    assert!(
        app.deps
            .store
            .acquire_lease(org, &key, "other-sync", now + Duration::seconds(300), now)
            .await
            .unwrap()
    );

    let waiting = run(
        &app,
        flow["id"].as_str().unwrap(),
        json!({"id": "A3", "email": "y@acme.io", "name": "Ann"}),
    )
    .await;
    assert_eq!(
        waiting["status"], "waiting",
        "the step waits for the other sync: {waiting}"
    );
    assert_eq!(
        app.dst.get_record("contacts", "contacts-9").unwrap()["name"],
        "Old",
        "nothing written"
    );

    app.deps
        .store
        .release_lease(org, &key, "other-sync")
        .await
        .unwrap();
    app.clock.advance(Duration::seconds(61));
    app.drain().await;
    let done = app
        .ok(
            "GET",
            &format!("/api/v1/executions/{}", waiting["id"].as_str().unwrap()),
            None,
        )
        .await;
    assert_eq!(done["status"], "succeeded", "{done}");
    assert_eq!(
        app.dst.get_record("contacts", "contacts-9").unwrap()["name"],
        "Ann"
    );
    app.cleanup().await;
}
