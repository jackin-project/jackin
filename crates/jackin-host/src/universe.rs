//! Host-side environment-flag helper.
//!
//! Mirror of `jackin-runtime::runtime::universe::env_flag_enabled` for the
//! one site the clipboard path needs. Kept inline to avoid a circular
//! `jackin-host` → `jackin-runtime` dependency.

pub(crate) fn env_flag_enabled(value: Option<impl AsRef<std::ffi::OsStr>>) -> bool {
    let Some(value) = value else {
        return false;
    };
    let Some(value) = value.as_ref().to_str() else {
        return true;
    };
    !matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "" | "0" | "false" | "no" | "off"
    )
}
