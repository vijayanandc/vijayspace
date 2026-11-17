//! Schedule specifications and their next fire time (time-zone and DST aware).

use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

fn utc() -> String {
    "UTC".to_owned()
}

/// When a schedule trigger fires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScheduleSpec {
    /// Once, at a local date-time (`2026-12-01T09:00`) in `timezone`.
    Once {
        at: String,
        #[serde(default = "utc")]
        timezone: String,
    },
    /// Every N minutes, aligned to the epoch (so every worker computes the same slots).
    Interval { minutes: u32 },
    /// A 5-field cron expression evaluated in `timezone`.
    Cron {
        expr: String,
        #[serde(default = "utc")]
        timezone: String,
    },
}

impl ScheduleSpec {
    /// Returns an error message when the spec cannot produce fire times.
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Once { at, timezone } => parse_local(at, parse_tz(timezone)?).map(|_| ()),
            Self::Interval { minutes } => {
                if (1..=525_600).contains(minutes) {
                    Ok(())
                } else {
                    Err("Interval must be between 1 minute and 1 year".to_owned())
                }
            }
            Self::Cron { expr, timezone } => {
                parse_tz(timezone)?;
                parse_cron(expr).map(|_| ())
            }
        }
    }

    /// The first fire time strictly after `after`, or `None` when the schedule has ended.
    pub fn next_after(&self, after: DateTime<Utc>) -> Result<Option<DateTime<Utc>>, String> {
        match self {
            Self::Once { at, timezone } => {
                let at = parse_local(at, parse_tz(timezone)?)?;
                Ok((at > after).then_some(at))
            }
            Self::Interval { minutes } => {
                let step = i64::from(*minutes) * 60;
                if step <= 0 {
                    return Err("Interval must be positive".to_owned());
                }
                let next = (after.timestamp().div_euclid(step) + 1) * step;
                Ok(DateTime::from_timestamp(next, 0))
            }
            Self::Cron { expr, timezone } => {
                let tz = parse_tz(timezone)?;
                let cron = parse_cron(expr)?;
                cron.find_next_occurrence(&after.with_timezone(&tz), false)
                    .map(|t| Some(t.with_timezone(&Utc)))
                    .map_err(|e| e.to_string())
            }
        }
    }
}

fn parse_tz(name: &str) -> Result<Tz, String> {
    name.parse::<Tz>()
        .map_err(|_| format!("Unknown time zone `{name}`"))
}

fn parse_cron(expr: &str) -> Result<croner::Cron, String> {
    croner::Cron::new(expr)
        .parse()
        .map_err(|e| format!("Invalid cron expression: {e}"))
}

fn parse_local(at: &str, tz: Tz) -> Result<DateTime<Utc>, String> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(at) {
        return Ok(dt.with_timezone(&Utc));
    }
    let naive = ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M"]
        .iter()
        .find_map(|f| NaiveDateTime::parse_from_str(at, f).ok())
        .ok_or_else(|| format!("Invalid date-time `{at}`"))?;
    tz.from_local_datetime(&naive)
        .earliest()
        .map(|t| t.with_timezone(&Utc))
        .ok_or_else(|| format!("`{at}` does not exist in {tz} (daylight-saving gap)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn cron_in_time_zone() {
        let spec = ScheduleSpec::Cron {
            expr: "0 9 * * *".into(),
            timezone: "Asia/Kolkata".into(),
        };
        assert_eq!(
            spec.next_after(ts("2026-01-01T00:00:00Z")).unwrap(),
            Some(ts("2026-01-01T03:30:00Z"))
        );
    }

    #[test]
    fn cron_across_us_spring_dst_gap() {
        let spec = ScheduleSpec::Cron {
            expr: "30 9 * * *".into(),
            timezone: "America/New_York".into(),
        };
        // 2026-03-08 is the US spring-forward day; 09:30 EDT = 13:30Z (was 14:30Z the day before).
        assert_eq!(
            spec.next_after(ts("2026-03-07T15:00:00Z")).unwrap(),
            Some(ts("2026-03-08T13:30:00Z"))
        );
    }

    #[test]
    fn interval_is_epoch_aligned() {
        let spec = ScheduleSpec::Interval { minutes: 15 };
        assert_eq!(
            spec.next_after(ts("2026-01-01T10:07:12Z")).unwrap(),
            Some(ts("2026-01-01T10:15:00Z"))
        );
        assert_eq!(
            spec.next_after(ts("2026-01-01T10:15:00Z")).unwrap(),
            Some(ts("2026-01-01T10:30:00Z"))
        );
    }

    #[test]
    fn once_fires_once() {
        let spec = ScheduleSpec::Once {
            at: "2026-12-01T09:00".into(),
            timezone: "Asia/Kolkata".into(),
        };
        assert_eq!(
            spec.next_after(ts("2026-11-01T00:00:00Z")).unwrap(),
            Some(ts("2026-12-01T03:30:00Z"))
        );
        assert_eq!(spec.next_after(ts("2026-12-02T00:00:00Z")).unwrap(), None);
    }

    #[test]
    fn invalid_specs_are_rejected() {
        assert!(
            ScheduleSpec::Cron {
                expr: "nope".into(),
                timezone: "UTC".into()
            }
            .validate()
            .is_err()
        );
        assert!(
            ScheduleSpec::Cron {
                expr: "* * * * *".into(),
                timezone: "Mars/Base".into()
            }
            .validate()
            .is_err()
        );
        assert!(ScheduleSpec::Interval { minutes: 0 }.validate().is_err());
    }

    proptest::proptest! {
        #[test]
        fn next_is_strictly_after(secs in 0i64..4_000_000_000, minutes in 1u32..1000) {
            let after = DateTime::from_timestamp(secs, 0).unwrap();
            let next = ScheduleSpec::Interval { minutes }.next_after(after).unwrap().unwrap();
            proptest::prop_assert!(next > after);
        }
    }
}
