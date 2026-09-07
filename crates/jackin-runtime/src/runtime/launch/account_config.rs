// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Materialize selected API account settings in the private capsule home.

use std::path::Path;

use anyhow::Context as _;
use jackin_config::{AccountCredential, AiProvider, AppConfig};
use jackin_core::{Agent, WorkspaceName};

pub(super) fn configure_accounts(
    root: &Path,
    config: &AppConfig,
    workspace: Option<&WorkspaceName>,
    role: &str,
    agents: &[Agent],
) -> anyhow::Result<()> {
    if agents.contains(&Agent::Opencode) {
        configure_opencode(root, config, workspace, role)?;
    }
    if !agents.contains(&Agent::Codex) {
        return Ok(());
    }
    let Some(account) = jackin_config::resolve_account(config, Agent::Codex, workspace, role)?
    else {
        return Ok(());
    };
    let AccountCredential::ApiKey {
        base_url, model, ..
    } = &account.credential
    else {
        return Ok(());
    };
    let cross_provider = account.provider != AiProvider::OpenAi;
    anyhow::ensure!(
        !cross_provider || model.is_some(),
        "a model is required for a Codex provider account"
    );
    let (default_url, key) = match account.provider {
        AiProvider::Moonshot => ("https://api.kimi.com/coding/v1", "KIMI_API_KEY"),
        AiProvider::Zai => ("https://api.z.ai/api/v1", "OPENAI_API_KEY"),
        AiProvider::Minimax => ("https://api.minimax.io/v1", "MINIMAX_API_KEY"),
        AiProvider::OpenAi => ("https://api.openai.com/v1", "OPENAI_API_KEY"),
        _ => anyhow::bail!("selected provider cannot authenticate Codex"),
    };
    let directory = root.join("home/.codex");
    std::fs::create_dir_all(&directory).context("create private Codex configuration directory")?;
    let path = directory.join("config.toml");
    let mut document: toml::Table = match std::fs::read_to_string(&path) {
        Ok(contents) => toml::from_str(&contents).context("parse private Codex configuration")?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => toml::Table::new(),
        Err(error) => return Err(error).context("read private Codex configuration"),
    };
    let mut provider = toml::Table::new();
    provider.insert("name".into(), account.provider.slug().into());
    provider.insert(
        "base_url".into(),
        base_url.as_deref().unwrap_or(default_url).into(),
    );
    provider.insert("env_key".into(), key.into());
    provider.insert("wire_api".into(), "responses".into());
    provider.insert("requires_openai_auth".into(), false.into());
    let providers = document
        .entry("model_providers")
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    let providers = providers
        .as_table_mut()
        .context("Codex model_providers must be a table")?;
    providers.insert("jackin_account".into(), provider.into());
    document.insert("model_provider".into(), "jackin_account".into());
    if cross_provider {
        document.remove("model_catalog_json");
    }
    if let Some(model) = model {
        document.insert("model".into(), model.clone().into());
        if let Some(catalog) = model_catalog(account.provider, model) {
            std::fs::write(
                directory.join("account-models.json"),
                serde_json::to_vec_pretty(&catalog)?,
            )
            .context("write private Codex model metadata")?;
            document.insert(
                "model_catalog_json".into(),
                "~/.codex/account-models.json".into(),
            );
            document.insert("model_reasoning_effort".into(), "high".into());
        }
    }
    std::fs::write(path, toml::to_string_pretty(&document)?)
        .context("write private Codex account configuration")
}

/// Provider identifiers from `OpenCode`'s catalog; config and CLI model use the same ID.
fn opencode_provider(
    provider: AiProvider,
) -> anyhow::Result<(&'static str, &'static str, &'static str)> {
    Ok(match provider {
        AiProvider::Anthropic => (
            "anthropic",
            "@ai-sdk/anthropic",
            "https://api.anthropic.com/v1",
        ),
        AiProvider::OpenAi => ("openai", "@ai-sdk/openai", "https://api.openai.com/v1"),
        AiProvider::Xai => ("xai", "@ai-sdk/xai", "https://api.x.ai/v1"),
        AiProvider::Moonshot => (
            "kimi-for-coding",
            "@ai-sdk/anthropic",
            "https://api.kimi.com/coding/v1",
        ),
        AiProvider::Zai => (
            "zai-coding-plan",
            "@ai-sdk/openai-compatible",
            "https://api.z.ai/api/coding/paas/v4",
        ),
        AiProvider::Minimax => (
            "minimax",
            "@ai-sdk/anthropic",
            "https://api.minimax.io/anthropic/v1",
        ),
        AiProvider::Opencode => (
            "opencode",
            "@ai-sdk/openai-compatible",
            "https://opencode.ai/zen/v1",
        ),
        AiProvider::Amp => anyhow::bail!("Amp accounts cannot authenticate OpenCode"),
    })
}

pub(super) fn opencode_model(provider: AiProvider, model: &str) -> anyhow::Result<String> {
    let (id, _, _) = opencode_provider(provider)?;
    if model.starts_with(&format!("{id}/")) {
        Ok(model.to_owned())
    } else {
        Ok(format!("{id}/{model}"))
    }
}

/// <https://opencode.ai/docs/providers>: provider options and model IDs are paired.
fn configure_opencode(
    root: &Path,
    config: &AppConfig,
    workspace: Option<&WorkspaceName>,
    role: &str,
) -> anyhow::Result<()> {
    let Some(account) = jackin_config::resolve_account(config, Agent::Opencode, workspace, role)?
    else {
        return Ok(());
    };
    let AccountCredential::ApiKey {
        base_url, model, ..
    } = &account.credential
    else {
        return Ok(());
    };
    let (id, npm, default_url) = opencode_provider(account.provider)?;
    let credentials = account.credential_env(Agent::Opencode)?;
    let key = credentials
        .keys()
        .next()
        .context("OpenCode account has no credential variable")?;
    let directory = root.join("home/.config/opencode");
    std::fs::create_dir_all(&directory)
        .context("create private OpenCode configuration directory")?;
    let mut provider = serde_json::json!({
        "name": account.name, "npm": npm,
        "options": { "baseURL": base_url.as_deref().unwrap_or(default_url), "apiKey": format!("{{env:{key}}}") }
    });
    // Zen chooses a protocol per model; preserve its built-in catalog routing.
    if account.provider == AiProvider::Opencode && base_url.is_none() {
        provider
            .as_object_mut()
            .context("OpenCode provider must be an object")?
            .remove("npm");
        provider["options"]
            .as_object_mut()
            .context("OpenCode options must be an object")?
            .remove("baseURL");
    }
    let mut document = serde_json::json!({
        "$schema": "https://opencode.ai/config.json", "permission": "allow",
        "enabled_providers": [id]
    });
    if let Some(model) = model {
        let full_model = opencode_model(account.provider, model)?;
        let model_id = full_model
            .strip_prefix(&format!("{id}/"))
            .context("OpenCode model provider mismatch")?;
        provider["models"] = serde_json::json!({ model_id: { "name": model_id } });
        document["model"] = full_model.into();
    }
    document["provider"] = serde_json::json!({ id: provider });
    std::fs::write(
        directory.join("opencode.json"),
        serde_json::to_vec_pretty(&document)?,
    )
    .context("write private OpenCode account configuration")
}

/// Provider-published metadata, not guessed for custom model IDs.
/// <https://www.kimi.com/code/docs/en/third-party-tools/codex.html>
/// <https://docs.z.ai/devpack/tool/codex>
/// <https://platform.minimax.io/docs/token-plan/codex>
fn model_catalog(provider: AiProvider, model: &str) -> Option<serde_json::Value> {
    let (context, modalities) = match (provider, model) {
        (AiProvider::Moonshot, "k3") => (1_048_576, vec!["text", "image"]),
        (AiProvider::Moonshot, "k3-256k") => (262_144, vec!["text", "image"]),
        (AiProvider::Zai, "glm-5.3") => (1_048_576, vec!["text"]),
        (AiProvider::Minimax, "MiniMax-M3") => (1_000_000, vec!["text", "image"]),
        _ => return None,
    };
    let mut entry = serde_json::json!({
        "slug": model, "display_name": model, "description": model,
        "default_reasoning_level": "high",
        "supported_reasoning_levels": [
            { "effort": "low", "description": "Light reasoning" },
            { "effort": "high", "description": "Enhanced reasoning" },
            { "effort": "max", "description": "Deep reasoning" }
        ],
        "shell_type": "shell_command", "visibility": "list", "supported_in_api": true,
        "priority": 0, "base_instructions": "", "supports_reasoning_summaries": true,
        "default_reasoning_summary": "none", "support_verbosity": false,
        "truncation_policy": { "mode": "bytes", "limit": 10000 },
        "context_window": context, "max_context_window": context,
        "effective_context_window_percent": 95, "supports_parallel_tool_calls": true,
        "experimental_supported_tools": [], "input_modalities": modalities
    });
    if provider == AiProvider::Minimax {
        entry["supported_reasoning_levels"] = serde_json::json!([
            { "effort": "none", "description": "Thinking disabled" },
            { "effort": "high", "description": "Adaptive thinking" }
        ]);
    }
    if provider == AiProvider::Zai {
        entry["apply_patch_tool_type"] = "freeform".into();
    }
    Some(serde_json::json!({ "models": [entry] }))
}

#[cfg(test)]
mod tests;
