// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_instance::InstanceManifest;

use jackin_core::JackinPaths;

pub fn set_role_terminal_title(paths: &JackinPaths, container_name: &str) {
    let title = if let Ok(manifest) = InstanceManifest::read(&paths.data_dir.join(container_name)) {
        manifest.role_display_name
    } else {
        let _warning = jackin_telemetry::record_recovered_degradation();
        container_name.to_owned()
    };
    jackin_diagnostics::set_terminal_title(&title);
}

pub fn host_alt_screen_exec_flag() -> Option<&'static str> {
    jackin_diagnostics::host_screen_owned().then_some("-e=JACKIN_HOST_ALT_SCREEN=1")
}

pub fn insert_run_as_user<'a>(args: &mut Vec<&'a str>, run_as_user: Option<&'a str>) {
    if let Some(user) = run_as_user {
        args.insert(1, user);
        args.insert(1, "--user");
    }
}

pub fn git_policy_env_pairs(
    coauthor_trailer: bool,
    dco: bool,
) -> Vec<(&'static str, &'static str)> {
    let mut pairs = Vec::with_capacity(2);
    if coauthor_trailer {
        pairs.push((jackin_core::JACKIN_GIT_COAUTHOR_TRAILER_ENV_NAME, "1"));
    }
    if dco {
        pairs.push((jackin_core::JACKIN_GIT_DCO_ENV_NAME, "1"));
    }
    pairs
}
