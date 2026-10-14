#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OpProbeError {
    #[error("failed to spawn op: {detail}")]
    NotInstalled { detail: String },

    #[error("1Password CLI is not signed in: {detail}")]
    NotSignedIn { detail: String },

    #[error("op timed out after {seconds}s")]
    Timeout { seconds: u64 },

    #[error("{message}")]
    Other { message: String },
}

#[cfg(test)]
mod tests;
