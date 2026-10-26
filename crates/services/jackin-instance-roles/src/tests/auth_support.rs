// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

pub(super) fn stage_host_kimi_dir(
    temp: &tempfile::TempDir,
    config_content: Option<&str>,
    cred_files: &[(&str, &str)],
    mcp_json: Option<&str>,
    device_id: Option<&str>,
) -> PathBuf {
    let host_home = temp.path().join("host_home");
    let kimi_dir = host_home.join(".kimi-code");
    std::fs::create_dir_all(&kimi_dir).unwrap();
    if let Some(content) = config_content {
        std::fs::write(kimi_dir.join("config.toml"), content).unwrap();
    }
    if !cred_files.is_empty() {
        let creds_dir = kimi_dir.join("credentials");
        std::fs::create_dir_all(&creds_dir).unwrap();
        for (name, content) in cred_files {
            std::fs::write(creds_dir.join(name), content).unwrap();
        }
    }
    if let Some(content) = mcp_json {
        std::fs::write(kimi_dir.join("mcp.json"), content).unwrap();
    }
    if let Some(content) = device_id {
        std::fs::write(kimi_dir.join("device_id"), content).unwrap();
    }
    host_home
}
