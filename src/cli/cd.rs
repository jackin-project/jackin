use clap::Args;
use std::path::Path;

use super::{BANNER, HELP_STYLES};

#[derive(Args, Debug, Clone, PartialEq, Eq)]
#[command(
    before_help = BANNER,
    styles = HELP_STYLES,
    after_long_help = "\
Examples:
  jackin cd the-architect
  jackin cd the-architect /workspace/jackin
  jackin cd jackin-the-architect /workspace/docs"
)]
pub struct CdArgs {
    /// Container short name (e.g. `the-architect`) or full container name.
    pub container: String,
    /// Optional mount destination. Required only when the container has
    /// multiple isolated mounts and stdin is non-interactive.
    pub dst: Option<String>,
}

/// Pure selection helper extracted from the dispatch handler for testability.
///
/// Behavior:
/// - Zero records → error "no isolated mounts".
/// - `dst` provided → exact match or error listing candidates.
/// - One record, no `dst` → return it.
/// - Multiple records, no `dst`: ask `prompt` if `interactive`, else error.
pub fn select_record(
    container_state_dir: &Path,
    dst: Option<&str>,
    interactive: bool,
    prompt: impl FnOnce(&[&str]) -> anyhow::Result<usize>,
) -> anyhow::Result<crate::isolation::state::IsolationRecord> {
    let records = crate::isolation::state::read_records(container_state_dir)?;
    if records.is_empty() {
        anyhow::bail!("container has no isolated mounts");
    }
    if let Some(dst) = dst {
        return records
            .iter()
            .find(|r| r.mount_dst == dst)
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "container has no isolated mount at `{dst}`. Available: {}",
                    records
                        .iter()
                        .map(|r| r.mount_dst.clone())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            });
    }
    if records.len() == 1 {
        return Ok(records.into_iter().next().expect("len checked"));
    }
    if !interactive {
        anyhow::bail!(
            "container has multiple isolated mounts; specify one: {}",
            records
                .iter()
                .map(|r| r.mount_dst.clone())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    let labels: Vec<&str> = records.iter().map(|r| r.mount_dst.as_str()).collect();
    let idx = prompt(&labels)?;
    Ok(records
        .into_iter()
        .nth(idx)
        .expect("prompt returned valid index"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::isolation::MountIsolation;
    use crate::isolation::state::{CleanupStatus, IsolationRecord, write_records};
    use tempfile::TempDir;

    fn rec(dst: &str) -> IsolationRecord {
        IsolationRecord {
            workspace: "jackin".into(),
            mount_dst: dst.into(),
            original_src: "/tmp/src".into(),
            isolation: MountIsolation::Worktree,
            worktree_path: format!("/wt{dst}"),
            scratch_branch: "jackin/scratch/x".into(),
            base_commit: "abc".into(),
            selector_key: "x".into(),
            container_name: "jackin-x".into(),
            cleanup_status: CleanupStatus::Active,
        }
    }

    #[test]
    fn select_record_zero_mounts_errors() {
        let dir = TempDir::new().unwrap();
        let err = select_record(dir.path(), None, false, |_| panic!("no prompt")).unwrap_err();
        assert!(err.to_string().contains("no isolated mounts"));
    }

    #[test]
    fn select_record_single_mount_no_dst_returns_only() {
        let dir = TempDir::new().unwrap();
        let r = rec("/workspace/x");
        write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
        let chosen = select_record(dir.path(), None, false, |_| panic!("no prompt")).unwrap();
        assert_eq!(chosen.mount_dst, r.mount_dst);
    }

    #[test]
    fn select_record_dst_match_returns_record() {
        let dir = TempDir::new().unwrap();
        let r1 = rec("/workspace/a");
        let r2 = rec("/workspace/b");
        write_records(dir.path(), &[r1, r2.clone()]).unwrap();
        let chosen = select_record(dir.path(), Some("/workspace/b"), false, |_| {
            panic!("no prompt")
        })
        .unwrap();
        assert_eq!(chosen.mount_dst, r2.mount_dst);
    }

    #[test]
    fn select_record_unknown_dst_errors_with_candidates() {
        let dir = TempDir::new().unwrap();
        let r = rec("/workspace/a");
        write_records(dir.path(), std::slice::from_ref(&r)).unwrap();
        let err =
            select_record(dir.path(), Some("/nope"), false, |_| panic!("no prompt")).unwrap_err();
        assert!(err.to_string().contains("no isolated mount at `/nope`"));
        assert!(err.to_string().contains("/workspace/a"));
    }

    #[test]
    fn select_record_multi_no_dst_non_tty_errors() {
        let dir = TempDir::new().unwrap();
        write_records(dir.path(), &[rec("/workspace/a"), rec("/workspace/b")]).unwrap();
        let err = select_record(dir.path(), None, false, |_| panic!("no prompt")).unwrap_err();
        assert!(err.to_string().contains("specify one"));
    }

    #[test]
    fn select_record_multi_no_dst_tty_calls_prompt() {
        let dir = TempDir::new().unwrap();
        let r2 = rec("/workspace/b");
        write_records(dir.path(), &[rec("/workspace/a"), r2.clone()]).unwrap();
        let chosen = select_record(dir.path(), None, true, |_labels| Ok(1)).unwrap();
        assert_eq!(chosen.mount_dst, r2.mount_dst);
    }
}
