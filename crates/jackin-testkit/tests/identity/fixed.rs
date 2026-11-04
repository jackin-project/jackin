// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::domain::fixed::Fixed;
use jackin_console::domain::vocabulary::Vocabulary;

use crate::app::{find, row_text, text};
use crate::steps::{
    agent_at, agent_row, attach_capsule, button, fixed_app, footer, inside, open_fixed_accounts,
    open_fixed_editor, open_fixed_settings, open_fixed_wizard, pick_one_password_reference,
    quiet_after, refused_with, row_has, row_with, status_expired,
};

#[test]
fn jackin_fixed_keep_awake_refuses_by_click_and_key() {
    let app = fixed_app("jackin-fixed-keep-awake");
    open_fixed_editor(&app, 1);
    app.visual("functional/jackin_fixed_editor_general");
}

#[test]
fn jackin_fixed_git_pull_refuses_by_key_and_click() {
    let reason = Fixed::openshell().git_pull.text;
    let app = fixed_app("jackin-fixed-git-pull");
    open_fixed_editor(&app, 1);

    for _ in 0..4 {
        app.key("tab");
    }
    app.wait_screen("focus reaches git pull", |s| {
        row_with(s, "Git pull before launch").contains('▎')
    });
    app.key("space");
    refused_with(&app, &reason);
    app.visual("functional/jackin_fixed_editor_general_refused");
}

#[test]
fn jackin_fixed_project_row_keeps_its_paint_and_refuses_every_edit() {
    let app = fixed_app("jackin-fixed-project");
    open_fixed_editor(&app, 1);
    app.key("]");
    app.key("enter");
    app.wait_screen("the Project list has focus", |s| {
        text(s).contains("Destination") && row_with(s, "~/src/payments-platform").contains('▎')
    });
    app.visual("functional/jackin_fixed_editor_project");
}

#[test]
fn jackin_fixed_workspace_account_toggle_refuses_and_prefer_stays_live() {
    let app = fixed_app("jackin-fixed-accounts");
    open_fixed_editor(&app, 1);
    for _ in 0..4 {
        app.key("]");
    }
    app.key("enter");
    app.wait_screen("the account list has focus", |s| {
        text(s).contains("Active accounts") && footer(s).contains("Prefer")
    });
    app.visual("functional/jackin_fixed_editor_accounts");
}

#[test]
fn jackin_fixed_settings_commits_refuse_by_key_and_click() {
    let app = fixed_app("jackin-fixed-commits");
    open_fixed_settings(&app, "General", 0);
    app.visual("functional/jackin_fixed_settings_commits");
}

#[test]
fn jackin_fixed_settings_global_mounts_refuse_every_add() {
    let app = fixed_app("jackin-fixed-global-mounts");
    open_fixed_settings(&app, "Mounts", 1);
    app.visual("functional/jackin_fixed_settings_mounts");
}

#[test]
fn jackin_fixed_settings_trust_refuses_the_toggle_and_shows_the_source() {
    let app = fixed_app("jackin-fixed-trust");
    open_fixed_settings(&app, "Trust", 4);
    app.visual("functional/jackin_fixed_settings_trust");
}

#[test]
fn jackin_fixed_settings_agents_refuse_every_row() {
    let fixed = Fixed::openshell();
    let app = fixed_app("jackin-fixed-agents");
    open_fixed_settings(&app, "Agents", 3);

    for key in ["space", "enter", "d"] {
        app.key(key);
        refused_with(&app, &fixed.agent_other.text);
        status_expired(&app, &fixed.agent_other.text);
    }
    app.click(inside(agent_at(&app.screen(), "Claude Code")));
    refused_with(&app, &fixed.agent_other.text);
    status_expired(&app, &fixed.agent_other.text);

    app.key("down");
    app.wait_screen("cursor on Codex", |s| agent_row(s, "Codex").contains('▎'));
    for key in ["enter", "d", "space"] {
        app.key(key);
        refused_with(&app, &fixed.agent_launchable.text);
        if key != "space" {
            status_expired(&app, &fixed.agent_launchable.text);
        }
    }
    app.visual("functional/jackin_fixed_settings_agents");
}

#[test]
fn jackin_fixed_settings_role_overrides_refuse_and_the_scope_lists_global() {
    let app = fixed_app("jackin-fixed-role-env");
    open_fixed_settings(&app, "Environments", 2);

    for row in ["CARGO_NET_GIT_FET", "+ Add environment variable"] {
        app.key("down");
        app.wait_screen("the cursor steps down", |s| {
            (0..s.rows())
                .map(|r| row_text(s, r))
                .any(|r| r.contains(row) && r.contains('▎'))
        });
    }
    app.key("down");
    app.wait_screen("cursor on the Add role row", |s| {
        row_with(s, "+ Add role override…").contains('▎')
    });
    app.visual("functional/jackin_fixed_settings_env");
}

#[test]
fn jackin_fixed_env_sources_refuse_the_radio_options_and_the_p_key() {
    let reason = Fixed::openshell().env_sources.text;
    let app = fixed_app("jackin-fixed-env-sources");
    open_fixed_settings(&app, "Environments", 2);
    app.key("p");
    refused_with(&app, &reason);
    status_expired(&app, &reason);

    app.key("a");
    app.wait_text("New global environment key");
    app.key("tab");
    app.wait_screen("focus reaches the source radio", |s| {
        row_with(s, "Plain text").contains('▎')
    });

    app.key("down");
    app.wait_screen("cursor on the 1Password option", |s| {
        row_with(s, "1Password item").contains('▎')
    });
    app.visual("functional/jackin_fixed_env_form");
}

#[test]
fn jackin_fixed_account_enable_refuses_by_key_button_and_menu() {
    let fixed = Fixed::openshell();
    let reason = fixed.enable_global.text;
    let short = fixed.enable_global.short.expect("a short reason");
    let app = fixed_app("jackin-fixed-account-enable");
    open_fixed_accounts(&app, 7, "Accounts › Codex › Experiments");

    app.key("shift+f10");
    app.wait_text("Show usage");
    for _ in 0..5 {
        app.key("down");
    }
    app.wait_screen("the highlight reaches Disable", |s| {
        text(s).contains(&format!(" {short} "))
    });
    app.visual("functional/jackin_fixed_account_menu");
    app.key("enter");
    app.settle();
    app.key("esc");
    app.wait_absent("Show usage");

    app.key("t");
    refused_with(&app, &reason);
    quiet_after(&app, &reason, "down");
    app.key("up");

    app.key("tab");
    app.key("tab");
    for _ in 0..4 {
        app.key("right");
    }
    app.wait_screen("focus reaches the Disable button", |s| {
        row_has(s, "Disable", "▎Disable")
    });
    app.key("enter");
    refused_with(&app, &reason);
    status_expired(&app, &reason);

    app.click(inside(find(&app.screen(), "Disable")));
    refused_with(&app, &reason);
    app.visual("functional/jackin_fixed_account_disable_refused");
}

#[test]
fn jackin_fixed_account_form_refuses_a_typed_key_and_its_button() {
    let reason = Fixed::openshell().typed_key.text;
    let app = fixed_app("jackin-fixed-account-form");
    open_fixed_accounts(&app, 0, "Accounts › Overview");
    app.key("a");
    app.wait_text("New account");
    for _ in 0..3 {
        app.key("tab");
    }
    app.wait_screen("focus reaches the source radio", |s| {
        row_has(s, "Local agent folder", "▎")
    });
    app.visual("functional/jackin_fixed_account_form");

    app.key("down");
    app.wait_screen("the cursor reaches the typed key option", |s| {
        row_has(s, "Plain-text API key", "▎")
    });
    for key in ["space", "enter"] {
        app.key(key);
        refused_with(&app, &reason);
        status_expired(&app, &reason);
    }
    app.key("space");
    refused_with(&app, &reason);
    app.visual("functional/jackin_fixed_account_form_refused");
    status_expired(&app, &reason);

    app.click(inside(find(&app.screen(), "Plain-text API key")));
    refused_with(&app, &reason);
    quiet_after(&app, &reason, "up");

    app.key("up");
    app.wait_screen("1Password is chosen", |s| {
        row_has(s, "1Password item / field", "(●)")
    });
    app.visual("functional/jackin_fixed_account_form_1password");
}

#[test]
fn jackin_fixed_account_form_offers_an_environment_variable() {
    let words = Vocabulary::openshell().source;
    let app = fixed_app("jackin-fixed-account-env");
    open_fixed_accounts(&app, 0, "Accounts › Overview");
    app.key("a");
    app.wait_text("New account");
    for _ in 0..3 {
        app.key("tab");
    }
    app.wait_screen("focus reaches the source radio", |s| {
        row_has(s, "Local agent folder", "▎")
    });
    for _ in 0..2 {
        app.key("down");
    }
    app.wait_screen("the environment variable is chosen", |s| {
        text(s).contains(&format!("(●) {}", words.env_label))
    });
    app.key("tab");
    app.key("enter");
    app.type_text("ANTHROPIC_API_KEY");
    app.wait_screen("the typed name replaces the placeholder", |s| {
        let shown = text(s);
        shown.contains("ANTHROPIC_API_KEY")
            && shown.contains(&words.env_help)
            && !shown.contains(&words.env_placeholder)
    });
    app.visual("functional/jackin_fixed_account_form_env");
}

#[test]
fn jackin_fixed_account_form_says_what_jackin_does_with_a_reference() {
    let app = fixed_app("jackin-fixed-account-pending");
    open_fixed_accounts(&app, 0, "Accounts › Overview");
    app.key("a");
    app.wait_text("New account");
    for _ in 0..3 {
        app.key("tab");
    }
    app.wait_screen("focus reaches the source radio", |s| {
        row_has(s, "Local agent folder", "▎")
    });
    app.key("up");
    app.wait_screen("1Password is chosen", |s| {
        row_has(s, "1Password item / field", "(●)")
    });
    pick_one_password_reference(&app);

    app.click(button(&app, "Validate"));
    app.wait_text("… material   checking");
    app.visual("functional/jackin_fixed_account_form_pending");
}

#[test]
fn jackin_fixed_account_edit_refuses_a_host_variable_account() {
    let reason = "reads ZAI_API_KEY · remove it, then add an account";
    let app = fixed_app("jackin-fixed-account-edit");
    open_fixed_accounts(&app, 13, "Accounts › Z.AI › host login");

    app.key("shift+f10");
    app.wait_text("Show usage");
    for _ in 0..3 {
        app.key("down");
    }
    app.wait_screen("the highlight reaches Edit…", |s| {
        text(s).contains(" env account ")
    });
    app.key("enter");
    app.settle();
    app.key("esc");
    app.wait_absent("Show usage");

    app.key("e");
    refused_with(&app, reason);
    app.visual("functional/jackin_fixed_account_edit_env");
}

#[test]
fn jackin_fixed_browser_git_url_keeps_its_help_and_refuses_a_url() {
    let fixed = Fixed::openshell();
    let words = Vocabulary::openshell();
    let app = fixed_app("jackin-fixed-browser-url");
    open_fixed_wizard(&app);
    app.key("g");
    app.wait_text(&words.git_url_help[0]);
    app.visual("functional/jackin_fixed_browser_url");

    app.paste("github.com/acme/payments");
    app.key("enter");
    app.wait_text(&format!("! {}", fixed.git_url));
    app.visual("functional/jackin_fixed_browser_url_refused");
}

#[test]
fn jackin_fixed_browser_read_only_refuses_by_key_and_click() {
    let reason = Fixed::openshell().wizard_ro.text;
    let app = fixed_app("jackin-fixed-browser-read-only");
    open_fixed_wizard(&app);

    app.key("tab");
    app.wait_screen("focus reaches the Read-only box", |s| {
        row_has(s, "Read-only", "▎")
    });
    app.key("space");
    refused_with(&app, &reason);
    app.visual("functional/jackin_fixed_browser_read_only");
}

#[test]
fn jackin_fixed_wizard_destination_cannot_change_and_next_advances() {
    let app = fixed_app("jackin-fixed-wizard-destination");
    open_fixed_wizard(&app);
    app.key("space");
    app.wait_text("step 2 of 5 · Destination");
    app.visual("functional/jackin_fixed_wizard_destination");

    app.key("down");
    let still = app.settle();
    app.click(inside(find(&still, "Edit destination…")));
    app.settle();

    app.key("enter");
    app.wait_text("step 4 of 5 · Working dir");
    app.visual("functional/jackin_fixed_wizard_workdir");
}

#[test]
fn jackin_fixed_palette_shows_reveal_and_open_as_unavailable() {
    let app = fixed_app("jackin-fixed-palette-export");
    attach_capsule(&app);
    app.key("ctrl+\\");
    app.wait_text("Command palette");
    app.visual("functional/jackin_fixed_palette_export");
}

#[test]
fn jackin_fixed_file_menu_shows_reveal_as_unavailable() {
    let short = Fixed::openshell()
        .reveal_open
        .short
        .expect("a short reason");
    let app = fixed_app("jackin-fixed-file-menu-export");
    attach_capsule(&app);
    app.key("f10");
    app.wait_text("Export selected file and reveal…");
    for _ in 0..4 {
        app.key("down");
    }
    app.wait_screen("the highlight reaches the reveal item", |s| {
        text(s).contains(&format!(" {short} "))
    });
    app.visual("functional/jackin_fixed_file_menu_export");
}
