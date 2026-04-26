//! Create-workspace mounts-first wizard state transitions.
//!
//! Flow: `PickFirstMountSrc` → `PickFirstMountDst` → `PickWorkdir` → `NameWorkspace` → (drop into editor).

use std::path::PathBuf;

use super::state::{CreatePreludeState, CreateStep};
use crate::workspace::{MountConfig, WorkspaceConfig};

impl CreatePreludeState<'_> {
    pub fn accept_mount_src(&mut self, src: PathBuf) {
        self.pending_mount_src = Some(src);
        self.step = CreateStep::PickFirstMountDst;
    }

    /// Default mount dst = same absolute path as host src. Operator can
    /// overwrite in the dst modal.
    pub fn default_mount_dst(&self) -> Option<String> {
        self.pending_mount_src
            .as_ref()
            .map(|p| p.display().to_string())
    }

    pub fn accept_mount_dst(&mut self, dst: String, readonly: bool) {
        self.pending_mount_dst = Some(dst);
        self.pending_readonly = readonly;
        self.step = CreateStep::PickWorkdir;
    }

    pub fn accept_workdir(&mut self, workdir: String) {
        self.pending_workdir = Some(workdir);
        self.step = CreateStep::NameWorkspace;
    }

    /// Default name = mount dst basename.
    pub fn default_name(&self) -> Option<String> {
        self.pending_mount_dst.as_ref().and_then(|dst| {
            std::path::Path::new(dst)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
        })
    }

    pub fn accept_name(&mut self, name: String) {
        self.pending_name = Some(name);
    }

    /// Produce the `WorkspaceConfig` for commit. Returns None if any
    /// required field is missing (unit guard; UX gates should prevent).
    pub fn build_workspace(&self) -> Option<WorkspaceConfig> {
        let src = self.pending_mount_src.as_ref()?;
        let dst = self.pending_mount_dst.as_ref()?;
        let workdir = self.pending_workdir.as_ref()?;

        Some(WorkspaceConfig {
            workdir: workdir.clone(),
            mounts: vec![MountConfig {
                src: src.display().to_string(),
                dst: dst.clone(),
                readonly: self.pending_readonly,
            }],
            ..WorkspaceConfig::default()
        })
    }

    pub fn name(&self) -> Option<&str> {
        self.pending_name.as_deref()
    }

    /// The wizard is complete iff a name, a mount source, a mount dst,
    /// and a workdir have all been captured. Returns the owned pair the
    /// dispatcher needs to transition to the editor; returns None when
    /// any field is still missing (the dispatcher then stays on the
    /// current wizard step).
    ///
    /// Prefer this over individually checking fields — the returned
    /// tuple guarantees by type that every required value is present,
    /// so the dispatcher no longer needs `expect("prelude complete")`.
    pub fn completed(&self) -> Option<(String, WorkspaceConfig)> {
        let name = self.pending_name.clone()?;
        let workspace = self.build_workspace()?;
        Some((name, workspace))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_state_is_at_first_step() {
        let s = CreatePreludeState::new();
        assert!(matches!(s.step, CreateStep::PickFirstMountSrc));
    }

    #[test]
    fn accepting_mount_src_advances_to_dst() {
        let mut s = CreatePreludeState::new();
        s.accept_mount_src(PathBuf::from("/home/user/p"));
        assert!(matches!(s.step, CreateStep::PickFirstMountDst));
    }

    #[test]
    fn default_dst_equals_src_path() {
        let mut s = CreatePreludeState::new();
        s.accept_mount_src(PathBuf::from("/home/user/p"));
        assert_eq!(s.default_mount_dst().as_deref(), Some("/home/user/p"));
    }

    #[test]
    fn default_name_is_dst_basename() {
        let mut s = CreatePreludeState::new();
        s.accept_mount_src(PathBuf::from("/home/user/my-app"));
        s.accept_mount_dst("/home/user/my-app".into(), false);
        assert_eq!(s.default_name().as_deref(), Some("my-app"));
    }

    #[test]
    fn full_happy_path_builds_workspace() {
        let mut s = CreatePreludeState::new();
        s.accept_mount_src(PathBuf::from("/home/user/my-app"));
        s.accept_mount_dst("/home/user/my-app".into(), false);
        s.accept_workdir("/home/user/my-app".into());
        s.accept_name("my-app".into());
        let ws = s.build_workspace().unwrap();
        assert_eq!(ws.workdir, "/home/user/my-app");
        assert_eq!(ws.mounts.len(), 1);
        assert_eq!(ws.mounts[0].src, "/home/user/my-app");
        assert_eq!(ws.mounts[0].dst, "/home/user/my-app");
    }

    #[test]
    fn incomplete_state_does_not_build() {
        let s = CreatePreludeState::new();
        assert!(s.build_workspace().is_none());
    }
}
