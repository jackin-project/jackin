//! Tests for `github_mounts`.
use super::*;

struct Sources(Vec<String>);

impl WorkspaceMounts for Sources {
    fn mount_sources(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(String::as_str)
    }
}

#[test]
fn cached_resolver_uses_stored_mount_info_without_inspecting_filesystem() {
    let cache = MountInfoCache::default();
    cache.store_entries([
        (
            "/repo".to_string(),
            MountKind::Git {
                branch: GitBranch::Named("main".to_string()),
                origin: Some(GitOrigin::Github {
                    remote_url: "git@github.com:owner/repo.git".to_string(),
                    web_url: "https://github.com/owner/repo/tree/main".to_string(),
                }),
            },
        ),
        ("/plain".to_string(), MountKind::Folder),
    ]);
    let choices = resolve_for_workspace_from_cache(
        &Sources(vec!["/repo".to_string(), "/plain".to_string()]),
        &cache,
    );

    assert_eq!(choices.len(), 1);
    assert_eq!(choices[0].src, "/repo");
    assert_eq!(choices[0].branch, "main");
    assert_eq!(choices[0].url, "https://github.com/owner/repo/tree/main");
}
