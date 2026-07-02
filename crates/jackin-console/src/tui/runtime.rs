//! G0 shared-runtime wiring for the console TUI.
//!
//! The shared TEA `Component<Ev, Msg>` and `View<Model>` contracts live in
//! `jackin_tui::runtime`. This module is the console's implementation of
//! those traits over its model (`ConsoleState`) and the existing render
//! function (`crate::tui::view::render`). The trait impls are thin
//! delegations that satisfy the shared contract at the type level. The
//! existing event loop in `crates/jackin/src/console/tui/run.rs` continues
//! to call `view::render` directly; migrating it to trait dispatch is a
//! follow-up tracked as a later W6 phase.

#[derive(Debug)]
pub struct ConsoleViewContext<'a> {
    pub config: &'a jackin_config::AppConfig,
    pub cwd: &'a std::path::Path,
}

#[derive(Debug)]
pub struct ConsoleView<'a> {
    pub context: ConsoleViewContext<'a>,
}

impl jackin_tui::runtime::View<crate::tui::console::ConsoleState> for ConsoleView<'_> {
    fn render(
        &self,
        model: &crate::tui::console::ConsoleState,
        frame: &mut ratatui::Frame<'_>,
        area: ratatui::layout::Rect,
    ) {
        let crate::tui::console::ConsoleStage::Manager(ms) = &model.stage;
        crate::tui::view::render(frame, area, ms, self.context.config, self.context.cwd);
    }
}
