//! Rendering JSON inputs whose string leaves are templates.

use serde_json::{Map, Value};

use crate::{
    env::env,
    error::ExprError,
    limits::{CappedWriter, MAX_RENDERED_BYTES, check_source},
};

/// Returns the inner expression when `s` is exactly one `{{ … }}` block.
///
/// Such strings are evaluated as expressions so the result keeps its JSON type
/// (a number stays a number, a list stays a list).
#[must_use]
pub fn single_expression(s: &str) -> Option<&str> {
    let inner = s.trim().strip_prefix("{{")?.strip_suffix("}}")?;
    if inner.contains("{{") || inner.contains("}}") || inner.contains("{%") || inner.contains("%}")
    {
        return None;
    }
    let inner = inner.trim();
    if inner.is_empty() { None } else { Some(inner) }
}

fn has_template(s: &str) -> bool {
    s.contains("{{") || s.contains("{%")
}

/// Evaluates one expression (without the braces) against `scope`, returning JSON.
pub fn eval_expression(expr: &str, scope: &Value) -> Result<Value, ExprError> {
    eval_with(expr, &minijinja::Value::from_serialize(scope))
}

fn eval_with(expr: &str, ctx: &minijinja::Value) -> Result<Value, ExprError> {
    check_source(expr).map_err(|m| ExprError::refused(expr, m))?;
    let compiled = env()
        .compile_expression(expr)
        .map_err(|e| ExprError::syntax(expr, &e))?;
    let value = compiled
        .eval(ctx.clone())
        .map_err(|e| ExprError::eval(expr, &e))?;
    to_json(expr, &value)
}

fn to_json(expr: &str, value: &minijinja::Value) -> Result<Value, ExprError> {
    if value.is_undefined() || value.is_none() {
        return Ok(Value::Null);
    }
    serde_json::to_value(value).map_err(|e| ExprError::eval(expr, &e))
}

/// Renders a template string to text. Strings without template markup are returned unchanged.
pub fn render_text(template: &str, scope: &Value) -> Result<String, ExprError> {
    render_text_with(template, &minijinja::Value::from_serialize(scope))
}

fn render_text_with(template: &str, ctx: &minijinja::Value) -> Result<String, ExprError> {
    if !has_template(template) {
        return Ok(template.to_owned());
    }
    check_source(template).map_err(|m| ExprError::refused(template, m))?;
    let compiled = env()
        .template_from_str(template)
        .map_err(|e| ExprError::syntax(template, &e))?;
    let mut out = CappedWriter::default();
    let rendered = compiled.render_captured_to(ctx.clone(), &mut out);
    if out.overflowed {
        let limit = format!(
            "the rendered text is larger than {} MB",
            MAX_RENDERED_BYTES / (1024 * 1024)
        );
        return Err(ExprError::refused(template, limit));
    }
    rendered.map_err(|e| ExprError::eval(template, &e))?;
    String::from_utf8(out.buf).map_err(|e| ExprError::eval(template, &e))
}

/// Renders every string leaf of `input` against `scope` (recursively through arrays and objects).
pub fn render_value(input: &Value, scope: &Value) -> Result<Value, ExprError> {
    let ctx = minijinja::Value::from_serialize(scope);
    render_with(input, &ctx)
}

fn render_with(input: &Value, ctx: &minijinja::Value) -> Result<Value, ExprError> {
    match input {
        Value::String(s) => match single_expression(s) {
            Some(expr) => eval_with(expr, ctx),
            None => render_text_with(s, ctx).map(Value::String),
        },
        Value::Array(items) => items
            .iter()
            .map(|item| render_with(item, ctx))
            .collect::<Result<_, _>>()
            .map(Value::Array),
        Value::Object(map) => {
            let mut out = Map::with_capacity(map.len());
            for (key, value) in map {
                out.insert(key.clone(), render_with(value, ctx)?);
            }
            Ok(Value::Object(out))
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => Ok(input.clone()),
    }
}

/// Checks that a template string compiles, without evaluating it.
pub fn check_template(s: &str) -> Result<(), ExprError> {
    if has_template(s) {
        check_source(s).map_err(|m| ExprError::refused(s, m))?;
    }
    if let Some(expr) = single_expression(s) {
        return env()
            .compile_expression(expr)
            .map(|_| ())
            .map_err(|e| ExprError::syntax(expr, &e));
    }
    if has_template(s) {
        return env()
            .template_from_str(s)
            .map(|_| ())
            .map_err(|e| ExprError::syntax(s, &e));
    }
    Ok(())
}

/// Lists the step keys referenced as `steps.<key>` anywhere in `s` (for scope analysis).
#[must_use]
pub fn referenced_steps(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(pos) = rest.find("steps.") {
        let preceded_by_ident = rest[..pos]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.');
        let after = &rest[pos + "steps.".len()..];
        let key: String = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if !preceded_by_ident && !key.is_empty() && !out.contains(&key) {
            out.push(key);
        }
        rest = after;
    }
    out
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn scope() -> Value {
        json!({
            "trigger": {"body": {"n": 5, "name": " ann lee ", "xs": [1, 2], "email": "A@X.IO", "raw": "{{ 7*7 }}"}},
            "steps": {"fetch": {"rows": [{"id": "r1"}, {"id": "r2"}]}},
            "vars": {"stage": "Closed Won"}
        })
    }

    #[test]
    fn single_expression_keeps_json_type() {
        assert_eq!(
            render_value(&json!("{{ trigger.body.n }}"), &scope()).unwrap(),
            json!(5)
        );
        assert_eq!(
            render_value(&json!("{{ trigger.body.xs }}"), &scope()).unwrap(),
            json!([1, 2])
        );
    }

    #[test]
    fn mixed_text_renders_to_string() {
        assert_eq!(
            render_value(&json!("n={{ trigger.body.n }}"), &scope()).unwrap(),
            json!("n=5")
        );
    }

    #[test]
    fn undefined_is_null_in_typed_mode_and_empty_in_text() {
        assert_eq!(
            render_value(&json!("{{ trigger.nope }}"), &scope()).unwrap(),
            Value::Null
        );
        assert_eq!(
            render_value(&json!("x{{ trigger.nope }}y"), &scope()).unwrap(),
            json!("xy")
        );
    }

    #[test]
    fn nested_objects_and_arrays_are_rendered() {
        let input = json!({"to": ["{{ trigger.body.email | lower }}"], "meta": {"n": "{{ trigger.body.n + 1 }}"}, "k": 3});
        let out = render_value(&input, &scope()).unwrap();
        assert_eq!(out, json!({"to": ["a@x.io"], "meta": {"n": 6}, "k": 3}));
    }

    #[test]
    fn payload_data_is_never_evaluated_as_template() {
        assert_eq!(
            render_value(&json!("{{ trigger.body.raw }}"), &scope()).unwrap(),
            json!("{{ 7*7 }}")
        );
        assert_eq!(
            render_value(&json!("v: {{ trigger.body.raw }}"), &scope()).unwrap(),
            json!("v: {{ 7*7 }}")
        );
    }

    #[test]
    fn plain_strings_pass_through() {
        assert_eq!(
            render_value(&json!("hello"), &scope()).unwrap(),
            json!("hello")
        );
    }

    #[test]
    fn syntax_errors_are_reported() {
        assert!(matches!(
            check_template("{{ trigger. }}"),
            Err(ExprError::Syntax { .. })
        ));
        assert!(check_template("Hi {{ trigger.body.name }}").is_ok());
        assert!(check_template("plain").is_ok());
    }

    #[test]
    fn runaway_loops_exhaust_fuel_instead_of_hanging() {
        let err = render_text(
            "{% for i in range(100000) %}{% for j in range(100000) %}x{% endfor %}{% endfor %}",
            &scope(),
        );
        assert!(err.is_err());
    }

    #[test]
    fn referenced_steps_are_extracted() {
        let refs = referenced_steps(
            "{{ steps.fetch.rows }} and {{ steps.send_mail.id | upper }} not mysteps.x",
        );
        assert_eq!(refs, vec!["fetch".to_owned(), "send_mail".to_owned()]);
    }

    #[test]
    fn single_expression_detection() {
        assert_eq!(single_expression(" {{ a.b }} "), Some("a.b"));
        assert_eq!(single_expression("{{ a }} {{ b }}"), None);
        assert_eq!(single_expression("x {{ a }}"), None);
        assert_eq!(single_expression("{{}}"), None);
    }
}

#[cfg(test)]
mod props {
    use proptest::prelude::*;
    use serde_json::json;

    use super::*;

    proptest! {
        #[test]
        fn rendering_arbitrary_text_never_panics(s in ".{0,200}") {
            let _ = render_value(&json!(s), &json!({"trigger": {"a": 1}}));
        }

        #[test]
        fn data_strings_round_trip_through_typed_passthrough(s in ".{0,100}") {
            let scope = json!({"trigger": {"v": s.clone()}});
            prop_assert_eq!(render_value(&json!("{{ trigger.v }}"), &scope).unwrap(), json!(s));
        }
    }

    #[test]
    fn user_templates_stay_within_the_limits() {
        let scope = json!({});
        let flood = "{% for i in range(60) %}{{ 'x' * 1000000 }}{% endfor %}";
        let err = render_text(flood, &scope).unwrap_err();
        assert!(err.to_string().contains("larger than 10 MB"), "{err}");
        let grow =
            "{% set a = 'x' %}{% for i in range(30) %}{% set a = a ~ a %}{% endfor %}{{ a }}";
        assert!(render_text(grow, &scope).is_err());
        assert!(
            check_template(grow).is_err(),
            "rejected when the flow is saved"
        );
        assert_eq!(
            render_text("{% if true %}ok{% endif %}", &scope).unwrap(),
            "ok"
        );
    }
}
