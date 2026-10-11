//! jackin-usage-provider-gemini: `Gemini CLI` eligibility + project-quota snapshot.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`gemini_snapshot`] — `Gemini` usage snapshot.
//!
//! Gemini CLI is a separate account from Antigravity even though both ride
//! Google OAuth. Consumer Google OAuth through Gemini CLI ended 2026-06-18;
//! Standard and Enterprise remain supported. Gemini/Vertex API-key billing is
//! project-scoped RPM/TPM/daily/model quotas, parsed into count buckets
//! without inventing denominators.

mod adapter;

pub use adapter::{
    GEMINI_CONSUMER_OAUTH_END, GeminiEntitlement, GeminiProjectQuota,
    gemini_consumer_oauth_retired, gemini_credential_origin, gemini_credential_presence,
    gemini_migration_action, gemini_oauth_creds_path, gemini_quota_buckets, gemini_snapshot,
    gemini_snapshot_with_presence, parse_gemini_entitlement, parse_gemini_project_quotas,
};

#[cfg(test)]
mod tests;
