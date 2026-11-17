//! Appblocks module and field metadata → option lists and the platform field schema.

use weft_connector_sdk::prelude::*;

/// Fields the platform maintains itself (never mapping targets).
const SYSTEM_FIELDS: [&str; 6] = [
    "created_time",
    "modified_time",
    "created_by",
    "modified_by",
    "is_presence",
    "api_id",
];

/// `GET /api/modules` rows → dropdown options (`module_api_name`, `name`).
pub(super) fn module_options(modules: &Value) -> Vec<OptionItem> {
    modules
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|m| {
                    let key = m.get("module_api_name").and_then(Value::as_str)?;
                    let label = m.get("name").and_then(Value::as_str).unwrap_or(key);
                    Some(OptionItem::new(key, label))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// One Appblocks `Field` → `FieldSpec`. Encrypted and attachment fields are skipped.
pub(super) fn field_spec(field: &Value) -> Option<FieldSpec> {
    let api_name = field.get("api_name").and_then(Value::as_str)?;
    if field
        .get("is_encrypted")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return None;
    }
    let label = field
        .get("label_name")
        .and_then(Value::as_str)
        .unwrap_or(api_name);
    let data_type = field
        .get("data_type")
        .and_then(Value::as_str)
        .unwrap_or("text");
    let (kind, multiple) = match data_type {
        "long_text" => (FieldKind::Text, false),
        "integer" | "decimal" => (FieldKind::Number, false),
        "boolean" => (FieldKind::Boolean, false),
        "date" | "date_time" => (FieldKind::Datetime, false),
        "drop_down" => (FieldKind::Select, false),
        "multi_select" => (FieldKind::Select, true),
        "attachment" => return None,
        t if t.starts_with("array") => (FieldKind::Json, false),
        _ => (FieldKind::String, false),
    };
    let mut spec = FieldSpec::new(api_name, label, kind);
    spec.multiple = multiple;
    spec.options = field
        .get("options")
        .and_then(Value::as_array)
        .map(|opts| {
            opts.iter()
                .filter_map(|o| {
                    let value = o.get("value").and_then(Value::as_str)?;
                    Some(OptionItem::new(
                        value,
                        o.get("label").and_then(Value::as_str).unwrap_or(value),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    if matches!(data_type, "lookup" | "multi_lookup" | "user") {
        spec.help = Some("api_id of the related record".into());
    }
    let read_only = matches!(
        data_type,
        "formula" | "auto_number" | "blueprint" | "internal_lookup"
    ) || field
        .get("read_only")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || SYSTEM_FIELDS.contains(&api_name);
    spec.read_only = read_only;
    spec.help = spec.help.or_else(|| {
        field
            .get("tool_tip")
            .and_then(Value::as_str)
            .map(str::to_owned)
    });
    Some(spec)
}

/// `GET /api/fields` rows → field specs.
pub(super) fn field_specs(fields: &Value) -> Vec<FieldSpec> {
    fields
        .as_array()
        .map(|rows| rows.iter().filter_map(field_spec).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_field_types() {
        let fields = json!([
            {"api_name": "status", "label_name": "Status", "data_type": "drop_down", "options": [{"value": "Open", "label": "Open"}, {"value": "Closed"}]},
            {"api_name": "notes", "label_name": "Notes", "data_type": "long_text"},
            {"api_name": "amount", "data_type": "decimal"},
            {"api_name": "job_number", "data_type": "auto_number"},
            {"api_name": "secret", "data_type": "text", "is_encrypted": true},
            {"api_name": "account_id", "data_type": "lookup"},
            {"api_name": "modified_time", "data_type": "date_time"}
        ]);
        let specs = field_specs(&fields);
        assert_eq!(specs.len(), 6);
        assert_eq!(specs[0].kind, FieldKind::Select);
        assert_eq!(specs[0].options[1].label, "Closed");
        assert_eq!(specs[1].kind, FieldKind::Text);
        assert_eq!(specs[2].kind, FieldKind::Number);
        assert!(specs[3].read_only);
        assert_eq!(
            specs[4].help.as_deref(),
            Some("api_id of the related record")
        );
        assert!(specs[5].read_only && specs[5].kind == FieldKind::Datetime);
    }
}
