mod adapter;

pub use adapter::{
    GEMINI_CONSUMER_OAUTH_END, GeminiEntitlement, GeminiProjectQuota,
    gemini_consumer_oauth_retired, gemini_credential_origin, gemini_credential_presence,
    gemini_migration_action, gemini_oauth_creds_path, gemini_quota_buckets, gemini_snapshot,
    gemini_snapshot_with_presence, parse_gemini_entitlement, parse_gemini_project_quotas,
};

#[cfg(test)]
mod tests;
