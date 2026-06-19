use crate::console::effects::{execute_manager_effect, poll_background_messages};
use crate::console::tui::app::{ConsoleStage, ConsoleState};
use crate::console::tui::message::{ManagerBackgroundEvent, ManagerMessage, update_manager};
use crate::console::tui::run::{no_modal_open, startup_error_was_dismissed};
use crate::console::tui::state::ManagerState;
use jackin_console::tui::effect::ConsoleEffect;

#[tokio::test]
async fn poll_background_messages_routes_file_browser_poll_through_message() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = crate::paths::JackinPaths::for_tests(tmp.path());
    let cwd = tmp.path();
    let mut config = jackin_config::AppConfig::default();
    let mut state = ManagerState::from_config(&config, cwd);

    let events = poll_background_messages(&mut state, &mut config, &paths);

    assert!(events.iter().any(|event| matches!(
        event,
        ManagerBackgroundEvent::Message(ManagerMessage::PollFileBrowserGitUrls)
    )));
}

#[tokio::test]
async fn execute_manager_effect_requests_instance_refresh() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = crate::paths::JackinPaths::for_tests(tmp.path());
    let cwd = tmp.path();
    let mut config = jackin_config::AppConfig::default();
    let mut state = ManagerState::from_config(&config, cwd);

    execute_manager_effect(
        &mut state,
        &mut config,
        &paths,
        ConsoleEffect::RequestInstanceRefresh.into(),
    );

    assert!(
        state.instance_refresh_in_flight(),
        "instance refresh effect should spawn a worker"
    );
}

#[test]
fn chip_click_does_not_fire_while_list_modal_open() {
    // Verifies single-consumer precedence: when a list_modal is active, the
    // no_modal_open guard returns false, preventing the debug chip handler
    // (and base-surface mouse routing) from mutating manager state.
    use std::cell::RefCell;
    use std::rc::Rc;

    let cwd = std::path::Path::new("/");
    let config = jackin_config::AppConfig::default();

    // Verify clean state has no modal.
    let op_cache = Rc::new(RefCell::new(jackin_env::OpCache::default()));
    let clean_manager = ManagerState::from_config(&config, cwd);
    let clean_state = ConsoleState::new(ConsoleStage::Manager(clean_manager), op_cache, false);
    assert!(
        no_modal_open(&clean_state),
        "no modal by default — chip is active"
    );

    // Open a list modal and verify the guard fires.
    let mut manager_with_modal = ManagerState::from_config(&config, cwd);
    let _unused = update_manager(
        &mut manager_with_modal,
        ManagerMessage::OpenListErrorPopup {
            title: "Error".into(),
            message: "something failed".into(),
        },
    );
    let op_cache2 = Rc::new(RefCell::new(jackin_env::OpCache::default()));
    let state_with_modal =
        ConsoleState::new(ConsoleStage::Manager(manager_with_modal), op_cache2, false);

    assert!(
        !no_modal_open(&state_with_modal),
        "list_modal open → chip and base surface must not fire"
    );
}

#[test]
fn chip_click_does_not_fire_while_quit_confirm_open() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let cwd = std::path::Path::new("/");
    let config = jackin_config::AppConfig::default();
    let manager = ManagerState::from_config(&config, cwd);
    let op_cache = Rc::new(RefCell::new(jackin_env::OpCache::default()));
    let mut state = ConsoleState::new(ConsoleStage::Manager(manager), op_cache, false);

    assert!(no_modal_open(&state), "no modal by default");
    state.open_quit_confirm();
    assert!(!no_modal_open(&state), "quit_confirm → chip must not fire");
}

#[test]
fn startup_error_exit_gate_fires_after_dialog_dismissal() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let cwd = std::path::Path::new("/");
    let config = jackin_config::AppConfig::default();
    let mut manager = ManagerState::from_config(&config, cwd);
    manager.open_list_error_popup("Docker daemon not reachable", "docker socket missing");
    let op_cache = Rc::new(RefCell::new(jackin_env::OpCache::default()));
    let mut state = ConsoleState::new(ConsoleStage::Manager(manager), op_cache, false);

    assert!(!startup_error_was_dismissed(&state, true));

    let ConsoleStage::Manager(manager) = &mut state.stage;
    manager.list_modal = None;

    assert!(startup_error_was_dismissed(&state, true));
    assert!(!startup_error_was_dismissed(&state, false));
}
