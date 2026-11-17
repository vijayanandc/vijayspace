//! Zoho CRM module and field metadata → option lists and the platform field schema.

use weft_connector_sdk::prelude::*;

/// `GET /settings/modules` → API-supported modules as options (`api_name`, `plural_label`).
pub(super) fn module_options(body: &Value) -> Vec<OptionItem> {
    body.get("modules")
        .and_then(Value::as_array)
        .map(|mods| {
            mods.iter()
                .filter(|m| {
                    m.get("api_supported")
                        .and_then(Value::as_bool)
                        .unwrap_or(true)
                })
                .filter_map(|m| {
                    let key = m.get("api_name").and_then(Value::as_str)?;
                    let label = m
                        .get("plural_label")
                        .or_else(|| m.get("module_name"))
                        .and_then(Value::as_str)
                        .unwrap_or(key);
                    Some(OptionItem::new(key, label))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// One Zoho field → `FieldSpec`.
pub(super) fn field_spec(field: &Value) -> Option<FieldSpec> {
    let api_name = field.get("api_name").and_then(Value::as_str)?;
    let label = field
        .get("field_label")
        .or_else(|| field.get("display_label"))
        .and_then(Value::as_str)
        .unwrap_or(api_name);
    let data_type = field
        .get("data_type")
        .and_then(Value::as_str)
        .unwrap_or("text");
    let (kind, multiple) = match data_type {
        "textarea" => (FieldKind::Text, false),
        "integer" | "double" | "currency" | "percent" | "bigint" | "decimal" | "long" => {
            (FieldKind::Number, false)
        }
        "boolean" => (FieldKind::Boolean, false),
        "date" | "datetime" => (FieldKind::Datetime, false),
        "picklist" => (FieldKind::Select, false),
        "multiselectpicklist" => (FieldKind::Select, true),
        "subform" | "multiselectlookup" => (FieldKind::Json, false),
        _ => (FieldKind::String, false),
    };
    let mut spec = FieldSpec::new(api_name, label, kind);
    spec.multiple = multiple;
    spec.required = field
        .get("system_mandatory")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    spec.options = field
        .get("pick_list_values")
        .and_then(Value::as_array)
        .map(|vals| {
            vals.iter()
                .filter_map(|v| {
                    let actual = v.get("actual_value").and_then(Value::as_str)?;
                    Some(OptionItem::new(
                        actual,
                        v.get("display_value")
                            .and_then(Value::as_str)
                            .unwrap_or(actual),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    if matches!(data_type, "lookup" | "ownerlookup" | "userlookup") {
        spec.help = Some("Record ID".into());
    }
    spec.read_only = matches!(data_type, "formula" | "autonumber" | "rollup_summary")
        || field
            .get("read_only")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        || field
            .get("field_read_only")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    if spec.read_only {
        spec.required = false;
    }
    Some(spec)
}

/// `GET /settings/fields` → specs.
pub(super) fn field_specs(body: &Value) -> Vec<FieldSpec> {
    body.get("fields")
        .and_then(Value::as_array)
        .map(|f| f.iter().filter_map(field_spec).collect())
        .unwrap_or_default()
}

/// Field API names for the `fields` query parameter (Zoho caps it at 50), `Modified_Time` first.
pub(super) fn list_field_names(specs: &[FieldSpec]) -> String {
    let mut names = vec!["Modified_Time".to_owned()];
    names.extend(
        specs
            .iter()
            .map(|f| f.key.clone())
            .filter(|k| k != "Modified_Time"),
    );
    names.truncate(50);
    names.join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_zoho_field_types() {
        let body = json!({"fields": [
            {"api_name": "Stage", "field_label": "Stage", "data_type": "picklist", "system_mandatory": true,
             "pick_list_values": [{"display_value": "Closed Won", "actual_value": "Closed Won"}]},
            {"api_name": "Amount", "field_label": "Amount", "data_type": "currency"},
            {"api_name": "Tags", "data_type": "multiselectpicklist"},
            {"api_name": "Account_Name", "data_type": "lookup"},
            {"api_name": "Deal_No", "data_type": "autonumber", "system_mandatory": true},
            {"api_name": "Description", "data_type": "textarea"}
        ]});
        let specs = field_specs(&body);
        assert_eq!(specs[0].kind, FieldKind::Select);
        assert!(specs[0].required);
        assert_eq!(specs[0].options[0].value, "Closed Won");
        assert_eq!(specs[1].kind, FieldKind::Number);
        assert!(specs[2].multiple);
        assert_eq!(specs[3].help.as_deref(), Some("Record ID"));
        assert!(specs[4].read_only && !specs[4].required);
        assert_eq!(specs[5].kind, FieldKind::Text);
        assert!(list_field_names(&specs).starts_with("Modified_Time,Stage,Amount"));
    }

    #[test]
    fn module_options_skip_unsupported() {
        let body = json!({"modules": [
            {"api_name": "Deals", "plural_label": "Deals", "api_supported": true},
            {"api_name": "Home", "plural_label": "Home", "api_supported": false}
        ]});
        assert_eq!(
            module_options(&body),
            vec![OptionItem::new("Deals", "Deals")]
        );
    }
}
