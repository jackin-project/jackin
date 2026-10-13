// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    AccountScanOutcome, OwnedSettingsSaveInput, WorkspaceSaveInput, WorkspaceSaveMode,
    run_account_scan, save_settings_first_run_aware, save_workspace, start_account_scan,
};

use jackin_config::{
    AccountConfig, AccountCredential, AiProvider, AppConfig, CURRENT_WORKSPACE_VERSION, EnvValue,
    GithubAuthConfig, MountConfig, MountIsolation, WorkspaceConfig, WorkspaceRoleOverride,
};

use jackin_console::tui::runtime::{BlockingSubscription, SubscriptionPoll};

use jackin_core::{Agent, JackinPaths};

use std::collections::BTreeMap;

mod support;
use support::*;
mod case_01;
