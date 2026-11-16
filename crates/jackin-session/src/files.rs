// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    CODEX_PLACEHOLDER_ID_TOKEN, ChatgptAccountId, ModelName, ProviderName, SandboxPath, Timestamp,
};
use serde_json::json;
use toml::{Table, Value};

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
            "id_token": CODEX_PLACEHOLDER_ID_TOKEN,
            "access_token": access_token,
            "refresh_token": "",
            "account_id": account_id.as_str(),
        },
        "last_refresh": now.rfc3339(),
    })
    .to_string()
}

pub(crate) fn render_placeholder_auth(spec: &jackin_core::AgentSpec) -> String {
    if spec.agent == jackin_core::Agent::Codex {
        return String::new();
    }
    let names: Vec<String> = spec
        .credential_env_names
        .iter()
        .map(|name| format!("\"{name}\": \"placeholder\""))
        .collect();
    format!("{{{}}}", names.join(", "))
}

pub fn render_omp_config(
    tuple: &'static jackin_core::AgentTuple,
    model: Option<&str>,
) -> Result<String, jackin_core::CatalogError> {
    let model = match model {
        Some(model) => {
            tuple.check_model(model)?;
            model
        }
        None => tuple.default_model,
    };
    let provider = tuple.omp_provider_id().unwrap_or_default();
    let env = tuple.omp_credential_env().unwrap_or_default();
    let base_url = match tuple.omp_base_url() {
        Some(url) => format!("    baseUrl: {url}\n"),
        None => String::new(),
    };
    Ok(format!(
        "providers:\n  {provider}:\n{base_url}    apiKey: placeholder\ncredentialEnv: {env}\nmodelRoles:\n  default: {provider}/{model}\n"
    ))
}

pub fn grok_auth_file(issuer: &str) -> crate::plan::PlannedFile {
    let spec = jackin_core::Agent::GrokBuild.spec();
    crate::plan::PlannedFile {
        path: spec.auth_file.path.clone(),
        mode: spec.auth_file.mode,
        text: json!({ issuer: { "key": "placeholder" } }).to_string(),
    }
}

pub fn omp_auth_file(tuple: &jackin_core::AgentTuple) -> crate::plan::PlannedFile {
    let spec = jackin_core::Agent::Omp.spec();
    let provider = tuple.omp_provider_id().unwrap_or_default();
    crate::plan::PlannedFile {
        path: spec.auth_file.path.clone(),
        mode: spec.auth_file.mode,
        text: format!("{{\"{provider}\": {{\"type\": \"api_key\", \"key\": \"placeholder\"}}}}"),
    }
}

pub(crate) fn placeholder_file(spec: &jackin_core::AgentSpec) -> crate::plan::PlannedFile {
    crate::plan::PlannedFile {
        path: spec.auth_file.path.clone(),
        mode: spec.auth_file.mode,
        text: render_placeholder_auth(spec),
    }
}

fn render_native_config(
    agent: jackin_core::Agent,
    provider: &ProviderName,
) -> crate::plan::PlannedFile {
    let spec = agent.spec();
    let env = spec
        .credential_env_names
        .first()
        .copied()
        .unwrap_or_default();
    crate::plan::PlannedFile {
        path: spec.user_config.path.clone(),
        mode: spec.user_config.mode,
        text: format!(
            "[providers.{provider}]\napi_key_env = \"{env}\"\napi_key = \"placeholder\"\n"
        ),
    }
}

pub fn render_kimi_config(provider: &ProviderName) -> crate::plan::PlannedFile {
    render_native_config(jackin_core::Agent::KimiCode, provider)
}

pub fn render_mimo_config(provider: &ProviderName) -> crate::plan::PlannedFile {
    render_native_config(jackin_core::Agent::MimoCode, provider)
}
