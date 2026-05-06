use crate::docker::CommandRunner;

use super::repo_cache::{git_branch, git_repo_name, is_git_dir};

pub(super) struct GitIdentity {
    pub(super) user_name: String,
    pub(super) user_email: String,
}

pub(super) struct HostIdentity {
    pub(super) uid: String,
    pub(super) gid: String,
}

/// Run a command and return its trimmed stdout, or `None` on failure.
pub(super) fn try_capture(
    runner: &mut impl CommandRunner,
    program: &str,
    args: &[&str],
) -> Option<String> {
    runner
        .capture(program, args, None)
        .ok()
        .filter(|s| !s.is_empty())
}

pub(super) fn load_git_identity(runner: &mut impl CommandRunner) -> GitIdentity {
    GitIdentity {
        user_name: try_capture(runner, "git", &["config", "user.name"]).unwrap_or_default(),
        user_email: try_capture(runner, "git", &["config", "user.email"]).unwrap_or_default(),
    }
}

#[cfg(unix)]
pub(super) fn load_host_identity(runner: &mut impl CommandRunner) -> HostIdentity {
    HostIdentity {
        uid: try_capture(runner, "id", &["-u"]).unwrap_or_else(|| "1000".to_string()),
        gid: try_capture(runner, "id", &["-g"]).unwrap_or_else(|| "1000".to_string()),
    }
}

#[cfg(not(unix))]
pub(super) fn load_host_identity(_runner: &mut impl CommandRunner) -> HostIdentity {
    HostIdentity {
        uid: "1000".to_string(),
        gid: "1000".to_string(),
    }
}

pub(super) fn build_config_rows(
    agent_display_name: &str,
    container_name: &str,
    workspace: &crate::workspace::ResolvedWorkspace,
    git: &GitIdentity,
    image: &str,
    runner: &mut impl CommandRunner,
) -> Vec<(String, String)> {
    // Who
    let mut rows = vec![("identity".to_string(), agent_display_name.to_string())];
    if !git.user_name.is_empty() {
        rows.push((
            "operator".to_string(),
            if git.user_email.is_empty() {
                git.user_name.clone()
            } else {
                format!("{} <{}>", git.user_name, git.user_email)
            },
        ));
    }

    // Where
    let workdir = std::path::Path::new(&workspace.label);
    if workdir.is_absolute() && is_git_dir(workdir, runner) {
        if let Some(repo_name) = git_repo_name(workdir, runner) {
            rows.push(("repository".to_string(), repo_name));
        }
        if let Some(branch) = git_branch(workdir, runner) {
            rows.push(("branch".to_string(), branch));
        }
    } else {
        rows.push(("workspace".to_string(), workspace.label.clone()));
    }

    // Runtime
    rows.push(("container".to_string(), container_name.to_string()));
    rows.push(("image".to_string(), image.to_string()));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_rows_show_repo_and_branch_for_git_directory() {
        // Use the jackin repo itself as a known git directory
        let cwd = std::env::current_dir().unwrap();
        let workspace = crate::workspace::ResolvedWorkspace {
            label: cwd.display().to_string(),
            workdir: cwd.display().to_string(),
            mounts: vec![],
            default_agent: None,
            keep_awake_enabled: false,
        git_pull_on_entry: false,
        };
        let git = GitIdentity {
            user_name: String::new(),
            user_email: String::new(),
        };

        let rows = build_config_rows(
            "Role",
            "jackin-role",
            &workspace,
            &git,
            "img",
            &mut crate::docker::ShellRunner::default(),
        );

        let labels: Vec<&str> = rows.iter().map(|(l, _)| l.as_str()).collect();
        assert!(labels.contains(&"repository"));
        assert!(labels.contains(&"branch"));
        assert!(!labels.contains(&"workspace"));
        assert!(!labels.contains(&"dind"));
    }

    #[test]
    fn config_rows_show_workspace_for_saved_workspace() {
        let workspace = crate::workspace::ResolvedWorkspace {
            label: "big-monorepo".to_string(),
            workdir: "/workspace/project".to_string(),
            mounts: vec![],
            default_agent: None,
            keep_awake_enabled: false,
        git_pull_on_entry: false,
        };
        let git = GitIdentity {
            user_name: "Alice".to_string(),
            user_email: "alice@example.com".to_string(),
        };

        let rows = build_config_rows(
            "Role",
            "jackin-role",
            &workspace,
            &git,
            "img",
            &mut crate::docker::ShellRunner::default(),
        );

        let labels: Vec<&str> = rows.iter().map(|(l, _)| l.as_str()).collect();
        assert!(labels.contains(&"workspace"));
        assert!(!labels.contains(&"repository"));
        assert!(!labels.contains(&"branch"));
        assert!(!labels.contains(&"dind"));

        let ws_value = rows.iter().find(|(l, _)| l == "workspace").unwrap();
        assert_eq!(ws_value.1, "big-monorepo");
    }

    #[test]
    fn config_rows_omit_dind() {
        let workspace = crate::workspace::ResolvedWorkspace {
            label: "test".to_string(),
            workdir: "/workspace".to_string(),
            mounts: vec![],
            default_agent: None,
            keep_awake_enabled: false,
        git_pull_on_entry: false,
        };
        let git = GitIdentity {
            user_name: String::new(),
            user_email: String::new(),
        };

        let rows = build_config_rows(
            "Role",
            "jackin-role",
            &workspace,
            &git,
            "img",
            &mut crate::docker::ShellRunner::default(),
        );

        assert!(!rows.iter().any(|(l, _)| l == "dind"));
    }
}
