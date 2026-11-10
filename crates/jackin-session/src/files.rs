// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::credentials::KimiRegion;
use jackin_core::{
    CODEX_PLACEHOLDER_ID_TOKEN, ChatgptAccountId, FileMode, ModelName, SandboxPath, Timestamp,
};
use serde_json::json;
use toml::{Table, Value};

const AUTH_MODE: &str = "chatgptAuthTokens";

pub fn render_codex_config(
    tuple: &'static jackin_core::AgentTuple,
    chatgpt_model: Option<&ModelName>,
    model: Option<&str>,
    workdir: &SandboxPath,
) -> Result<String, crate::plan::PlanError> {
    let mut config = Table::new();
    if let Some(id) = tuple.codex_provider_id() {
        let model = match model {
            Some(model) => {
                tuple.check_model(model).map_err(|_| {
                    crate::plan::PlanError::TupleNotSupported {
                        agent: tuple.agent,
                        provider: tuple.provider,
                        reason: format!("unknown model {model}"),
                    }
                })?;
                model
            }
            None => tuple.default_model,
        };
        config.insert("model".to_owned(), Value::String(model.to_owned()));
        config.insert("model_provider".to_owned(), Value::String(id.to_owned()));
        if tuple.id == "T20" {
            config.insert(
                "web_search".to_owned(),
                Value::String("disabled".to_owned()),
            );
        }
        let mut provider = Table::new();
        provider.insert(
            "name".to_owned(),
            Value::String(tuple.codex_provider_name().unwrap_or_default().to_owned()),
        );
        provider.insert(
            "base_url".to_owned(),
            Value::String(tuple.codex_base_url().unwrap_or_default().to_owned()),
        );
        provider.insert(
            "env_key".to_owned(),
            Value::String(tuple.codex_credential_env().unwrap_or_default().to_owned()),
        );
        provider.insert("wire_api".to_owned(), Value::String("responses".to_owned()));
        let mut providers = Table::new();
        providers.insert(id.to_owned(), Value::Table(provider));
        config.insert("model_providers".to_owned(), Value::Table(providers));
    } else if let Some(model) = chatgpt_model {
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
    Ok(toml::to_string(&config)?)
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

pub fn render_claude_config(
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
    match tuple.claude_base_url() {
        Some(base_url) => Ok(json!({
            "model": model,
            "env": {
                "ANTHROPIC_BASE_URL": base_url,
            }
        })
        .to_string()),
        None => Ok(json!({ "model": model }).to_string()),
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

pub fn render_omp_models(
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
    let mut text = format!("providers:\n  {provider}:\n");
    if let Some(url) = tuple.omp_base_url() {
        text.push_str("    baseUrl: ");
        text.push_str(url);
        text.push('\n');
    }
    if let Some(api) = tuple.omp_model_api() {
        text.push_str("    api: ");
        text.push_str(api);
        text.push('\n');
    }
    text.push_str("    apiKey: ");
    text.push_str(env);
    text.push('\n');
    if tuple.omp_auth_header() {
        text.push_str("    authHeader: true\n");
    }
    if tuple.omp_custom_model() {
        text.push_str("    models:\n      - id: ");
        text.push_str(model);
        text.push('\n');
    }
    Ok(text)
}

pub fn omp_models_file(
    tuple: &'static jackin_core::AgentTuple,
    model: Option<&str>,
) -> Result<crate::plan::PlannedFile, crate::plan::PlanError> {
    let spec = jackin_core::Agent::Omp.spec();
    let root = spec
        .auth_file
        .path
        .as_str()
        .rsplit_once('/')
        .map(|(dir, _)| dir)
        .unwrap_or_default();
    let path = SandboxPath::parse(&format!("{root}/agent/models.yml"))
        .map_err(crate::plan::PlanError::Project)?;
    Ok(crate::plan::PlannedFile {
        path,
        mode: FileMode(0o600),
        text: render_omp_models(tuple, model).map_err(|_| {
            crate::plan::PlanError::TupleNotSupported {
                agent: tuple.agent,
                provider: tuple.provider,
                reason: "unknown model".to_owned(),
            }
        })?,
    })
}

pub fn claude_config_file(
    tuple: &'static jackin_core::AgentTuple,
    model: Option<&str>,
) -> Result<crate::plan::PlannedFile, jackin_core::CatalogError> {
    let spec = jackin_core::Agent::ClaudeCode.spec();
    Ok(crate::plan::PlannedFile {
        path: spec.user_config.path.clone(),
        mode: spec.user_config.mode,
        text: render_claude_config(tuple, model)?,
    })
}

pub(crate) fn placeholder_file(tuple: &jackin_core::AgentTuple) -> crate::plan::PlannedFile {
    let spec = tuple.agent.spec();
    let text = match tuple.id {
        "T02" => json!({"claudeAiOauth": {
            "accessToken": "openshell:resolve:env:CLAUDE_CODE_OAUTH_TOKEN",
            "refreshToken": "",
            "expiresAt": 4_102_444_800_000_u64,
            "scopes": ["user:profile", "user:inference"],
        }})
        .to_string(),
        "T03" => String::new(),
        _ if tuple.agent == jackin_core::Agent::ClaudeCode && tuple.claude_base_url().is_some() => {
            "{}".to_owned()
        }
        _ => render_placeholder_auth(spec),
    };
    crate::plan::PlannedFile {
        path: spec.auth_file.path.clone(),
        mode: spec.auth_file.mode,
        text,
    }
}

pub fn render_kimi_config(region: KimiRegion) -> crate::plan::PlannedFile {
    let spec = jackin_core::Agent::KimiCode.spec();
    let model = spec.default_model;
    let base_url = region.base_url();
    let slot = region.credential_slot();
    let oauth_host = region
        .oauth_host()
        .map(|host| format!("oauth_host = \"{host}\"\n"))
        .unwrap_or_default();
    crate::plan::PlannedFile {
        path: spec.user_config.path.clone(),
        mode: spec.user_config.mode,
        text: format!(
            "default_model = \"kimi-code/{model}\"\n\n[providers.\"managed:kimi-code\"]\ntype = \"kimi\"\nbase_url = \"{base_url}\"\napi_key = \"\"\n\n[providers.\"managed:kimi-code\".oauth]\nstorage = \"file\"\nkey = \"oauth/{slot}\"\n{oauth_host}\n[models.\"{model}\"]\nprovider = \"managed:kimi-code\"\nmodel = \"{model}\"\nmax_context_size = 262144\ncapabilities = [\"thinking\", \"always_thinking\", \"image_in\", \"video_in\", \"tool_use\"]\n"
        ),
    }
}

pub fn render_kimi_auth(
    region: KimiRegion,
    scope: &str,
    access_token: &str,
) -> crate::plan::PlannedFile {
    let spec = jackin_core::Agent::KimiCode.spec();
    let path = match region {
        KimiRegion::MainlandCn => spec.auth_file.path.clone(),
        KimiRegion::Global => SandboxPath::from_static(
            "/sandbox/.kimi-code/credentials/kimi-code-env-0e4f99c69cc27850.json",
        ),
    };
    crate::plan::PlannedFile {
        path,
        mode: spec.auth_file.mode,
        text: json!({
            "access_token": access_token,
            "refresh_token": "",
            "expires_at": 0,
            "expires_in": 0,
            "scope": scope,
            "token_type": "Bearer"
        })
        .to_string(),
    }
}

pub fn render_muse_config(model: Option<&str>) -> crate::plan::PlannedFile {
    let spec = jackin_core::Agent::MuseCode.spec();
    crate::plan::PlannedFile {
        path: spec.user_config.path.clone(),
        mode: spec.user_config.mode,
        text: json!({
            "schema_version": 1,
            "provider": "meta",
            "model": model.unwrap_or(spec.default_model),
        })
        .to_string(),
    }
}

pub fn render_muse_trust(workdir: &SandboxPath) -> crate::plan::PlannedFile {
    let config = &jackin_core::Agent::MuseCode.spec().user_config.path;
    let parent = config
        .as_str()
        .rsplit_once('/')
        .map(|(dir, _)| dir)
        .unwrap_or_default();
    let path = SandboxPath::parse(parent)
        .and_then(|dir| dir.join("trust.json"))
        .unwrap_or_else(|_| config.clone());
    crate::plan::PlannedFile {
        path,
        mode: FileMode(0o600),
        text: json!({
            "schema_version": 1,
            "projects": {
                workdir.as_str(): { "decision": "trusted" }
            }
        })
        .to_string(),
    }
}

pub fn render_grok_config(model: Option<&str>) -> crate::plan::PlannedFile {
    const HELPER: &str = "jq -n -e --arg access_token \"${GROK_SESSION_TOKEN-}\" '$access_token | select(test(\"^openshell:resolve:[^\\\\r\\\\n]+$\")) | {access_token: ., issuer: \"https://auth.x.ai\"}'";
    let spec = jackin_core::Agent::GrokBuild.spec();
    let models = model.map_or_else(String::new, |model| {
        format!(
            "\n[models]\ndefault = {}\n",
            Value::String(model.to_owned())
        )
    });
    let helper = Value::String(HELPER.to_owned());
    crate::plan::PlannedFile {
        path: spec.user_config.path.clone(),
        mode: spec.user_config.mode,
        text: format!(
            "[cli]\nauto_update = false\n{models}\n[auth]\nauth_provider_command = {helper}\ndisable_api_key_auth = true\n"
        ),
    }
}
