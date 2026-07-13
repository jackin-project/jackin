//! Typed failure classes for probing the 1Password `op` CLI. Constructed at
//! the process boundary (jackin-env); consumed by pickers/UI without
//! substring matching. Attached as an anyhow source so it survives `?`
//! propagation and is recovered by `downcast_ref`.

/// Why an `op` CLI probe failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OpProbeError {
    /// The `op` binary could not be spawned (not installed / not on PATH).
    #[error("failed to spawn op: {detail}")]
    NotInstalled {
        /// Spawn/IO detail from the process boundary.
        detail: String,
    },
    /// The CLI ran but reports no signed-in account.
    #[error("1Password CLI is not signed in: {detail}")]
    NotSignedIn {
        /// Operator-facing detail from `op` (no secrets).
        detail: String,
    },
    /// The probe timed out.
    #[error("op timed out after {seconds}s")]
    Timeout {
        /// Timeout budget in whole seconds.
        seconds: u64,
    },
    /// Any other failure; carries the raw message.
    #[error("{message}")]
    Other {
        /// Raw failure text (no secret material).
        message: String,
    },
}

#[cfg(test)]
mod tests;
