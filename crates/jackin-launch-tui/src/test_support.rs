use jackin_core::launch_progress::LaunchHostTerminal;

struct TestHostTerminal;

impl LaunchHostTerminal for TestHostTerminal {
    fn set_rich_surface_active(&self, _active: bool) {}

    fn host_screen_owned(&self) -> bool {
        false
    }

    fn is_debug_mode(&self) -> bool {
        false
    }

    fn emit_compact_line(&self, _kind: &str, _line: &str) {}

    fn emit_debug_line(&self, _category: &str, _line: &str) {}

    fn set_pointer_shape(&self, _pointer: bool) {}

    fn copy_to_clipboard(&self, _payload: &str) -> bool {
        true
    }

    fn reveal_file(&self, _path: &std::path::Path) -> bool {
        false
    }

    fn open_file(&self, _path: &std::path::Path) -> bool {
        false
    }
}

static TEST_HOST_TERMINAL: TestHostTerminal = TestHostTerminal;

pub(crate) fn test_host_terminal() -> &'static dyn LaunchHostTerminal {
    &TEST_HOST_TERMINAL
}
