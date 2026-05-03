use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

pub mod profile;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Agent {
    Claude,
    Codex,
}

impl Agent {
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }
}

impl fmt::Display for Agent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.slug())
    }
}

#[derive(Debug, thiserror::Error)]
#[error("unknown agent: {got:?}; supported: claude, codex")]
pub struct ParseAgentError {
    got: String,
}

impl FromStr for Agent {
    type Err = ParseAgentError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "claude" => Ok(Self::Claude),
            "codex" => Ok(Self::Codex),
            other => Err(ParseAgentError {
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
        for h in [Agent::Claude, Agent::Codex] {
            assert_eq!(Agent::from_str(h.slug()).unwrap(), h);
        }
    }

    #[test]
    fn display_matches_slug() {
        assert_eq!(format!("{}", Agent::Claude), "claude");
        assert_eq!(format!("{}", Agent::Codex), "codex");
    }

    #[test]
    fn rejects_unknown_agent() {
        let err = Agent::from_str("amp").unwrap_err();
        assert!(err.to_string().contains("amp"));
        assert!(err.to_string().contains("claude"));
    }

    #[test]
    fn serializes_lowercase() {
        let json = serde_json::to_string(&Agent::Claude).unwrap();
        assert_eq!(json, "\"claude\"");
    }

    #[test]
    fn deserializes_lowercase() {
        let h: Agent = serde_json::from_str("\"codex\"").unwrap();
        assert_eq!(h, Agent::Codex);
    }
}
