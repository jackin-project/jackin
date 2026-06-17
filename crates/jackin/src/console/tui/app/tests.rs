use super::*;
use crate::console::tui::state::Modal;

#[test]
fn startup_error_opens_list_error_dialog() {
    let config = AppConfig::default();
    let cwd = std::path::Path::new("/");
    let state = new_console_state_with_startup_error(
        &config,
        cwd,
        false,
        Some((
            "Docker daemon not reachable".into(),
            "failed to connect to Docker daemon".into(),
        )),
    )
    .expect("console state");

    let manager = match state.stage {
        ConsoleStage::Manager(manager) => manager,
    };
    let Some(Modal::ErrorPopup { state: popup }) = manager.list_modal else {
        panic!("startup Docker failure should open ErrorDialog");
    };
    assert_eq!(popup.title, "Docker daemon not reachable");
    assert_eq!(popup.message, "failed to connect to Docker daemon");
}
