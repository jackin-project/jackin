//! Host-side snapshot types shared between the console UI and the
//! runtime fetch layer.

use crate::control::TabSnapshot;

/// Current tab/pane state fetched from a running container's daemon.
#[derive(Debug, Clone)]
pub struct InstanceSnapshot {
    /// `tabs` field.
    pub tabs: Vec<TabSnapshot>,
    /// `active_tab` field.
    pub active_tab: u32,
}
