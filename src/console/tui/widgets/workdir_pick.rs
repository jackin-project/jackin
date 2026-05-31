pub use jackin_console::widgets::workdir_pick::*;

impl WorkdirMount for crate::workspace::MountConfig {
    fn dst(&self) -> &str {
        &self.dst
    }
}
