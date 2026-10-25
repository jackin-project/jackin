mod adapter;

pub use adapter::{
    OpenCodeQuota, OpenCodeUsageError, classify_opencode_http_error, fetch_opencode_usage,
    load_opencode_api_key, opencode_api_key_snapshot, opencode_profile_snapshot,
    parse_opencode_usage,
};

#[cfg(test)]
pub(crate) use adapter::{
    build_opencode_usage_request, opencode_api_key_view, opencode_profile_view,
};

#[cfg(test)]
mod tests;
