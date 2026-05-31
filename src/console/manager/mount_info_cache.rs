pub use jackin_console::mount_info_cache::MountInfoCache;
pub(crate) use jackin_console::mount_info_cache::MountSource;

impl MountSource for crate::workspace::MountConfig {
    fn mount_src(&self) -> &str {
        &self.src
    }
}

impl MountSource for crate::config::GlobalMountRow {
    fn mount_src(&self) -> &str {
        &self.mount.src
    }
}
