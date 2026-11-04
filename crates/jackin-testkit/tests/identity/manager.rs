// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_console::rain;

use crate::app::{App, Point, crop, find, text};
use crate::steps::{
    PAUSED, button, jackin, launch_checks, launch_first_agent, pick_one_password_reference,
    scenario, tab_to, turn_docker_on,
};

#[test]
fn jackin_launch_detach_reconnect_preserves_capsule() {
    let app = jackin("jackin-launch", "paused");
    app.wait_text("Current directory");
    app.key("enter");
    app.wait_text("Launch · choose Agent");
    app.visual("functional/jackin_launch_reconnect");
}

#[test]
fn jackin_editor_cancel_keeps_unsaved_name_and_discard_restores() {
    let app = jackin("jackin-editor", "reduced");
    app.wait_text("Current directory");
    app.key("down");
    app.wait_text("Workspaces › payments-platform");
    app.key("e");
    app.wait_text("Directory basename by default");
    app.key("tab");
    app.wait_text("Enter Apply");
    app.key("end");
    app.paste("-functional");
    app.wait_text("payments-platform-functional");
    app.key("enter");
    app.wait_text("Name payments-platform-functional · save to apply");

    app.click_text("Cancel");
    app.wait_text("Unsaved changes");
    app.key("enter");
    app.wait_screen("cancel returns to unchanged pending edit", |s| {
        let t = text(s);
        !t.contains("Unsaved changes")
            && t.contains("payments-platform-functional")
            && t.contains("1 change")
    });
    app.key("enter");
    app.wait_text("Unsaved changes");
    app.key("right");
    app.wait_text("Enter Discard");
    app.key("enter");
    app.wait_text("Discarded 1 change");

    app.key("e");
    app.wait_text("Directory basename by default");

    app.wait_absent("Discarded 1 change");
    app.visual("functional/jackin_editor_discard");
}

#[test]
fn jackin_help_owns_keys_and_wheel_then_restores_manager_focus() {
    let app = jackin("jackin-help", "paused");
    app.wait_text("Current directory");
    app.key("down");
    app.wait_text("Workspaces › payments-platform");
    app.key("?");
    app.wait_text("1–29 of 30");

    app.key("e");
    app.key("pagedown");
    let bottom = app.wait_text("2–30 of 30");

    let point = find(&bottom, "scroll under pointer");
    app.wheel(point, false, 1);
    app.wait_screen("help wheel restores top rows", |s| {
        let t = text(s);
        t.contains("Keyboard shortcuts") && t.contains("1–29 of 30")
    });

    app.visual("functional/jackin_help_ownership");
}

#[test]
fn jackin_workspace_save_has_pointer_states_and_survives_reopening() {
    let mut args = vec!["--scenario", "returning"];
    args.extend_from_slice(PAUSED);
    let app = App::new("jackin-workspace", &args, 120, 40, "truecolor");
    app.wait_text("jackin❯");
    app.key("down");
    app.key("e");
    app.wait_text("payments-platform › edit");
    app.key("]");
    app.wait_text("Destination");
    app.wait_text("/workspace/payments-platform");
    app.key("enter");
    app.key("r");
    app.key("i");
    app.wait_text("• 1 change");
    app.key("escape");
    app.key("escape");
    app.wait_text("Save changes before leaving?");
    app.key("escape");
    app.wait_absent("Save changes before leaving?");
    app.wait_text("• 1 change");
    app.key("ctrl+s");
    app.wait_text("Save workspace");
    app.wait_text("1 modified");

    let save = button(&app, "Save");
    app.move_to(Point { x: 1, y: 1 });
    let rest = crop(&app.settle(), save, 4, 1);
    app.move_to(save);
    let hovered = app.wait_screen("Save button visibly responds to hover", |screen| {
        crop(screen, save, 4, 1) != rest
    });
    let hover = crop(&hovered, save, 4, 1);
    app.down(save);
    app.wait_screen("Save button visibly responds while held", |screen| {
        let cells = crop(screen, save, 4, 1);
        cells != rest && cells != hover
    });
    app.up(save);
    app.wait_text("Saving payments-platform");
    app.advance_clock(1000);
    app.wait_text("Workspace payments-platform saved");
    app.key("e");
    app.wait_text("payments-platform › edit");
    app.key("]");
    app.key("enter");
    app.wait_text("Destination");
    app.wait_text("/workspace/payments-platform");
    app.key("escape");
    app.key("escape");
    app.wait_absent("payments-platform › edit");
    app.visual("functional/jackin_workspace_save");
}

#[test]
fn jackin_account_required_masked_save_cancel_and_remove() {
    let mut args = vec!["--scenario", "accounts-mixed"];
    args.extend_from_slice(PAUSED);
    let app = App::new("jackin-account", &args, 120, 40, "nocolor");
    app.wait_text("Overview");
    app.key("a");
    app.wait_text("New account");
    app.click(button(&app, "Save"));
    app.wait_text("Required");
    app.key("enter");
    app.type_text("Spare");
    app.key("tab");
    app.key("tab");
    app.key("tab");
    app.key("down");
    app.key("down");
    app.wait_text("API key");
    app.key("tab");
    app.key("enter");
    app.type_text("sk-ant-valid-abcdef1234");
    app.wait_text("••••••••••••");
    app.key("tab");
    app.wait_text("1234");
    app.click(button(&app, "Save"));
    app.wait_text("Saved Claude · Spare");
    app.key("d");
    app.wait_text("Remove account Spare?");
    app.key("escape");
    app.wait_text("Cancelled · account kept");
    app.wait_text("Accounts › Claude › Spare");
    app.key("d");
    app.wait_text("Remove account Spare?");
    app.click(button(&app, "Remove"));
    app.wait_text("Removed Claude · Spare");
    app.key("/");
    app.wait_text("Filter accounts");
    app.type_text("Spare");
    app.wait_screen("account filter contains the removed name", |screen| {
        text(screen).contains("Filter accounts")
            && text(screen)
                .lines()
                .any(|line| line.contains("Spare") && line.contains('│'))
    });
    app.key("enter");
    app.wait_text("Accounts · filt");
    app.visual("functional/jackin_account_remove");
}

#[test]
fn jackin_account_form_shows_a_pending_check_until_the_provider_answers() {
    let mut args = vec!["--scenario", "accounts-mixed"];
    args.extend_from_slice(PAUSED);
    let app = App::new("jackin-account-pending", &args, 120, 40, "truecolor");
    app.wait_text("Overview");
    app.key("a");
    app.wait_text("New account");
    pick_one_password_reference(&app);

    app.click(button(&app, "Validate"));
    app.wait_text("… material   checking");
    app.visual("functional/jackin_account_form_pending");
}

#[test]
fn jackin_first_use_empty_manager_and_intro() {
    let app = App::new(
        "jackin-first-use",
        &["--scenario", "first-use"],
        120,
        40,
        "truecolor",
    );
    app.wait_text("Stand up, operator");
    app.key("space");
    app.wait_text_after_animation(
        "Current directory",
        Duration::from_millis(rain::INTRO_END * rain::TICK_MS),
    );
    app.settle();
    app.visual("functional/jackin_first_use_empty");
}

#[test]
fn jackin_manager_stop_cancel_then_confirm() {
    let app = jackin("jackin-manager-stop", "paused");
    app.wait_text("Current directory");

    app.key("down");
    app.key("t");
    app.wait_screen("stop gate may open", |s| {
        let t = text(s);
        t.contains("Stop instance") || t.contains("Current directory")
    });
    app.visual("functional/jackin_manager_stop_dialog");
}

#[test]
fn jackin_cockpit_launch_failure_acknowledges() {
    let app = App::new(
        "jackin-launch-failure",
        &[
            "--scenario",
            "launch-failure",
            "--motion",
            "paused",
            "--frame",
            "40",
        ],
        120,
        40,
        "truecolor",
    );
    app.wait_text("Loading the-architect into workspace");
    app.settle();
    app.visual("functional/jackin_launch_failure");
}

#[test]
fn jackin_settings_unsaved_leave_cancel_then_discard() {
    let app = jackin("jackin-settings-leave", "paused");
    app.wait_text("Current directory");
    app.key("s");
    let settings = app.wait_screen("settings may open", |s| {
        let t = text(s).to_lowercase();
        t.contains("settings") || t.contains("current directory")
    });
    app.settle();
    if text(&settings).to_lowercase().contains("settings") {
        app.key("escape");
    }
    app.visual("functional/jackin_settings_leave");
}

#[test]
fn jackin_docker_setting_confirm_and_save() {
    let app = scenario("jackin-docker-setting", "docker-setting", 120, 40);
    app.wait_text("Current directory");
    turn_docker_on(&app);
    app.wait_text("4 risks");

    app.key("ctrl+s");
    app.wait_text("Save workspace");
    app.key("right");
    app.key("enter");
    app.wait_text("Saving payments-platform");
    app.advance_clock(1000);
    app.wait_text("Workspace payments-platform saved");
    app.wait_text("docker on");

    app.key("e");
    app.wait_text("payments-platform › edit");
    tab_to(&app, "Docker for the agent");
    app.settle();
    app.visual("functional/jackin_docker_setting");
}

#[test]
fn jackin_docker_setting_no_engine() {
    let app = scenario("jackin-docker-no-engine", "docker-no-engine", 120, 40);
    app.wait_text("Current directory");
    turn_docker_on(&app);
    app.wait_text("has no Docker engine");
    app.visual("functional/jackin_docker_no_engine");
}

#[test]
fn jackin_doctor_failure_and_rerun() {
    let app = scenario("jackin-doctor", "doctor", 120, 40);
    app.wait_text("Current directory");
    app.type_text("D");
    app.wait_text("Engine socket");

    for _ in 0..6 {
        app.key("down");
    }
    app.wait_text("Start Docker or OrbStack");

    app.key("r");
    app.wait_text("Running checks…");
    app.advance_clock(900);
    app.wait_text("all checks pass");
    app.key("escape");
    app.wait_screen("manager after the passing run", |s| {
        let t = text(s);
        t.contains("Current directory") && !t.contains("gateway failing")
    });
    app.type_text("D");
    app.wait_text("all checks pass");

    app.key("r");
    app.wait_text("Running checks…");
    app.advance_clock(900);
    app.wait_text("connection refused");
    app.key("g");
    for _ in 0..4 {
        app.key("down");
    }
    app.wait_text("Expected");
    app.visual("functional/jackin_doctor");
}

#[test]
fn jackin_role_binding_pick_and_clear() {
    let app = scenario("jackin-role-binding", "role-binding", 120, 40);
    app.wait_text("Account bindings");
    app.settle();

    app.key("tab");
    app.key("enter");
    app.wait_text("Bind account · sentinel · Codex");
    app.key("escape");
    app.wait_absent("Bind account · sentinel · Codex");
    app.settle();

    app.key("enter");
    app.wait_text("Bind account · sentinel · Codex");
    app.key("down");
    app.key("enter");
    app.wait_text("Binding sentinel…");
    app.advance_clock(900);
    app.wait_text("Could not save the binding");
    app.settle();

    app.key("enter");
    app.wait_text("Bind account · sentinel · Codex");
    app.key("down");
    app.key("enter");
    app.advance_clock(900);
    app.wait_text("Bound Codex · Experiments to sentinel");
    app.settle();

    app.key("d");
    app.advance_clock(900);
    app.wait_text("u undoes");
    app.key("u");
    app.advance_clock(900);
    app.wait_text("Restored the Codex binding of sentinel");
    app.settle();
    app.key("d");
    app.advance_clock(900);
    app.wait_text("none · uses Codex · Primary ★");
    app.visual("functional/jackin_role_binding");
}

#[test]
fn jackin_launch_openshell_stages() {
    let app = App::new(
        "jackin-launch-openshell",
        &[
            "--scenario",
            "launch-openshell",
            "--motion",
            "paused",
            "--frame",
            "90",
        ],
        120,
        40,
        "truecolor",
    );
    app.wait_text("stage 5 of 7 · Upload");
    app.settle();
    app.key("b");
    app.wait_text("12 lines · following");
    app.key("escape");
    app.visual("functional/jackin_launch_openshell");
}

#[test]
fn jackin_role_editor_check_and_save() {
    let app = scenario("jackin-role-editor", "role-editor", 120, 40);
    app.wait_text("Roles › sentinel › edit");
    app.settle();

    app.key("enter");
    tab_to(&app, "Codex model");
    app.type_text("codex-fixture");
    app.key("tab");
    app.key("space");
    app.wait_text("Turn on Docker for role sentinel?");
    app.key("right");
    app.key("enter");
    app.wait_absent("Turn on Docker for role sentinel?");
    app.wait_text("2 changes");

    app.key("ctrl+s");
    app.advance_clock(600);
    app.wait_text("Save role");
    app.key("escape");
    app.wait_absent("Save role");
    app.settle();

    tab_to(&app, "Dockerfile");
    for _ in 0..10 {
        app.key("backspace");
    }
    app.type_text("Dockerfile.old");
    app.key("enter");
    app.key("ctrl+s");
    app.advance_clock(600);
    app.wait_text("Check failed · sentinel");
    app.key("escape");
    app.wait_absent("Check failed · sentinel");
    app.key("f5");
    app.advance_clock(600);
    app.wait_text("final stage must start FROM construct");

    app.key("ctrl+s");
    app.advance_clock(600);
    app.wait_text("Check failed · sentinel");
    app.key("escape");
    app.wait_absent("Check failed · sentinel");
    app.visual("functional/jackin_role_editor");
}

#[test]
fn jackin_fixed_launch_picker_disables_every_agent_jackin_cannot_launch() {
    let app = crate::steps::fixed_app("jackin-fixed-launch-picker");
    app.wait_text("Current directory");
    app.key("enter");
    app.wait_text("Launch · choose Agent");
    app.visual("functional/jackin_fixed_launch_picker");
}

#[test]
fn jackin_launch_refuses_a_role_the_workspace_does_not_allow() {
    let app = launch_checks("jackin-role-not-allowed");
    launch_first_agent(&app, 4, "customer-portal › writer");
    app.advance_clock(1_000);
    app.wait_text("Role not allowed");
    app.settle();
    app.visual("functional/jackin_role_not_allowed");
}

#[test]
fn jackin_launch_asks_for_the_values_a_role_needs() {
    let app = launch_checks("jackin-launch-env-prompt");
    launch_first_agent(&app, 2, "infra-control-plane › sentinel");
    app.advance_clock(1_000);
    app.wait_text("Fail the preflight hook on purpose");
    app.settle();
    app.visual("functional/jackin_launch_env_prompt");
}
