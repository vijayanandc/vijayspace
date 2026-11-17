//! R6: Appblocks HMS → Zoho CRM, consent-gated and limited to allowlisted fields (doc 13 §8).

use serde_json::json;

use super::{
    FlowTemplate, Recipe, Slot,
    parts::{appblocks_trigger, cmp, only_if, upsert},
};

const PATIENTS: &str = "hms_patients__crm_contacts";

/// The only patient fields R6 may send to the CRM (doc 13 §8), with their source templates.
pub(super) const PATIENT_ALLOWLIST: [(&str, &str); 6] = [
    ("First_Name", "{{ trigger.record.first_name }}"),
    (
        "Last_Name",
        "{{ trigger.record.last_name | default(trigger.record.first_name) }}",
    ),
    ("Mobile", "{{ trigger.record.mobile | default('') }}"),
    ("Email", "{{ trigger.record.email | default('') }}"),
    ("Mailing_City", "{{ trigger.record.city | default('') }}"),
    ("UHID", "{{ trigger.record.uhid }}"),
];

pub(super) fn recipes() -> Vec<Recipe> {
    let patient_values: serde_json::Map<String, serde_json::Value> = PATIENT_ALLOWLIST
        .iter()
        .map(|(field, template)| ((*field).to_owned(), json!(template)))
        .collect();
    let patient_values = serde_json::Value::Object(patient_values);
    let patients = json!({"trigger": appblocks_trigger("hms", "patients", "any"), "steps": [
        only_if("consented", "Patient consented to CRM", cmp("{{ trigger.record.crm_consent }}", "is_true", json!(null)), json!([
            upsert("contact", PATIENTS, ("hms", "patients"), ("crm", "Contacts"), patient_values,
                Some(("UHID", "{{ trigger.record.uhid }}")), json!([]))
        ]))
    ]});
    let appointments = json!({"trigger": appblocks_trigger("hms", "appointments", "create"), "steps": [
        {"key": "patient", "type": "action", "connector": "core.sync", "action": "resolve_link",
            "input": {"link_set": PATIENTS, "source_connection_id": "$slot:hms", "source_object": "patients",
                "source_record_id": "{{ trigger.record.patient_id }}"}},
        only_if("synced", "Patient is in the CRM", cmp("{{ steps.patient.found }}", "is_true", json!(null)), json!([
            {"key": "event", "type": "action", "connector": "zoho_crm", "action": "create_record", "connection_id": "$slot:crm",
                "input": {"module": "Events", "trigger_workflows": false, "values": {
                    "Event_Title": "Appointment {{ trigger.record.token_number | default('') }} ({{ trigger.record.appointment_type | default('visit') }})",
                    "Start_DateTime": "{{ trigger.record.appointment_start_at }}",
                    "End_DateTime": "{{ trigger.record.appointment_start_at | date_add(30, 'minutes') }}",
                    "Who_Id": {"id": "{{ steps.patient.target_id }}"}
                }}}
        ]))
    ]});
    vec![Recipe {
        key: "r6_hms_to_crm",
        name: "HMS patients & appointments → Zoho CRM",
        description: "Patients who consented to CRM contact become Zoho contacts (name, mobile, email, city and UHID only). Their appointments become Zoho events. Needs the HMS crm_consent field (AB-6).",
        apps: vec!["appblocks", "zoho_crm"],
        slots: vec![
            Slot {
                slot: "hms",
                connector: "appblocks",
                label: "Appblocks HMS connection",
            },
            Slot {
                slot: "crm",
                connector: "zoho_crm",
                label: "Zoho CRM connection",
            },
        ],
        flows: vec![
            FlowTemplate {
                name: "HMS patients → Zoho contacts",
                description: "R6: consent-gated, allowlisted fields only.",
                definition: patients,
            },
            FlowTemplate {
                name: "HMS appointments → Zoho events",
                description: "R6: one event per appointment of a synced patient.",
                definition: appointments,
            },
        ],
    }]
}
