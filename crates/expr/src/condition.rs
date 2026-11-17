//! Condition trees for decision branches (`all` / `any` / `not` / comparison leaves).
//!
//! Evaluation never fails: a leaf whose operands cannot be compared evaluates to `false`
//! and adds a warning, so a decision can never crash a run.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{dates, template::render_value};

/// A boolean expression over rendered template operands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Condition {
    /// True when every child is true (an empty list is true).
    All { all: Vec<Condition> },
    /// True when at least one child is true (an empty list is false).
    Any { any: Vec<Condition> },
    /// Negation.
    Not { not: Box<Condition> },
    /// A single comparison.
    Leaf(Comparison),
}

/// One comparison: `left op right`. Operands are templates rendered against the scope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Comparison {
    pub left: Value,
    pub op: Op,
    #[serde(default)]
    pub right: Value,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub case_insensitive: bool,
}

/// Comparison operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Op {
    Eq,
    Neq,
    Gt,
    Gte,
    Lt,
    Lte,
    Contains,
    NotContains,
    StartsWith,
    EndsWith,
    In,
    NotIn,
    IsEmpty,
    IsNotEmpty,
    IsTrue,
    IsFalse,
    MatchesRegex,
    DateBefore,
    DateAfter,
    Exists,
    NotExists,
}

/// Outcome of evaluating a condition.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConditionResult {
    pub matched: bool,
    pub warnings: Vec<String>,
}

/// Evaluates `condition` against `scope`.
#[must_use]
pub fn eval_condition(condition: &Condition, scope: &Value) -> ConditionResult {
    let mut warnings = Vec::new();
    let matched = eval(condition, scope, &mut warnings);
    ConditionResult { matched, warnings }
}

fn eval(condition: &Condition, scope: &Value, warnings: &mut Vec<String>) -> bool {
    match condition {
        Condition::All { all } => all.iter().all(|c| eval(c, scope, warnings)),
        Condition::Any { any } => any.iter().any(|c| eval(c, scope, warnings)),
        Condition::Not { not } => !eval(not, scope, warnings),
        Condition::Leaf(cmp) => eval_leaf(cmp, scope, warnings),
    }
}

fn eval_leaf(cmp: &Comparison, scope: &Value, warnings: &mut Vec<String>) -> bool {
    let left = render_value(&cmp.left, scope);
    let right = render_value(&cmp.right, scope);
    match (left, right) {
        (Ok(l), Ok(r)) => compare(cmp.op, &l, &r, cmp.case_insensitive, warnings),
        (Err(e), _) | (_, Err(e)) => {
            warnings.push(format!("condition operand could not be rendered: {e}"));
            false
        }
    }
}

fn compare(op: Op, l: &Value, r: &Value, ci: bool, warnings: &mut Vec<String>) -> bool {
    match op {
        Op::Eq => equals(l, r, ci),
        Op::Neq => !equals(l, r, ci),
        Op::Gt => ordering(l, r, ci) == Some(Ordering::Greater),
        Op::Gte => matches!(
            ordering(l, r, ci),
            Some(Ordering::Greater | Ordering::Equal)
        ),
        Op::Lt => ordering(l, r, ci) == Some(Ordering::Less),
        Op::Lte => matches!(ordering(l, r, ci), Some(Ordering::Less | Ordering::Equal)),
        Op::Contains => contains(l, r, ci),
        Op::NotContains => !contains(l, r, ci),
        Op::StartsWith => fold(&text(l), ci).starts_with(&fold(&text(r), ci)),
        Op::EndsWith => fold(&text(l), ci).ends_with(&fold(&text(r), ci)),
        Op::In => member_of(l, r, ci),
        Op::NotIn => !member_of(l, r, ci),
        Op::IsEmpty => is_empty(l),
        Op::IsNotEmpty => !is_empty(l),
        Op::IsTrue => truthy(l),
        Op::IsFalse => !truthy(l),
        Op::MatchesRegex => regex_match(l, r, ci, warnings),
        Op::DateBefore => date_cmp(l, r, warnings) == Some(Ordering::Less),
        Op::DateAfter => date_cmp(l, r, warnings) == Some(Ordering::Greater),
        Op::Exists => !l.is_null(),
        Op::NotExists => l.is_null(),
    }
}

/// Text form of a value: strings as-is, null as empty, everything else as JSON.
fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Array(_) | Value::Object(_) => v.to_string(),
    }
}

fn fold(s: &str, ci: bool) -> String {
    if ci { s.to_lowercase() } else { s.to_owned() }
}

fn number(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) if !s.trim().is_empty() => s.trim().parse::<f64>().ok(),
        Value::String(_) | Value::Null | Value::Bool(_) | Value::Array(_) | Value::Object(_) => {
            None
        }
    }
}

fn equals(l: &Value, r: &Value, ci: bool) -> bool {
    match (number(l), number(r)) {
        (Some(a), Some(b)) => (a - b).abs() < f64::EPSILON,
        _ => fold(&text(l), ci) == fold(&text(r), ci),
    }
}

fn ordering(l: &Value, r: &Value, ci: bool) -> Option<Ordering> {
    if let (Some(a), Some(b)) = (number(l), number(r)) {
        return a.partial_cmp(&b);
    }
    if let (Some(a), Some(b)) = (dates::parse_json(l), dates::parse_json(r)) {
        return Some(a.cmp(&b));
    }
    Some(fold(&text(l), ci).cmp(&fold(&text(r), ci)))
}

fn contains(l: &Value, r: &Value, ci: bool) -> bool {
    match l {
        Value::Array(items) => items.iter().any(|item| equals(item, r, ci)),
        Value::String(_) | Value::Null | Value::Bool(_) | Value::Number(_) | Value::Object(_) => {
            fold(&text(l), ci).contains(&fold(&text(r), ci))
        }
    }
}

fn member_of(l: &Value, r: &Value, ci: bool) -> bool {
    match r {
        Value::Array(items) => items.iter().any(|item| equals(l, item, ci)),
        Value::String(s) => s
            .split(',')
            .any(|part| equals(l, &Value::String(part.trim().to_owned()), ci)),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::Object(_) => equals(l, r, ci),
    }
}

fn is_empty(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::String(s) => s.trim().is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
        Value::Bool(_) | Value::Number(_) => false,
    }
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::String(s) => matches!(
            s.trim().to_lowercase().as_str(),
            "true" | "1" | "yes" | "y" | "on"
        ),
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::Null | Value::Array(_) | Value::Object(_) => false,
    }
}

fn regex_match(l: &Value, r: &Value, ci: bool, warnings: &mut Vec<String>) -> bool {
    match regex::RegexBuilder::new(&text(r))
        .case_insensitive(ci)
        .size_limit(1 << 16)
        .build()
    {
        Ok(re) => re.is_match(&text(l)),
        Err(e) => {
            warnings.push(format!("invalid regular expression: {e}"));
            false
        }
    }
}

fn date_cmp(l: &Value, r: &Value, warnings: &mut Vec<String>) -> Option<Ordering> {
    if let (Some(a), Some(b)) = (dates::parse_json(l), dates::parse_json(r)) {
        Some(a.cmp(&b))
    } else {
        warnings.push(format!("cannot compare as dates: {l} and {r}"));
        None
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn check(cond: Value, scope: &Value) -> bool {
        let c: Condition = serde_json::from_value(cond).unwrap();
        eval_condition(&c, scope).matched
    }

    fn scope() -> Value {
        json!({"trigger": {"body": {"amount": "250", "country": "IN", "vip": true, "tags": ["a", "b"], "name": "Ann", "at": "2026-01-02T00:00:00Z"}}})
    }

    #[test]
    fn numeric_comparison_coerces_strings() {
        let s = scope();
        assert!(check(
            json!({"left": "{{ trigger.body.amount }}", "op": "gt", "right": 100}),
            &s
        ));
        assert!(!check(
            json!({"left": "{{ trigger.body.amount }}", "op": "lt", "right": "100"}),
            &s
        ));
        assert!(check(
            json!({"left": "{{ trigger.body.amount }}", "op": "eq", "right": 250}),
            &s
        ));
    }

    #[test]
    fn groups_combine() {
        let s = scope();
        let cond = json!({"any": [
            {"all": [
                {"left": "{{ trigger.body.amount }}", "op": "gt", "right": 1000},
                {"left": "{{ trigger.body.country }}", "op": "in", "right": ["IN", "US"]}
            ]},
            {"left": "{{ trigger.body.vip }}", "op": "is_true"}
        ]});
        assert!(check(cond, &s));
        assert!(check(json!({"all": []}), &s));
        assert!(!check(json!({"any": []}), &s));
        assert!(check(
            json!({"not": {"left": "{{ trigger.body.vip }}", "op": "is_false"}}),
            &s
        ));
    }

    #[test]
    fn text_operators() {
        let s = scope();
        assert!(check(
            json!({"left": "{{ trigger.body.name }}", "op": "eq", "right": "ann", "case_insensitive": true}),
            &s
        ));
        assert!(!check(
            json!({"left": "{{ trigger.body.name }}", "op": "eq", "right": "ann"}),
            &s
        ));
        assert!(check(
            json!({"left": "{{ trigger.body.tags }}", "op": "contains", "right": "b"}),
            &s
        ));
        assert!(check(
            json!({"left": "{{ trigger.body.country }}", "op": "in", "right": "US, IN"}),
            &s
        ));
        assert!(check(
            json!({"left": "{{ trigger.body.name }}", "op": "starts_with", "right": "An"}),
            &s
        ));
        assert!(check(
            json!({"left": "{{ trigger.body.missing }}", "op": "is_empty"}),
            &s
        ));
        assert!(check(
            json!({"left": "{{ trigger.body.missing }}", "op": "not_exists"}),
            &s
        ));
    }

    #[test]
    fn dates_and_regex() {
        let s = scope();
        assert!(check(
            json!({"left": "{{ trigger.body.at }}", "op": "date_before", "right": "2026-02-01"}),
            &s
        ));
        assert!(check(
            json!({"left": "{{ trigger.body.name }}", "op": "matches_regex", "right": "^A.n$"}),
            &s
        ));
    }

    #[test]
    fn errors_evaluate_false_with_warning() {
        let c: Condition =
            serde_json::from_value(json!({"left": "x", "op": "matches_regex", "right": "("}))
                .unwrap();
        let r = eval_condition(&c, &scope());
        assert!(!r.matched);
        assert_eq!(r.warnings.len(), 1);
    }
}
