use serde::Serialize;

/// Output format for list-style and status commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    #[default]
    Human,
    Json,
}

/// Stable JSON envelope for all list-style subcommand output.
///
/// `schema_version` is versioned independently of `config.toml` so callers
/// can detect schema changes without parsing the `data` contents.
#[derive(Debug, Serialize)]
pub struct OutputEnvelope<T: Serialize> {
    pub schema_version: &'static str,
    pub data: T,
}

impl<T: Serialize> OutputEnvelope<T> {
    pub const SCHEMA_V1: &'static str = "v1";

    pub fn v1(data: T) -> Self {
        Self {
            schema_version: Self::SCHEMA_V1,
            data,
        }
    }
}
