//! Multiplexer input mode.
//!
//! The daemon still stores the underlying interaction state close to the code
//! that mutates it, but input dispatch should ask one question: which mode owns
//! this operator event?

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MuxMode {
    Normal,
    PrefixAwait,
    Dialog,
    Drag,
    Select,
}

impl MuxMode {
    pub const fn forwards_to_pane(self) -> bool {
        matches!(self, Self::Normal | Self::PrefixAwait)
    }

    pub const fn blocks_focus_report(self) -> bool {
        !matches!(self, Self::Normal | Self::PrefixAwait)
    }
}

#[cfg(test)]
mod tests {
    use super::MuxMode;

    #[test]
    fn pane_forwarding_modes_are_explicit() {
        assert!(MuxMode::Normal.forwards_to_pane());
        assert!(MuxMode::PrefixAwait.forwards_to_pane());
        assert!(!MuxMode::Dialog.forwards_to_pane());
        assert!(!MuxMode::Drag.forwards_to_pane());
        assert!(!MuxMode::Select.forwards_to_pane());
    }

    #[test]
    fn focus_reports_only_pass_through_pane_owned_modes() {
        assert!(!MuxMode::Normal.blocks_focus_report());
        assert!(!MuxMode::PrefixAwait.blocks_focus_report());
        assert!(MuxMode::Dialog.blocks_focus_report());
        assert!(MuxMode::Drag.blocks_focus_report());
        assert!(MuxMode::Select.blocks_focus_report());
    }
}
