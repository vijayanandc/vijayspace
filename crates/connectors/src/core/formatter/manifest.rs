//! The Formatter connector's manifest: its actions and their fields (declarative data).

use weft_connector_sdk::prelude::*;

fn text_field() -> FieldSpec {
    FieldSpec::string("text", "Text").required()
}

/// Every Formatter action with its fields.
#[allow(clippy::too_many_lines)] // declarative manifest data, not logic
pub(super) fn manifest() -> ConnectorManifest {
    let actions = vec![
        ActionSpec::new(
            "text_transform",
            "Text: change case / trim",
            "Upper, lower, title case, capitalise, trim or slugify text.",
        )
        .field(text_field())
        .field(
            FieldSpec::select("operation", "Operation")
                .required()
                .options(&[
                    ("upper", "UPPER CASE"),
                    ("lower", "lower case"),
                    ("title", "Title Case"),
                    ("capitalize", "Capitalize"),
                    ("trim", "Trim spaces"),
                    ("slugify", "Slugify"),
                ]),
        )
        .output(json!({"result": "Ann Lee"})),
        ActionSpec::new(
            "text_replace",
            "Text: find and replace",
            "Replace text, optionally with a regular expression.",
        )
        .field(text_field())
        .field(FieldSpec::string("find", "Find").required())
        .field(FieldSpec::string("replace", "Replace with"))
        .field(FieldSpec::new("regex", "Use regular expression", FieldKind::Boolean).advanced())
        .output(json!({"result": "text"})),
        ActionSpec::new(
            "text_split",
            "Text: split",
            "Split text into a list; optionally pick one item.",
        )
        .field(text_field())
        .field(FieldSpec::string("separator", "Separator").default_value(json!(",")))
        .field(FieldSpec::new("index", "Pick item (0-based)", FieldKind::Number).advanced())
        .output(json!({"result": ["a", "b"], "item": "a"})),
        ActionSpec::new(
            "split_name",
            "Text: split full name",
            "Split a full name into first and last name.",
        )
        .field(FieldSpec::string("full_name", "Full name").required())
        .output(json!({"first_name": "Ann", "last_name": "Lee"})),
        ActionSpec::new(
            "extract",
            "Text: extract",
            "Extract an email, phone number, number, URL or a custom pattern.",
        )
        .field(text_field())
        .field(
            FieldSpec::select("pattern", "What to extract")
                .required()
                .options(&[
                    ("email", "Email"),
                    ("phone", "Phone number"),
                    ("number", "Number"),
                    ("url", "URL"),
                    ("custom", "Custom regex"),
                ]),
        )
        .field(
            FieldSpec::string("regex", "Custom regex").help("Used when 'Custom regex' is selected"),
        )
        .output(json!({"result": "ann@example.com", "all": ["ann@example.com"]})),
        ActionSpec::new(
            "number_format",
            "Number: format",
            "Round and format a number with separators.",
        )
        .field(FieldSpec::string("value", "Number").required())
        .field(FieldSpec::new("decimals", "Decimals", FieldKind::Number).default_value(json!(2)))
        .field(
            FieldSpec::string("thousands_separator", "Thousands separator")
                .default_value(json!(",")),
        )
        .field(
            FieldSpec::string("decimal_separator", "Decimal separator").default_value(json!(".")),
        )
        .output(json!({"result": "1,234.50"})),
        ActionSpec::new(
            "math",
            "Number: arithmetic",
            "Add, subtract, multiply, divide or round.",
        )
        .field(FieldSpec::string("a", "Value A").required())
        .field(
            FieldSpec::select("operation", "Operation")
                .required()
                .options(&[
                    ("add", "A + B"),
                    ("subtract", "A − B"),
                    ("multiply", "A × B"),
                    ("divide", "A ÷ B"),
                    ("round", "Round A to B decimals"),
                ]),
        )
        .field(FieldSpec::string("b", "Value B").required())
        .output(json!({"result": 42})),
        ActionSpec::new(
            "date_format",
            "Date: format",
            "Format a date in a time zone.",
        )
        .field(FieldSpec::string("date", "Date").required())
        .field(
            FieldSpec::string("format", "Format")
                .default_value(json!("%Y-%m-%d"))
                .help("strftime, e.g. %d %b %Y %H:%M"),
        )
        .field(FieldSpec::string("timezone", "Time zone").default_value(json!("UTC")))
        .output(json!({"result": "2026-11-17"})),
        ActionSpec::new(
            "date_add",
            "Date: add or subtract time",
            "Add (or subtract with a negative amount) time to a date.",
        )
        .field(FieldSpec::string("date", "Date").required())
        .field(FieldSpec::new("amount", "Amount", FieldKind::Number).required())
        .field(FieldSpec::select("unit", "Unit").required().options(&[
            ("minutes", "Minutes"),
            ("hours", "Hours"),
            ("days", "Days"),
            ("weeks", "Weeks"),
        ]))
        .output(json!({"result": "2026-11-18T09:00:00Z"})),
        ActionSpec::new(
            "lookup_table",
            "Data: lookup table",
            "Translate a value with a table (e.g. CRM stage → FSM status).",
        )
        .field(FieldSpec::string("value", "Value").required())
        .field(FieldSpec::new("table", "Table", FieldKind::KeyValue).required())
        .field(FieldSpec::string("default", "Default when not found"))
        .output(json!({"result": "Open", "found": true})),
        ActionSpec::new(
            "parse_json",
            "Data: parse JSON text",
            "Turn JSON text into structured data.",
        )
        .field(text_field())
        .output(json!({"result": {"a": 1}})),
    ];
    ConnectorManifest {
        key: "core.formatter".into(),
        name: "Formatter".into(),
        description: "Transform text, numbers, dates and data.".into(),
        icon: "🧮".into(),
        category: "Utilities".into(),
        auth: AuthSpec::none(),
        actions,
        triggers: Vec::new(),
        docs_url: None,
    }
}
