//! Which conditions raise staff alerts.

use chrono::TimeZone as _;
use uuid::Uuid;

use super::*;
use crate::ports::{OrgFailures, QueueDepth, StaleConnection};

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 18, 12, 0, 0).unwrap()
}

#[test]
fn a_healthy_platform_raises_nothing() {
    let queues = QueueStats {
        queues: vec![QueueDepth {
            queue: "executions".into(),
            ready: 3,
            oldest_due: Some(now() - Duration::seconds(20)),
        }],
        running: 2,
        dead: 0,
    };
    let facts = AlertFacts {
        failed_runs: vec![OrgFailures {
            org_id: Uuid::nil(),
            org_name: "Acme".into(),
            failed: 2,
        }],
        ..AlertFacts::default()
    };
    assert!(evaluate(now(), &queues, &facts, 1).is_empty());
}

#[test]
fn lag_dead_jobs_failing_orgs_stale_connections_and_no_worker_are_each_raised_once() {
    let queues = QueueStats {
        queues: vec![QueueDepth {
            queue: "executions".into(),
            ready: 900,
            oldest_due: Some(now() - Duration::minutes(12)),
        }],
        running: 0,
        dead: 4,
    };
    let facts = AlertFacts {
        new_dead_jobs: 4,
        failed_runs: vec![OrgFailures {
            org_id: Uuid::nil(),
            org_name: "Acme".into(),
            failed: 25,
        }],
        needs_reauth: vec![StaleConnection {
            org_id: Uuid::nil(),
            org_name: "Acme".into(),
            connection_id: Uuid::from_u128(7),
            name: "CRM".into(),
            connector: "zoho_crm".into(),
        }],
    };
    let alerts = evaluate(now(), &queues, &facts, 0);
    let keys: Vec<&str> = alerts.iter().map(|a| a.key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "no-worker",
            "lag:executions",
            "dead-jobs",
            "failed:00000000-0000-0000-0000-000000000000",
            "reauth:00000000-0000-0000-0000-000000000007",
        ]
    );
    assert_eq!(alerts[0].severity, "critical");
    assert_eq!(
        alerts[1].title,
        "Jobs in `executions` have waited 12 min (900 due)"
    );
    assert_eq!(alerts[3].title, "25 runs failed in Acme in the last hour");
    assert_eq!(
        alerts[4].title,
        "Connection CRM (zoho_crm) in Acme needs signing in again"
    );
}
