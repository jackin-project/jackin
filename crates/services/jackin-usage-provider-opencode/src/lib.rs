mod adapter;

pub use adapter::{
    OpenCodeQuota, OpenCodeUsageError, classify_opencode_http_error, fetch_opencode_usage,
    load_opencode_api_key, opencode_profile_snapshot, parse_opencode_usage,
};

#[cfg(test)]
mod tests;
