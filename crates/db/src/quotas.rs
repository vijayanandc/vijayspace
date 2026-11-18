//! Per-day send quotas (invitation emails): every scope must have room, then all are counted, in
//! one transaction with the rows locked in a fixed order.

use chrono::{DateTime, NaiveDate, Utc};
use weft_app::ports::Res;

use crate::{MySqlStore, codec::db_err};

impl MySqlStore {
    pub(crate) async fn take_quota(
        &self,
        quotas: &[(String, u32)],
        day: NaiveDate,
        _now: DateTime<Utc>,
    ) -> Res<bool> {
        let mut sorted: Vec<&(String, u32)> = quotas.iter().collect();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        for (scope, cap) in &sorted {
            sqlx::query(
                "INSERT INTO send_counters (scope, day, sent) VALUES (?, ?, 0) ON DUPLICATE KEY UPDATE scope = scope",
            )
            .bind(scope)
            .bind(day)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
            let sent: u32 = sqlx::query_scalar(
                "SELECT sent FROM send_counters WHERE scope = ? AND day = ? FOR UPDATE",
            )
            .bind(scope)
            .bind(day)
            .fetch_one(&mut *tx)
            .await
            .map_err(db_err)?;
            if sent >= *cap {
                return Ok(false);
            }
        }
        for (scope, _) in &sorted {
            sqlx::query("UPDATE send_counters SET sent = sent + 1 WHERE scope = ? AND day = ?")
                .bind(scope)
                .bind(day)
                .execute(&mut *tx)
                .await
                .map_err(db_err)?;
        }
        tx.commit().await.map_err(db_err)?;
        Ok(true)
    }
}
