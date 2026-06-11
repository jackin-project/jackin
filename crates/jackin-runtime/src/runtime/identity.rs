//! Capture host git user.name/email for in-container git defaults.
//!
//! All reads are best-effort: missing git config or id failures produce empty
//! strings rather than hard errors.

use jackin_core::CommandRunner;

pub(super) struct GitIdentity {
    pub(super) user_name: String,
    pub(super) user_email: String,
}

pub(super) async fn try_capture(
    runner: &mut impl CommandRunner,
    program: &str,
    args: &[&str],
) -> Option<String> {
    runner
        .capture(program, args, None)
        .await
        .ok()
        .filter(|s| !s.is_empty())
}

pub(super) async fn load_git_identity(runner: &mut impl CommandRunner) -> GitIdentity {
    GitIdentity {
        user_name: try_capture(runner, "git", &["config", "user.name"])
            .await
            .unwrap_or_default(),
        user_email: try_capture(runner, "git", &["config", "user.email"])
            .await
            .unwrap_or_default(),
    }
}
