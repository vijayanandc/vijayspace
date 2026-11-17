//! R7–R9: Zoho CRM ↔ Autodesk Forma (doc 13 §3, MVP 1.1).

use serde_json::{Value, json};

use super::{
    FlowTemplate, Recipe, Slot,
    parts::{cmp, find_source, only_if, zoho_trigger},
};

const PROJECTS: &str = "crm_deals__forma_projects";

fn slots() -> Vec<Slot> {
    vec![
        Slot {
            slot: "crm",
            connector: "zoho_crm",
            label: "Zoho CRM connection",
        },
        Slot {
            slot: "forma",
            connector: "autodesk_forma",
            label: "Autodesk Forma connection",
        },
    ]
}

/// A Forma trigger; the project is chosen in the builder before publishing.
fn forma_trigger(trigger: &str, events: &[&str]) -> Value {
    json!({"type": "app", "connector": "autodesk_forma", "trigger": trigger, "connection_id": "$slot:forma",
        "input": {"project_id": "", "events": events}, "debounce_secs": 30})
}

fn crm_action(key: &str, action: &str, input: Value) -> Value {
    json!({"key": key, "type": "action", "connector": "zoho_crm", "action": action, "connection_id": "$slot:crm", "input": input})
}

fn r7() -> Recipe {
    let create = json!([
        {"key": "project", "type": "action", "connector": "autodesk_forma", "action": "create_project", "connection_id": "$slot:forma",
            "input": {"name": "{{ trigger.record.Deal_Name }}", "type": "Office", "job_number": "{{ trigger.id }}",
                "project_value": "{{ trigger.record.Amount | default(0) }}",
                "start_date": "{{ trigger.record.Closing_Date | default('') }}"}},
        {"key": "link", "type": "action", "connector": "core.sync", "action": "save_link",
            "input": {"link_set": PROJECTS, "source_connection_id": "$slot:crm", "source_object": "Deals", "source_record_id": "{{ trigger.id }}",
                "target_connection_id": "$slot:forma", "target_object": "projects", "target_record_id": "{{ steps.project.id }}"}},
        crm_action("write_back", "update_record", json!({"module": "Deals", "record_id": "{{ trigger.id }}", "trigger_workflows": false,
            "values": {"Forma_Project_ID": "{{ steps.project.id }}"}}))
    ]);
    let definition = json!({"trigger": zoho_trigger("Deals", &["edit"]), "steps": [
        only_if("is_won", "Closed Won", cmp("{{ trigger.record.Stage }}", "eq", json!("Closed Won")), json!([
            {"key": "existing", "type": "action", "connector": "core.sync", "action": "resolve_link",
                "input": {"link_set": PROJECTS, "source_connection_id": "$slot:crm", "source_object": "Deals", "source_record_id": "{{ trigger.id }}"}},
            only_if("no_project", "No project yet", cmp("{{ steps.existing.found }}", "is_false", Value::Null), create)
        ]))
    ]});
    Recipe {
        key: "r7_deal_won_to_forma_project",
        name: "Zoho CRM deal won → Forma Build project",
        description: "Creates one Forma Build project per won deal (in the connection's account), links the two, and writes the project ID back to the deal (custom field Forma_Project_ID).",
        apps: vec!["zoho_crm", "autodesk_forma"],
        slots: slots(),
        flows: vec![FlowTemplate {
            name: "Deal won → Forma project",
            description: "R7: never creates a second project for a deal.",
            definition,
        }],
    }
}

fn r8() -> Recipe {
    let deal = "{{ steps.deal.source_id }}";
    let on_deal = json!({"key": "event_kind", "type": "decision", "branches": [
        {"key": "created", "name": "Issue created", "condition": cmp("{{ trigger.event }}", "eq", json!("issue.created-1.0")), "steps": [
            crm_action("task", "create_task", json!({"subject": "Forma issue #{{ trigger.issue.displayId }}: {{ trigger.issue.title }}",
                "module": "Deals", "record_id": deal, "description": "{{ trigger.issue.description | default('') }}",
                "due_date": "{{ trigger.issue.dueDate | default('') }}"}))
        ]},
        {"key": "closed", "name": "Issue closed", "condition": cmp("{{ trigger.issue.status }}", "eq", json!("closed")), "steps": [
            crm_action("note", "add_note", json!({"module": "Deals", "record_id": deal, "title": "Forma issue closed",
                "content": "Issue #{{ trigger.issue.displayId }} ({{ trigger.issue.title }}) was closed in Forma."}))
        ]}
    ], "otherwise": []});
    let definition = json!({"trigger": forma_trigger("issue_event", &["issue.created-1.0", "issue.updated-1.0"]), "steps": [
        find_source("deal", PROJECTS, ("forma", "projects"), "{{ trigger.project_id }}"),
        only_if("has_deal", "Project linked to a deal", cmp("{{ steps.deal.found }}", "is_true", Value::Null), json!([on_deal]))
    ]});
    Recipe {
        key: "r8_forma_issues_to_crm",
        name: "Forma issues → Zoho CRM tasks & notes",
        description: "A new issue in a linked Forma project becomes a task on the deal; closing it adds a note. Choose the project in the trigger before publishing.",
        apps: vec!["autodesk_forma", "zoho_crm"],
        slots: slots(),
        flows: vec![FlowTemplate {
            name: "Forma issues → deal tasks",
            description: "R8: issue → project → deal.",
            definition,
        }],
    }
}

fn r9() -> Recipe {
    let definition = json!({"trigger": forma_trigger("cost_event", &["changeOrder.created-1.0", "changeOrder.updated-1.0"]), "steps": [
        only_if("approved", "Change order approved", cmp("{{ trigger.payload.status }}", "eq", json!("approved")), json!([
            find_source("deal", PROJECTS, ("forma", "projects"), "{{ trigger.project_id }}"),
            only_if("has_deal", "Project linked to a deal", cmp("{{ steps.deal.found }}", "is_true", Value::Null), json!([
                crm_action("deal_record", "get_record", json!({"module": "Deals", "record_id": "{{ steps.deal.source_id }}"})),
                crm_action("value", "update_record", json!({"module": "Deals", "record_id": "{{ steps.deal.source_id }}", "trigger_workflows": false,
                    "values": {"Contract_Value": "{{ (steps.deal_record.record.Contract_Value | default(steps.deal_record.record.Amount) | default(0)) + trigger.payload.amount }}"}})),
                crm_action("note", "add_note", json!({"module": "Deals", "record_id": "{{ steps.deal.source_id }}", "title": "Change order approved",
                    "content": "{{ trigger.payload.name }}: {{ trigger.payload.amount }} added to the contract value."}))
            ]))
        ]))
    ]});
    Recipe {
        key: "r9_forma_change_orders_to_crm",
        name: "Forma change orders → Zoho CRM deal value",
        description: "An approved change order in a linked Forma project adds its amount to the deal's Contract_Value and adds a note.",
        apps: vec!["autodesk_forma", "zoho_crm"],
        slots: slots(),
        flows: vec![FlowTemplate {
            name: "Forma change orders → deal value",
            description: "R9: project → deal.",
            definition,
        }],
    }
}

pub(super) fn recipes() -> Vec<Recipe> {
    vec![r7(), r8(), r9()]
}
