//! Recipes: catalog, install as drafts with slot checks, and the drafts validate against the
//! real connector catalog.

use serde_json::{Value, json};

use crate::harness::TestApp;

#[tokio::test]
async fn every_recipe_installs_as_valid_drafts() {
    let app = TestApp::spawn().await;
    let crm = app.stored_connection("zoho_crm", "CRM").await;
    let fsm = app.stored_connection("appblocks", "FSM").await;
    let forma = app.stored_connection("autodesk_forma", "Forma").await;
    let ids = json!({"crm": crm, "fsm": fsm, "hms": fsm, "forma": forma});
    let recipes = app.ok("GET", "/api/v1/recipes", None).await;
    let recipes = recipes.as_array().unwrap();
    assert!(recipes.len() >= 9);
    for recipe in recipes {
        let key = recipe["key"].as_str().unwrap();
        assert!(!recipe["slots"].as_array().unwrap().is_empty(), "{key}");
        let installed = app
            .ok(
                "POST",
                &format!("/api/v1/recipes/{key}/install"),
                Some(json!({"connections": ids})),
            )
            .await;
        let flow_ids = installed["flow_ids"].as_array().unwrap();
        assert_eq!(
            flow_ids.len(),
            recipe["flows"].as_array().unwrap().len(),
            "{key}"
        );
        for id in flow_ids {
            let id = id.as_str().unwrap();
            let flow = app.ok("GET", &format!("/api/v1/flows/{id}"), None).await;
            assert_eq!(
                flow["is_enabled"], false,
                "{key}: recipes install as drafts"
            );
            let issues = app
                .ok("POST", &format!("/api/v1/flows/{id}/validate"), None)
                .await;
            let errors: Vec<&Value> = issues["issues"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|i| i["severity"] == "error")
                // Forma triggers need a project picked in the builder; nothing else may be missing.
                .filter(|i| !i["path"].as_str().unwrap_or("").contains("project_id"))
                .collect();
            assert!(errors.is_empty(), "{key} / {id}: {errors:?}");
        }
    }
    app.cleanup().await;
}

#[tokio::test]
async fn install_checks_slots_before_creating_anything() {
    let app = TestApp::spawn().await;
    let crm = app.stored_connection("zoho_crm", "CRM").await;
    let fsm = app.stored_connection("appblocks", "FSM").await;
    let path = "/api/v1/recipes/r1_crm_accounts_contacts_to_fsm/install";

    let (status, body) = app
        .req("POST", path, Some(json!({"connections": {"crm": crm}})))
        .await;
    assert_eq!(status.as_u16(), 422, "{body}");
    assert_eq!(body["code"], "recipe.slot_missing");

    let (status, body) = app
        .req(
            "POST",
            path,
            Some(json!({"connections": {"crm": fsm, "fsm": fsm}})),
        )
        .await;
    assert_eq!(status.as_u16(), 422, "{body}");
    assert_eq!(body["code"], "recipe.slot_mismatch");

    let (status, body) = app
        .req(
            "POST",
            "/api/v1/recipes/nope/install",
            Some(json!({"connections": {}})),
        )
        .await;
    assert_eq!(status.as_u16(), 404, "{body}");
    assert_eq!(body["code"], "recipe.not_found");

    let flows = app.ok("GET", "/api/v1/flows", None).await;
    assert_eq!(flows["items"].as_array().map_or(0, Vec::len), 0, "{flows}");
    app.cleanup().await;
}
