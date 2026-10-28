use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

macro_rules! define_id {
    ($name:ident, $prefix:literal) => {
        #[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn generate() -> Self {
                Self(format!("{}_{}", $prefix, Uuid::new_v4().simple()))
            }

            pub fn new(val: impl Into<String>) -> Self {
                Self(val.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self.0)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl FromStr for $name {
            type Err = std::convert::Infallible;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self(s.to_string()))
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}

define_id!(RequestId, "req");
define_id!(CorrelationId, "corr");
define_id!(SessionId, "sess");
define_id!(ClientId, "client");
define_id!(SubscriptionId, "sub");
define_id!(ApprovalId, "appr");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_id_generation_and_prefix() {
        let req = RequestId::generate();
        assert!(req.as_str().starts_with("req_"));
        assert_eq!(req.to_string(), req.as_str());

        let corr = CorrelationId::generate();
        assert!(corr.as_str().starts_with("corr_"));

        let appr = ApprovalId::generate();
        assert!(appr.as_str().starts_with("appr_"));
    }

    #[test]
    fn test_id_serde() {
        let req = RequestId::generate();
        let json = serde_json::to_string(&req).expect("serialize");
        let decoded: RequestId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(req, decoded);
    }
}
