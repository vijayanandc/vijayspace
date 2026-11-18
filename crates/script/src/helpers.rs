//! Functions scripts may call (doc 08 §7). They delegate to `weft_expr::funcs`, so templates and
//! scripts behave the same. No helper has IO, a clock or randomness.

use rhai::{Dynamic, Engine, EvalAltResult, INT, ImmutableString};
use serde_json::Value;
use weft_expr::funcs;

type Res<T> = Result<T, Box<EvalAltResult>>;

fn fail<T>(message: impl Into<String>) -> Res<T> {
    Err(message.into().into())
}

fn date_value(text: &str) -> Value {
    Value::String(text.to_owned())
}

pub(crate) fn register(engine: &mut Engine) {
    engine.register_fn("parse_json", |text: ImmutableString| -> Res<Dynamic> {
        let value: Value =
            serde_json::from_str(&text).or_else(|e| fail(format!("parse_json: {e}")))?;
        rhai::serde::to_dynamic(&value)
    });
    engine.register_fn("to_json", |value: Dynamic| -> Res<String> {
        let json: Value = rhai::serde::from_dynamic(&value)?;
        Ok(json.to_string())
    });
    register_dates(engine);
    register_text(engine);
}

fn register_dates(engine: &mut Engine) {
    engine.register_fn(
        "format_date",
        |v: ImmutableString, fmt: ImmutableString| -> Res<String> {
            funcs::format_date(&date_value(&v), Some(&fmt), None).or_else(fail)
        },
    );
    engine.register_fn(
        "format_date",
        |v: ImmutableString, fmt: ImmutableString, tz: ImmutableString| -> Res<String> {
            funcs::format_date(&date_value(&v), Some(&fmt), Some(&tz)).or_else(fail)
        },
    );
    engine.register_fn(
        "format_date",
        |epoch: INT, fmt: ImmutableString| -> Res<String> {
            funcs::format_date(&Value::from(epoch), Some(&fmt), None).or_else(fail)
        },
    );
    engine.register_fn(
        "date_add",
        |v: ImmutableString, amount: INT, unit: ImmutableString| -> Res<String> {
            funcs::date_add(&date_value(&v), amount, Some(&unit)).or_else(fail)
        },
    );
}

fn register_text(engine: &mut Engine) {
    engine.register_fn(
        "regex_replace",
        |text: ImmutableString,
         pattern: ImmutableString,
         replacement: ImmutableString|
         -> Res<String> {
            funcs::regex_replace(&text, &pattern, &replacement).or_else(fail)
        },
    );
    engine.register_fn(
        "regex_match",
        |text: ImmutableString, pattern: ImmutableString| -> Res<bool> {
            funcs::regex_is_match(&text, &pattern).or_else(fail)
        },
    );
    engine.register_fn("base64_encode", |v: ImmutableString| {
        funcs::base64_encode(&v)
    });
    engine.register_fn("base64_decode", |v: ImmutableString| -> Res<String> {
        funcs::base64_decode(&v).or_else(fail)
    });
    engine.register_fn("url_encode", |v: ImmutableString| funcs::url_encode(&v));
    engine.register_fn("url_decode", |v: ImmutableString| -> Res<String> {
        funcs::url_decode(&v).or_else(fail)
    });
    engine.register_fn("sha256", |v: ImmutableString| funcs::sha256_hex(&v));
    engine.register_fn(
        "hmac_sha256",
        |v: ImmutableString, key: ImmutableString| -> Res<String> {
            funcs::hmac_sha256_hex(&v, &key).or_else(fail)
        },
    );
}
