//! Typed, prefixed identifiers (`flw_0190c3…`) backed by `UUIDv7`.

use std::fmt;

/// An identifier string had the wrong prefix or was not a UUID.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid {kind} id `{value}`")]
pub struct IdError {
    pub kind: &'static str,
    pub value: String,
}

/// Defines one ID newtype. This is the only declarative macro allowed in the domain.
macro_rules! define_id {
    ($name:ident, $prefix:literal) => {
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(uuid::Uuid);

        impl $name {
            pub const PREFIX: &'static str = $prefix;

            #[must_use]
            pub const fn from_uuid(id: uuid::Uuid) -> Self {
                Self(id)
            }

            #[must_use]
            pub const fn as_uuid(&self) -> uuid::Uuid {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}_{}", $prefix, self.0.simple())
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(self, f)
            }
        }

        impl std::str::FromStr for $name {
            type Err = IdError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let err = || IdError {
                    kind: $prefix,
                    value: s.chars().take(64).collect(),
                };
                let rest = s.strip_prefix(concat!($prefix, "_")).ok_or_else(err)?;
                uuid::Uuid::parse_str(rest).map(Self).map_err(|_| err())
            }
        }

        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.collect_str(self)
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = <std::borrow::Cow<'de, str>>::deserialize(d)?;
                s.parse().map_err(serde::de::Error::custom)
            }
        }
    };
}

define_id!(OrgId, "org");
define_id!(FlowId, "flw");
define_id!(FlowVersionId, "fvr");
define_id!(ConnectionId, "cxn");
define_id!(RegistrationId, "trg");
define_id!(TriggerEventId, "tev");
define_id!(ExecutionId, "exe");
define_id!(StepRunId, "stp");
define_id!(JobId, "job");
define_id!(LinkId, "lnk");
define_id!(SubscriptionId, "sub");
define_id!(BackfillId, "bkf");
define_id!(UserId, "usr");
define_id!(InvitationId, "inv");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_with_prefix() {
        let id = FlowId::from_uuid(uuid::Uuid::from_u128(
            0x0190_c3aa_0000_7000_8000_0000_0000_0001,
        ));
        let s = id.to_string();
        assert!(s.starts_with("flw_"));
        assert_eq!(s.parse::<FlowId>().unwrap(), id);
        assert_eq!(serde_json::to_string(&id).unwrap(), format!("\"{s}\""));
    }

    #[test]
    fn wrong_prefix_is_rejected() {
        let id = FlowId::from_uuid(uuid::Uuid::from_u128(7));
        let as_org = id.to_string().replacen("flw_", "org_", 1);
        assert!(as_org.parse::<FlowId>().is_err());
        assert!("flw_nothex".parse::<FlowId>().is_err());
    }
}
