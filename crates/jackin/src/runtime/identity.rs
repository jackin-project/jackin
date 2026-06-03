//! Capture host git user.name/email and UID/GID for derived-image UID remapping.
//!
//! All reads are best-effort: missing git config or id failures produce empty
//! strings or zeros rather than hard errors. Not responsible for applying the
//! identity to the image — callers in `image.rs` pass it as build-args.

use crate::docker::CommandRunner;

pub(super) struct GitIdentity {
    pub(super) user_name: String,
    pub(super) user_email: String,
}

pub(super) struct HostIdentity {
    pub(super) uid: String,
    pub(super) gid: String,
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

#[cfg(unix)]
pub(super) async fn load_host_identity(runner: &mut impl CommandRunner) -> HostIdentity {
    HostIdentity {
        uid: try_capture(runner, "id", &["-u"])
            .await
            .unwrap_or_else(|| "1000".to_string()),
        gid: try_capture(runner, "id", &["-g"])
            .await
            .unwrap_or_else(|| "1000".to_string()),
    }
}

#[cfg(not(unix))]
pub(super) async fn load_host_identity(_runner: &mut impl CommandRunner) -> HostIdentity {
    HostIdentity {
        uid: "1000".to_string(),
        gid: "1000".to_string(),
    }
}
