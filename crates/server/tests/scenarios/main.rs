//! End-to-end scenarios: real router + MySQL + worker drain, manual clock, mock apps.
//! Needs MySQL (`WEFT_TEST_DATABASE_URL`, default `mysql://weft:weft@127.0.0.1:3306`).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines
)]

mod abuse;
mod app_events;
mod connections;
mod custom_functions;
mod hardening;
mod harness;
mod http;
mod lifecycle;
mod limits;
mod oauth;
mod order_alert;
mod recipes;
mod retention;
mod roles;
mod schedule;
mod sync;
mod team_rules;
mod tenancy;
