//! MySQL adapter tests (test binary `db`, excluded from `just test-unit`): job queue leases,
//! poison jobs, dedupe keys of dead jobs and daily send quotas.
//! Needs MySQL (`WEFT_TEST_DATABASE_URL`, default `mysql://weft:weft@127.0.0.1:3306`).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use chrono::{DateTime, Duration, TimeZone as _, Utc};
use serde_json::json;
use sqlx::Executor as _;
use weft_app::ports::{ClaimedJob, JobQueue, NewJob, TenancyRepo};
use weft_db::MySqlStore;
use weft_domain::ids::JobId;

struct TestDb {
    store: MySqlStore,
    base: String,
    name: String,
}

async fn db() -> TestDb {
    let base = std::env::var("WEFT_TEST_DATABASE_URL")
        .unwrap_or_else(|_| "mysql://weft:weft@127.0.0.1:3306".into());
    let name = format!("weft_test_{}", uuid::Uuid::new_v4().simple());
    let admin = weft_db::connect(&format!("{base}/mysql"), 1).await.unwrap();
    admin
        .execute(format!("CREATE DATABASE {name}").as_str())
        .await
        .unwrap();
    let pool = weft_db::connect(&format!("{base}/{name}"), 4)
        .await
        .unwrap();
    weft_db::migrate(&pool).await.unwrap();
    TestDb {
        store: MySqlStore::new(pool),
        base,
        name,
    }
}

async fn drop_db(t: TestDb) {
    let admin = weft_db::connect(&format!("{}/mysql", t.base), 1)
        .await
        .unwrap();
    admin
        .execute(format!("DROP DATABASE {}", t.name).as_str())
        .await
        .unwrap();
}

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()
}

fn job(key: &str) -> NewJob {
    NewJob {
        id: JobId::from_uuid(uuid::Uuid::new_v4()),
        queue: "triggers".into(),
        kind: "poll_trigger".into(),
        org_id: None,
        payload: json!({}),
        run_at: t0(),
        dedupe_key: Some(key.into()),
    }
}

async fn claim(store: &MySqlStore, worker: &str, now: DateTime<Utc>) -> Vec<ClaimedJob> {
    store
        .claim(&["triggers"], worker, 10, now, now + Duration::seconds(90))
        .await
        .unwrap()
}

#[tokio::test]
async fn only_the_lease_holder_settles_a_job() {
    let t = db().await;
    assert!(t.store.enqueue(&job("poll:r1"), t0()).await.unwrap());
    let a = claim(&t.store, "worker-a", t0()).await.remove(0);
    // A's lease expires and B reclaims the job.
    let later = t0() + Duration::seconds(120);
    t.store.reap(later).await.unwrap();
    let b = claim(&t.store, "worker-b", later).await.remove(0);
    assert_eq!(a.id, b.id);

    assert!(
        !t.store
            .heartbeat(&a, later + Duration::seconds(90))
            .await
            .unwrap()
    );
    assert!(
        !t.store.complete(&a).await.unwrap(),
        "A no longer holds the lease"
    );
    assert_eq!(t.store.fail(&a, "x", later, later).await.unwrap(), None);
    assert!(
        t.store
            .reschedule(&b, later + Duration::hours(1), later)
            .await
            .unwrap()
    );
    // The job is still there for B's reschedule: A's late complete did not delete it.
    assert!(
        !t.store.enqueue(&job("poll:r1"), later).await.unwrap(),
        "dedupe key still held"
    );
    drop_db(t).await;
}

#[tokio::test]
async fn a_job_whose_worker_keeps_dying_goes_dead_and_frees_its_key() {
    let t = db().await;
    let mut j = job("poll:r2");
    j.id = JobId::from_uuid(uuid::Uuid::new_v4());
    t.store.enqueue(&j, t0()).await.unwrap();
    let mut now = t0();
    let mut claims = 0;
    let mut reported_dead = Vec::new();
    loop {
        let got = claim(&t.store, "w", now).await;
        if got.is_empty() {
            break;
        }
        claims += 1;
        assert!(claims <= 25, "poison job cycles forever");
        now += Duration::seconds(120); // the worker died: the lease expires
        reported_dead.extend(t.store.reap(now).await.unwrap());
    }
    assert_eq!(claims, 25, "dead after max_attempts claims");
    assert_eq!(
        reported_dead,
        vec![json!({})],
        "the dead job is reported so its execution can be failed"
    );
    assert!(
        t.store.enqueue(&job("poll:r2"), now).await.unwrap(),
        "a dead job must not block new work"
    );
    drop_db(t).await;
}

#[tokio::test]
async fn a_due_subscription_is_claimed_by_one_worker_only() {
    use weft_app::ports::{Subscription, TriggerRepo as _};
    use weft_domain::ids::{ConnectionId, OrgId, RegistrationId, SubscriptionId};
    let t = db().await;
    let id = |n: u128| uuid::Uuid::from_u128(n);
    let sub = Subscription {
        id: SubscriptionId::from_uuid(id(1)),
        org_id: OrgId::from_uuid(id(2)),
        registration_id: RegistrationId::from_uuid(id(3)),
        connection_id: ConnectionId::from_uuid(id(4)),
        connector: "zoho_crm".into(),
        external_id: "ch-1".into(),
        data: json!({}),
        status: "active".into(),
        expires_at: Some(t0() + Duration::hours(24)),
        renew_after: Some(t0()),
        last_error: None,
    };
    t.store.upsert_subscription(&sub, t0()).await.unwrap();
    let until = t0() + Duration::minutes(10);
    let first = t
        .store
        .claim_due_subscriptions(t0(), until, 10)
        .await
        .unwrap();
    let second = t
        .store
        .claim_due_subscriptions(t0(), until, 10)
        .await
        .unwrap();
    assert_eq!(first.len(), 1);
    assert!(second.is_empty(), "already claimed by the first worker");
    // If the renewal never reports back, the claim expires and it is retried.
    assert_eq!(
        t.store
            .claim_due_subscriptions(until, until + Duration::minutes(10), 10)
            .await
            .unwrap()
            .len(),
        1
    );
    drop_db(t).await;
}

#[tokio::test]
async fn a_send_counts_against_every_quota_only_when_all_have_room() {
    let t = db().await;
    let day = t0().date_naive();
    let quotas = |org: &str| vec![(format!("org:{org}"), 2), ("user:u".to_owned(), 3)];
    assert!(
        t.store
            .take_send_quota(&quotas("a"), day, t0())
            .await
            .unwrap()
    );
    assert!(
        t.store
            .take_send_quota(&quotas("a"), day, t0())
            .await
            .unwrap()
    );
    assert!(
        !t.store
            .take_send_quota(&quotas("a"), day, t0())
            .await
            .unwrap(),
        "org a is full"
    );
    assert!(
        t.store
            .take_send_quota(&quotas("b"), day, t0())
            .await
            .unwrap()
    );
    assert!(
        !t.store
            .take_send_quota(&quotas("c"), day, t0())
            .await
            .unwrap(),
        "the user has sent 3 today"
    );
    let tomorrow = day.succ_opt().unwrap();
    assert!(
        t.store
            .take_send_quota(&quotas("c"), tomorrow, t0())
            .await
            .unwrap(),
        "a new day starts from zero; the refused send above counted nowhere"
    );
    drop_db(t).await;
}
