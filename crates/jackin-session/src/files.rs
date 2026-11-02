// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{ChatgptAccountId, ModelName, SandboxPath, Timestamp};
use serde_json::json;
use toml::{Table, Value};

const ID_TOKEN: &str = "eyJhbGciOiJub25lIiwidHlwIjoiSldUIn0.eyJpc3MiOiJodHRwczovL2F1dGgub3BlbmFpLmNvbSIsImF1ZCI6ImNvZGV4Iiwic3ViIjoib3BlbnNoZWxsLWFwcC1zZXJ2ZXIifQ.placeholder";
const AUTH_MODE: &str = "chatgptAuthTokens";

pub(crate) fn render_codex_config(
    model: Option<&ModelName>,
    workdir: &SandboxPath,
) -> Result<String, toml::ser::Error> {
    let mut config = Table::new();
    if let Some(model) = model {
        config.insert("model".to_owned(), Value::String(model.as_str().to_owned()));
    }
    let mut trusted = Table::new();
    trusted.insert(
        "trust_level".to_owned(),
        Value::String("trusted".to_owned()),
    );
    let mut projects = Table::new();
    projects.insert(workdir.as_str().to_owned(), Value::Table(trusted));
    config.insert("projects".to_owned(), Value::Table(projects));
    toml::to_string(&config)
}

pub(crate) fn render_auth_json(
    access_token: &str,
    account_id: &ChatgptAccountId,
    now: Timestamp,
) -> String {
    json!({
        "auth_mode": AUTH_MODE,
        "OPENAI_API_KEY": null,
        "tokens": {
            "id_token": ID_TOKEN,
            "access_token": access_token,
            "refresh_token": "",
            "account_id": account_id.as_str(),
        },
        "last_refresh": now.rfc3339(),
    })
    .to_string()
}
