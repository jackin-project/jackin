use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

pub mod profile;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Harness {
    Claude,
    Codex,
}

impl Harness {
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }
}

impl fmt::Display for Harness {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.slug())
    }
}

#[derive(Debug, thiserror::Error)]
#[error("unknown harness: {got:?}; supported: claude, codex")]
pub struct ParseHarnessError {
    got: String,
}

impl FromStr for Harness {
    type Err = ParseHarnessError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "claude" => Ok(Self::Claude),
            "codex" => Ok(Self::Codex),
            other => Err(ParseHarnessError {
                got: other.to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_round_trip() {
        for h in [Harness::Claude, Harness::Codex] {
            assert_eq!(Harness::from_str(h.slug()).unwrap(), h);
        }
    }

    #[test]
    fn display_matches_slug() {
        assert_eq!(format!("{}", Harness::Claude), "claude");
        assert_eq!(format!("{}", Harness::Codex), "codex");
    }

    #[test]
    fn rejects_unknown_harness() {
        let err = Harness::from_str("amp").unwrap_err();
        assert!(err.to_string().contains("amp"));
        assert!(err.to_string().contains("claude"));
    }

    #[test]
    fn serializes_lowercase() {
        let json = serde_json::to_string(&Harness::Claude).unwrap();
        assert_eq!(json, "\"claude\"");
    }

    #[test]
    fn deserializes_lowercase() {
        let h: Harness = serde_json::from_str("\"codex\"").unwrap();
        assert_eq!(h, Harness::Codex);
    }
}
