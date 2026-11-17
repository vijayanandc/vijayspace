//! R1–R5: Zoho CRM ↔ Appblocks FSM (doc 13 §3).

use serde_json::json;

use super::{
    FlowTemplate, Recipe, Slot,
    parts::{appblocks_trigger, cmp, find_source, only_if, upsert, zoho_trigger},
};

const ACCOUNTS: &str = "crm_accounts__fsm_accounts";
const CONTACTS: &str = "crm_contacts__fsm_contacts";
const DEALS: &str = "crm_deals__fsm_requests";
const PRODUCTS: &str = "crm_products__fsm_items";

fn crm() -> Slot {
    Slot {
        slot: "crm",
        connector: "zoho_crm",
        label: "Zoho CRM connection",
    }
}

fn fsm() -> Slot {
    Slot {
        slot: "fsm",
        connector: "appblocks",
        label: "Appblocks FSM connection",
    }
}

/// The Zoho record lookup → FSM account link (used by contacts and deals).
fn account_lookup(source_id: &str) -> serde_json::Value {
    json!([{"field": "account_id", "link_set": ACCOUNTS, "source_object": "Accounts", "source_record_id": source_id}])
}

fn r1() -> Recipe {
    let accounts = json!({"trigger": zoho_trigger("Accounts", &["create", "edit"]), "steps": [
        upsert("sync_account", ACCOUNTS, ("crm", "Accounts"), ("fsm", "accounts"), json!({
            "name": "{{ trigger.record.Account_Name }}",
            "external_id": "{{ trigger.id }}",
            "phone": "{{ trigger.record.Phone | default('') }}",
            "website": "{{ trigger.record.Website | default('') }}",
            "street1": "{{ trigger.record.Billing_Street | default('') }}",
            "city": "{{ trigger.record.Billing_City | default('') }}",
            "state": "{{ trigger.record.Billing_State | default('') }}",
            "zip": "{{ trigger.record.Billing_Code | default('') }}",
            "country": "{{ trigger.record.Billing_Country | default('') }}"
        }), Some(("external_id", "{{ trigger.id }}")), json!([]))
    ]});
    let contacts = json!({"trigger": zoho_trigger("Contacts", &["create", "edit"]), "steps": [
        upsert("sync_contact", CONTACTS, ("crm", "Contacts"), ("fsm", "contacts"), json!({
            "name": "{{ trigger.record.First_Name | default('') }} {{ trigger.record.Last_Name }}",
            "email": "{{ trigger.record.Email | default('') }}",
            "mobile": "{{ trigger.record.Mobile | default(trigger.record.Phone) | default('') }}"
        }), Some(("email", "{{ trigger.record.Email }}")), account_lookup("{{ trigger.record.Account_Name.id }}"))
    ]});
    Recipe {
        key: "r1_crm_accounts_contacts_to_fsm",
        name: "Zoho CRM accounts & contacts → FSM",
        description: "Keeps FSM accounts and contacts in step with Zoho CRM. Accounts match on external_id = Zoho ID, contacts on email; a contact's account is resolved through the account link.",
        apps: vec!["zoho_crm", "appblocks"],
        slots: vec![crm(), fsm()],
        flows: vec![
            FlowTemplate {
                name: "Zoho accounts → FSM accounts",
                description: "R1: create or update the linked FSM account.",
                definition: accounts,
            },
            FlowTemplate {
                name: "Zoho contacts → FSM contacts",
                description: "R1: create or update the linked FSM contact.",
                definition: contacts,
            },
        ],
    }
}

fn r2() -> Recipe {
    let won = only_if(
        "is_won",
        "Closed Won",
        cmp("{{ trigger.record.Stage }}", "eq", json!("Closed Won")),
        json!([
            upsert("request", DEALS, ("crm", "Deals"), ("fsm", "requests"), json!({
                "summary": "{{ trigger.record.Deal_Name }}",
                "notes": "{{ trigger.record.Description | default('') }}",
                "status": "New"
            }), None, account_lookup("{{ trigger.record.Account_Name.id }}")),
            {"key": "fsm_request", "type": "action", "connector": "appblocks", "action": "get_record",
                "connection_id": "$slot:fsm", "input": {"module": "requests", "record_id": "{{ steps.request.target_id }}"}},
            {"key": "write_back", "type": "action", "connector": "zoho_crm", "action": "update_record",
                "connection_id": "$slot:crm", "input": {"module": "Deals", "record_id": "{{ trigger.id }}",
                    "values": {"FSM_Request_Number": "{{ steps.fsm_request.record.request_number }}"}, "trigger_workflows": false}}
        ]),
    );
    let definition = json!({"trigger": zoho_trigger("Deals", &["edit"]), "steps": [won]});
    Recipe {
        key: "r2_deal_won_to_fsm_request",
        name: "Zoho CRM deal won → FSM service request",
        description: "When a deal moves to Closed Won, creates an FSM request for its account and writes the request number back to the deal (custom field FSM_Request_Number).",
        apps: vec!["zoho_crm", "appblocks"],
        slots: vec![crm(), fsm()],
        flows: vec![FlowTemplate {
            name: "Deal won → FSM request",
            description: "R2: one FSM request per won deal, never duplicated.",
            definition,
        }],
    }
}

fn r3() -> Recipe {
    let stage = "{{ trigger.record.status | translate({\"Scheduled\": \"Scheduled\", \"In Progress\": \"In Progress\", \"Completed\": \"Job Completed\", \"Cancelled\": \"Closed Lost\"}) }}";
    let definition = json!({"trigger": appblocks_trigger("fsm", "jobs", "update"), "steps": [
        find_source("deal", DEALS, ("fsm", "requests"), "{{ trigger.record.request_id }}"),
        only_if("has_deal", "Linked to a deal", cmp("{{ steps.deal.found }}", "is_true", json!(null)), json!([
            {"key": "stage", "type": "action", "connector": "zoho_crm", "action": "update_record", "connection_id": "$slot:crm",
                "input": {"module": "Deals", "record_id": "{{ steps.deal.source_id }}", "values": {"Stage": stage}, "trigger_workflows": false}},
            {"key": "note", "type": "action", "connector": "zoho_crm", "action": "add_note", "connection_id": "$slot:crm",
                "input": {"module": "Deals", "record_id": "{{ steps.deal.source_id }}", "title": "FSM job update",
                    "content": "Job {{ trigger.record.job_number }} is now {{ trigger.record.status }}."}}
        ]))
    ]});
    Recipe {
        key: "r3_fsm_job_status_to_deal",
        name: "FSM job status → Zoho CRM deal stage + note",
        description: "Finds the deal through the request link, maps the job status to a deal stage with a value map, and adds a note to the deal.",
        apps: vec!["appblocks", "zoho_crm"],
        slots: vec![fsm(), crm()],
        flows: vec![FlowTemplate {
            name: "FSM job status → deal",
            description: "R3: job → request → deal.",
            definition,
        }],
    }
}

fn r4() -> Recipe {
    let definition = json!({"trigger": appblocks_trigger("fsm", "invoices", "update"), "steps": [
        only_if("is_paid", "Paid", cmp("{{ trigger.record.status }}", "eq", json!("Paid")), json!([
            {"key": "job", "type": "action", "connector": "appblocks", "action": "get_record", "connection_id": "$slot:fsm",
                "input": {"module": "jobs", "record_id": "{{ trigger.record.job_id }}"}},
            find_source("deal", DEALS, ("fsm", "requests"), "{{ steps.job.record.request_id }}"),
            only_if("has_deal", "Linked to a deal", cmp("{{ steps.deal.found }}", "is_true", json!(null)), json!([
                {"key": "paid", "type": "action", "connector": "zoho_crm", "action": "update_record", "connection_id": "$slot:crm",
                    "input": {"module": "Deals", "record_id": "{{ steps.deal.source_id }}", "trigger_workflows": false,
                        "values": {"Payment_Status": "Paid", "Paid_On": "{{ trigger.record.modified_time | date('%Y-%m-%d') }}"}}},
                {"key": "note", "type": "action", "connector": "zoho_crm", "action": "add_note", "connection_id": "$slot:crm",
                    "input": {"module": "Deals", "record_id": "{{ steps.deal.source_id }}", "title": "Invoice paid",
                        "content": "Invoice {{ trigger.record.invoice_number }} ({{ trigger.record.total }}) was paid."}}
            ]))
        ]))
    ]});
    Recipe {
        key: "r4_fsm_invoice_paid_to_deal",
        name: "FSM invoice paid → Zoho CRM deal",
        description: "When an FSM invoice is paid, sets Payment_Status and Paid_On on the linked deal and adds a note.",
        apps: vec!["appblocks", "zoho_crm"],
        slots: vec![fsm(), crm()],
        flows: vec![FlowTemplate {
            name: "FSM invoice paid → deal",
            description: "R4: invoice → job → request → deal.",
            definition,
        }],
    }
}

fn r5() -> Recipe {
    let definition = json!({"trigger": zoho_trigger("Products", &["create", "edit"]), "steps": [
        upsert("sync_item", PRODUCTS, ("crm", "Products"), ("fsm", "items"), json!({
            "name": "{{ trigger.record.Product_Name }}",
            "sku": "{{ trigger.record.Product_Code | default('') }}",
            "description": "{{ trigger.record.Description | default('') }}",
            "selling_price": "{{ trigger.record.Unit_Price | default(0) }}",
            "external_id": "{{ trigger.id }}",
            "is_active": "{{ trigger.record.Product_Active | default(true) }}"
        }), Some(("external_id", "{{ trigger.id }}")), json!([]))
    ]});
    Recipe {
        key: "r5_crm_products_to_fsm_items",
        name: "Zoho CRM products → FSM items",
        description: "Creates or updates FSM items from Zoho CRM products, matching on external_id.",
        apps: vec!["zoho_crm", "appblocks"],
        slots: vec![crm(), fsm()],
        flows: vec![FlowTemplate {
            name: "Zoho products → FSM items",
            description: "R5: price and SKU kept in step.",
            definition,
        }],
    }
}

pub(super) fn recipes() -> Vec<Recipe> {
    vec![r1(), r2(), r3(), r4(), r5()]
}
