//! The paging cursor of `RecordObjects::changed_since` (used by backfill): records sorted by
//! modified time, read from `since` with an offset, re-anchored on a later modified time before
//! the app's offset limit. Ties (many records with one modified time) never stall the cursor.

use serde_json::{Value, json};

/// `{since, skip}`; a plain string is accepted as `since` (older cursors).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ListCursor {
    pub since: Option<String>,
    pub skip: u32,
}

impl ListCursor {
    pub(crate) fn parse(value: Option<&Value>) -> Self {
        match value {
            Some(Value::String(since)) => Self {
                since: Some(since.clone()),
                skip: 0,
            },
            Some(Value::Object(o)) => Self {
                since: o.get("since").and_then(Value::as_str).map(str::to_owned),
                skip: o
                    .get("skip")
                    .and_then(Value::as_u64)
                    .and_then(|s| u32::try_from(s).ok())
                    .unwrap_or(0),
            },
            Some(_) | None => Self::default(),
        }
    }

    pub(crate) fn to_value(&self) -> Value {
        json!({"since": self.since, "skip": self.skip})
    }

    /// The cursor after a full page whose records have these modified times (in order).
    /// Pages by offset until `max_skip`; then restarts at the last modified time, skipping the
    /// records with that time already read (`keep_ties`) or none (apps that page by page number,
    /// where the repeats are deduplicated downstream).
    pub(crate) fn next(&self, modified: &[Option<String>], max_skip: u32, keep_ties: bool) -> Self {
        let read = u32::try_from(modified.len()).unwrap_or(u32::MAX);
        let skip = self.skip.saturating_add(read);
        let last = modified.iter().rev().find_map(Clone::clone);
        match last {
            Some(last) if skip > max_skip => {
                let ties = modified
                    .iter()
                    .rev()
                    .take_while(|m| m.as_deref() == Some(last.as_str()))
                    .count();
                let ties = u32::try_from(ties).unwrap_or(u32::MAX);
                // Every record of the page has the same time as `since`: re-anchoring would not
                // move, so keep the offset (the app's limit is then the hard stop).
                if self.since.as_deref() == Some(last.as_str()) && ties == read {
                    return Self {
                        since: self.since.clone(),
                        skip,
                    };
                }
                Self {
                    since: Some(last),
                    skip: if keep_ties { ties } else { 0 },
                }
            }
            Some(_) | None => Self {
                since: self.since.clone(),
                skip,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn times(ts: &[&str]) -> Vec<Option<String>> {
        ts.iter().map(|t| Some((*t).to_owned())).collect()
    }

    #[test]
    fn pages_by_offset_then_reanchors_on_the_last_time_keeping_ties() {
        let start = ListCursor::default();
        let next = start.next(&times(&["t1", "t2", "t3"]), 100, true);
        assert_eq!(
            next,
            ListCursor {
                since: None,
                skip: 3
            }
        );
        let near_limit = ListCursor {
            since: None,
            skip: 99,
        };
        let re = near_limit.next(&times(&["t5", "t6", "t6"]), 100, true);
        assert_eq!(
            re,
            ListCursor {
                since: Some("t6".into()),
                skip: 2
            },
            "skip the two t6 already read"
        );
        let by_page = near_limit.next(&times(&["t5", "t6", "t6"]), 100, false);
        assert_eq!(
            by_page,
            ListCursor {
                since: Some("t6".into()),
                skip: 0
            }
        );
    }

    #[test]
    fn many_records_with_one_time_never_stall() {
        // 250 records share t1: offsets keep advancing even past the re-anchor point.
        let mut c = ListCursor {
            since: Some("t1".into()),
            skip: 0,
        };
        let page = times(&["t1"; 100]);
        let mut seen = 0;
        for _ in 0..3 {
            c = c.next(&page, 150, true);
            seen += 100;
            assert_eq!(c.skip, seen, "progress on every page");
        }
    }

    #[test]
    fn parses_old_string_cursors_and_round_trips() {
        assert_eq!(
            ListCursor::parse(Some(&json!("t9"))),
            ListCursor {
                since: Some("t9".into()),
                skip: 0
            }
        );
        let c = ListCursor {
            since: Some("t1".into()),
            skip: 7,
        };
        assert_eq!(ListCursor::parse(Some(&c.to_value())), c);
        assert_eq!(ListCursor::parse(None), ListCursor::default());
    }
}
