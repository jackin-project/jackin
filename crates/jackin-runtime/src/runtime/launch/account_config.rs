// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Materialize selected API account settings in the private capsule home.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Context as _;
use jackin_config::{AccountCredential, AiProvider, AppConfig};
use jackin_core::Agent;

#[cfg(unix)]
mod private_config_fs {
    use std::fs::File;
    use std::io::{Read as _, Write as _};
    use std::os::unix::fs::MetadataExt as _;
    use std::path::{Component, Path};
    use std::sync::atomic::{AtomicU64, Ordering};

    use anyhow::Context as _;
    use fs4::FileExt as _;
    use nix::errno::Errno;
    use nix::fcntl::{openat, renameat, AtFlags, OFlag};
    use nix::sys::stat::{fstat, fstatat, mkdirat, Mode, SFlag};
    use nix::unistd::{linkat, unlinkat, UnlinkatFlags};

    static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
    const LOCK_FILE: &str = ".jackin-private-provider-config.lock";

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(super) enum Artifact {
        CodexCatalog,
        CodexConfig,
        OpenCodeConfig,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(super) enum PublishPoint {
        TempCreated(Artifact),
        TempWritten(Artifact),
        TempSynced(Artifact),
        BeforeInstall(Artifact),
        Installed(Artifact),
        DirectorySynced(Artifact),
    }

    pub(super) fn open_directory(root: &Path, home_relative: &Path) -> anyhow::Result<File> {
        let root_path = std::fs::canonicalize(root).context("resolve private account config root")?;
        let expected = std::fs::metadata(&root_path).context("stat private account config root")?;
        anyhow::ensure!(expected.is_dir(), "private account config root is not a directory");

        let mut directory = File::open("/").context("open filesystem root")?;
        for component in root_path.components() {
            match component {
                Component::RootDir => {}
                Component::Normal(name) => {
                    directory = open_child_directory(&directory, name, false)?;
                }
                Component::CurDir => {}
                Component::ParentDir | Component::Prefix(_) => {
                    anyhow::bail!("resolved private account config root is not canonical")
                }
            }
        }
        let actual = directory.metadata().context("stat opened account config root")?;
        anyhow::ensure!(
            expected.dev() == actual.dev() && expected.ino() == actual.ino(),
            "private account config root changed while opening"
        );

        let mut directory = open_child_directory(&directory, "home", true)?;
        let mut found_component = false;
        for component in home_relative.components() {
            let Component::Normal(name) = component else {
                anyhow::bail!("private account config directory must be a relative normal path")
            };
            found_component = true;
            directory = open_child_directory(&directory, name, true)?;
        }
        anyhow::ensure!(found_component, "private account config directory is empty");
        Ok(directory)
    }

    fn open_child_directory(parent: &File, name: &std::ffi::OsStr, create: bool) -> anyhow::Result<File> {
        let flags = OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC | OFlag::O_NOFOLLOW;
        match openat(parent, name, flags, Mode::empty()) {
            Ok(fd) => Ok(File::from(fd)),
            Err(Errno::ENOENT) if create => {
                match mkdirat(parent, name, Mode::S_IRWXU) {
                    Ok(()) => parent.sync_all().context("sync private account config parent")?,
                    Err(Errno::EEXIST) => {}
                    Err(error) => return Err(error).context("create private account config directory"),
                }
                openat(parent, name, flags, Mode::empty())
                    .map(File::from)
                    .context("open private account config directory")
            }
            Err(error) => Err(error).context("open private account config directory"),
        }
    }

    pub(super) fn lock(directory: &File) -> anyhow::Result<File> {
        let fd = openat(
            directory,
            LOCK_FILE,
            OFlag::O_RDWR
                | OFlag::O_CREAT
                | OFlag::O_CLOEXEC
                | OFlag::O_NOFOLLOW
                | OFlag::O_NONBLOCK,
            Mode::S_IRUSR | Mode::S_IWUSR,
        )
        .context("open private provider config lock")?;
        let lock = File::from(fd);
        ensure_regular(&lock, LOCK_FILE)?;
        lock.sync_all().context("sync private provider config lock")?;
        directory
            .sync_all()
            .context("sync private provider config directory")?;
        FileExt::lock(&lock).context("lock private provider config directory")?;
        Ok(lock)
    }

    pub(super) fn read_optional(directory: &File, name: &str) -> anyhow::Result<Option<Vec<u8>>> {
        let Some(mut file) = open_existing_regular(directory, name)? else {
            return Ok(None);
        };
        let mut contents = Vec::new();
        file.read_to_end(&mut contents)
            .with_context(|| format!("read private provider config {name}"))?;
        Ok(Some(contents))
    }

    fn open_existing_regular(directory: &File, name: &str) -> anyhow::Result<Option<File>> {
        match openat(
            directory,
            name,
            OFlag::O_RDONLY | OFlag::O_CLOEXEC | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK,
            Mode::empty(),
        ) {
            Ok(fd) => {
                let file = File::from(fd);
                ensure_regular(&file, name)?;
                Ok(Some(file))
            }
            Err(Errno::ENOENT) => Ok(None),
            Err(error) => Err(error).with_context(|| format!("open private provider config {name}")),
        }
    }

    fn ensure_regular(file: &File, name: &str) -> anyhow::Result<()> {
        let stat = fstat(file).with_context(|| format!("stat private provider config {name}"))?;
        anyhow::ensure!(
            SFlag::from_bits_truncate(stat.st_mode) == SFlag::S_IFREG,
            "private provider config {name} is not a regular file"
        );
        Ok(())
    }

    pub(super) fn publish_catalog<F>(
        directory: &File,
        name: &str,
        contents: &[u8],
        mut hook: F,
    ) -> anyhow::Result<()>
    where
        F: FnMut(PublishPoint) -> anyhow::Result<()>,
    {
        match read_optional(directory, name)? {
            Some(existing) if existing == contents => return Ok(()),
            Some(_) => anyhow::bail!("content-addressed Codex catalog {name} has different contents"),
            None => {}
        }

        let (temp_name, mut temp_file) = create_temp_file(directory)?;
        let result = (|| {
            hook(PublishPoint::TempCreated(Artifact::CodexCatalog))?;
            temp_file
                .write_all(contents)
                .context("write staged Codex model catalog")?;
            hook(PublishPoint::TempWritten(Artifact::CodexCatalog))?;
            temp_file.sync_all().context("sync staged Codex model catalog")?;
            hook(PublishPoint::TempSynced(Artifact::CodexCatalog))?;
            hook(PublishPoint::BeforeInstall(Artifact::CodexCatalog))?;
            match linkat(
                directory,
                temp_name.as_str(),
                directory,
                name,
                AtFlags::empty(),
            ) {
                Ok(()) => {}
                Err(Errno::EEXIST) => {
                    let existing = read_optional(directory, name)?;
                    anyhow::ensure!(
                        existing.as_deref() == Some(contents),
                        "content-addressed Codex catalog {name} changed during publication"
                    );
                    return Ok(());
                }
                Err(error) => return Err(error).context("install immutable Codex model catalog"),
            }
            hook(PublishPoint::Installed(Artifact::CodexCatalog))?;
            directory
                .sync_all()
                .context("sync installed Codex model catalog")?;
            hook(PublishPoint::DirectorySynced(Artifact::CodexCatalog))?;
            Ok(())
        })();

        cleanup_owned_temp(directory, &temp_name, &temp_file, result)
    }

    pub(super) fn publish_atomic<F>(
        directory: &File,
        name: &str,
        contents: &[u8],
        artifact: Artifact,
        mut hook: F,
    ) -> anyhow::Result<()>
    where
        F: FnMut(PublishPoint) -> anyhow::Result<()>,
    {
        // Check the current entry without following it. Renameat below cannot
        // follow a leaf symlink, but rejecting non-regular entries also keeps
        // malformed capsule state from being silently taken over.
        let _ = read_optional(directory, name)?;
        let (temp_name, mut temp_file) = create_temp_file(directory)?;
        let result = (|| {
            hook(PublishPoint::TempCreated(artifact))?;
            temp_file
                .write_all(contents)
                .with_context(|| format!("write staged private provider config {name}"))?;
            hook(PublishPoint::TempWritten(artifact))?;
            temp_file
                .sync_all()
                .with_context(|| format!("sync staged private provider config {name}"))?;
            hook(PublishPoint::TempSynced(artifact))?;
            hook(PublishPoint::BeforeInstall(artifact))?;
            renameat(directory, temp_name.as_str(), directory, name)
                .with_context(|| format!("atomically install private provider config {name}"))?;
            hook(PublishPoint::Installed(artifact))?;
            directory
                .sync_all()
                .with_context(|| format!("sync private provider config directory for {name}"))?;
            hook(PublishPoint::DirectorySynced(artifact))?;
            Ok(())
        })();

        cleanup_owned_temp(directory, &temp_name, &temp_file, result)
    }

    fn create_temp_file(directory: &File) -> anyhow::Result<(String, File)> {
        for _ in 0..128 {
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let name = format!(
                ".jackin-private-provider-config-{}-{sequence}.tmp",
                std::process::id()
            );
            match openat(
                directory,
                name.as_str(),
                OFlag::O_WRONLY
                    | OFlag::O_CREAT
                    | OFlag::O_EXCL
                    | OFlag::O_CLOEXEC
                    | OFlag::O_NOFOLLOW,
                Mode::S_IRUSR | Mode::S_IWUSR,
            ) {
                Ok(fd) => return Ok((name, File::from(fd))),
                Err(Errno::EEXIST) => continue,
                Err(error) => return Err(error).context("create private provider config staging file"),
            }
        }
        anyhow::bail!("could not allocate a private provider config staging name")
    }

    fn cleanup_owned_temp(
        directory: &File,
        temp_name: &str,
        temp_file: &File,
        result: anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let temp_stat = fstat(temp_file).context("stat owned provider config staging file")?;
        let mut removed = false;
        match fstatat(directory, temp_name, AtFlags::AT_SYMLINK_NOFOLLOW) {
            Ok(path_stat)
                if path_stat.st_dev == temp_stat.st_dev && path_stat.st_ino == temp_stat.st_ino =>
            {
                match unlinkat(directory, temp_name, UnlinkatFlags::NoRemoveDir) {
                    Ok(()) => removed = true,
                    Err(Errno::ENOENT) => {}
                    Err(error) => {
                        return Err(error).context("remove owned provider config staging file")
                    }
                }
            }
            Err(Errno::ENOENT) => {}
            Ok(_) => anyhow::bail!("provider config staging name changed ownership; left untouched"),
            Err(error) => return Err(error).context("verify owned provider config staging file"),
        }
        if removed {
            directory
                .sync_all()
                .context("sync private provider config staging cleanup")?;
        }
        result
    }
}

fn account_with_effective_model(
    account: &jackin_config::AccountConfig,
    model: Option<&str>,
) -> jackin_config::AccountConfig {
    let Some(model) = model else {
        return account.clone();
    };
    let mut account = account.clone();
    if let AccountCredential::ApiKey {
        model: account_model,
        ..
    } = &mut account.credential
    {
        *account_model = Some(model.to_owned());
    }
    account
}

pub(super) fn configure_accounts(
    root: &Path,
    config: &AppConfig,
    instances: &[jackin_config::ResolvedInstance],
    slots: &BTreeMap<String, crate::instance::ProvisionedInstanceAuth>,
    models: &BTreeMap<String, String>,
    efforts: &BTreeMap<String, String>,
) -> anyhow::Result<()> {
    for instance in instances {
        let Some(slot) = slots.get(&instance.config_id) else {
            if matches!(instance.agent, Agent::Codex | Agent::Opencode) {
                anyhow::bail!(
                    "instance {:?} has no provisioned config slot",
                    instance.config_id
                );
            }
            continue;
        };
        anyhow::ensure!(
            slot.agent == instance.agent,
            "provisioned config slot for {:?} belongs to {}, not {}",
            instance.config_id,
            slot.agent,
            instance.agent
        );
        anyhow::ensure!(
            slot.account_id == instance.account_id,
            "provisioned config slot for {:?} belongs to account {:?}, not {:?}",
            instance.config_id,
            slot.account_id,
            instance.account_id
        );
        let model = models
            .get(&instance.config_id)
            .map(String::as_str)
            .or(instance.model.as_deref());
        match instance.agent {
            Agent::Codex => configure_codex(
                root,
                config,
                instance,
                slot,
                model,
                efforts.get(&instance.config_id).map(String::as_str),
            )?,
            Agent::Opencode => configure_opencode(root, config, instance, slot, model)?,
            _ => {}
        }
    }
    Ok(())
}

#[cfg(unix)]
fn configure_codex(
    root: &Path,
    config: &AppConfig,
    instance: &jackin_config::ResolvedInstance,
    slot: &crate::instance::ProvisionedInstanceAuth,
    model: Option<&str>,
    effort: Option<&str>,
) -> anyhow::Result<()> {
    configure_codex_with_publish_hook(root, config, instance, slot, model, effort, |_| Ok(()))
}

#[cfg(not(unix))]
fn configure_codex(
    _root: &Path,
    _config: &AppConfig,
    _instance: &jackin_config::ResolvedInstance,
    _slot: &crate::instance::ProvisionedInstanceAuth,
    _model: Option<&str>,
    _effort: Option<&str>,
) -> anyhow::Result<()> {
    anyhow::bail!("private Codex config publication requires Unix descriptor-relative file operations")
}

#[cfg(unix)]
fn configure_codex_with_publish_hook<F>(
    root: &Path,
    config: &AppConfig,
    instance: &jackin_config::ResolvedInstance,
    slot: &crate::instance::ProvisionedInstanceAuth,
    model: Option<&str>,
    effort: Option<&str>,
    mut hook: F,
) -> anyhow::Result<()>
where
    F: FnMut(private_config_fs::PublishPoint) -> anyhow::Result<()>,
{
    let account = config
        .accounts
        .get(&instance.account_id)
        .ok_or_else(|| anyhow::anyhow!("unknown account {:?}", instance.account_id))?;
    let AccountCredential::ApiKey { .. } = &account.credential else {
        return Ok(());
    };
    let base_url = instance.base_url.as_deref();
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
    // `container_home_rel` is computed by the auth provisioner from the same
    // slot layout used by mounts and the Capsule's CODEX_HOME value. Never
    // collapse multiple admitted Codex instances onto the primary home.
    let directory = private_config_fs::open_directory(root, Path::new(&slot.container_home_rel))?;
    let _lock = private_config_fs::lock(&directory)?;
    let mut document: toml::Table = match private_config_fs::read_optional(&directory, "config.toml")
        .context("read private Codex configuration")?
    {
        Some(contents) => toml::from_str(
            std::str::from_utf8(&contents).context("decode private Codex configuration")?,
        )
        .context("parse private Codex configuration")?,
        None => toml::Table::new(),
    };
    let mut provider = toml::Table::new();
    provider.insert("name".into(), account.provider.slug().into());
    provider.insert("base_url".into(), base_url.unwrap_or(default_url).into());
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
    let mut catalog_to_publish = None;
    if let Some(model) = model {
        document.insert("model".into(), model.into());
        if let Some(catalog) = model_catalog(account.provider, model) {
            if let Some(effort) = effort {
                anyhow::ensure!(
                    catalog_supports_effort(&catalog, effort),
                    "Codex model {model:?} does not support reasoning effort {effort:?}"
                );
            }
            let catalog_contents = serde_json::to_vec_pretty(&catalog)?;
            let catalog_name = codex_catalog_filename(&catalog_contents);
            let catalog_target = Path::new(&slot.folder_target)
                .join(&catalog_name)
                .to_string_lossy()
                .into_owned();
            document.insert("model_catalog_json".into(), catalog_target.into());
            catalog_to_publish = Some((catalog_name, catalog_contents));
        } else {
            document.remove("model_catalog_json");
        }
    } else {
        document.remove("model");
        document.remove("model_catalog_json");
    }
    if let Some(effort) = effort {
        document.insert("model_reasoning_effort".into(), effort.into());
    } else {
        document.remove("model_reasoning_effort");
    }
    let config_contents = toml::to_string_pretty(&document)?.into_bytes();
    if let Some((catalog_name, catalog_contents)) = catalog_to_publish {
        // The catalog name is content-addressed and immutable. Sync it before
        // atomically changing config.toml, which is the pair's commit point.
        private_config_fs::publish_catalog(
            &directory,
            &catalog_name,
            &catalog_contents,
            &mut hook,
        )
        .context("publish private Codex model metadata")?;
    }
    private_config_fs::publish_atomic(
        &directory,
        "config.toml",
        &config_contents,
        private_config_fs::Artifact::CodexConfig,
        &mut hook,
    )
    .context("publish private Codex account configuration")
}

fn codex_catalog_filename(contents: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};

    format!("account-models-{:x}.json", Sha256::digest(contents))
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
        // models.dev `google` provider via the AI SDK Google package; v1beta
        // is the package default base.
        AiProvider::Google => (
            "google",
            "@ai-sdk/google",
            "https://generativelanguage.googleapis.com/v1beta",
        ),
        AiProvider::OpenRouter => (
            "openrouter",
            "@ai-sdk/openai-compatible",
            "https://openrouter.ai/api/v1",
        ),
        // No OpenCode catalog provider exists for Cursor/Meta; a custom
        // provider entry needs verified protocol/base-URL details that are
        // still unknown (Cursor's agent endpoint is proprietary; Meta's API
        // base is unconfirmed). Deferred to the provider-config lane.
        AiProvider::Cursor | AiProvider::Meta => anyhow::bail!(
            "{provider} accounts cannot authenticate OpenCode yet: no catalog provider entry"
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
    instance: &jackin_config::ResolvedInstance,
    slot: &crate::instance::ProvisionedInstanceAuth,
    model: Option<&str>,
) -> anyhow::Result<()> {
    let account = config
        .accounts
        .get(&instance.account_id)
        .ok_or_else(|| anyhow::anyhow!("unknown account {:?}", instance.account_id))?;
    let AccountCredential::ApiKey { .. } = &account.credential else {
        return Ok(());
    };
    let base_url = instance.base_url.as_deref();
    let (id, npm, default_url) = opencode_provider(account.provider)?;
    let credential_account = account_with_effective_model(account, model);
    let credentials = credential_account.credential_env(Agent::Opencode)?;
    let key = credentials
        .keys()
        .next()
        .context("OpenCode account has no credential variable")?;
    let directory = root.join("home").join(crate::instance::slot_home_rel(
        ".config/opencode",
        slot.slot_suffix.as_deref(),
    ));
    std::fs::create_dir_all(&directory)
        .context("create private OpenCode configuration directory")?;
    let mut provider = serde_json::json!({
        "name": account.name, "npm": npm,
        "options": { "baseURL": base_url.unwrap_or(default_url), "apiKey": format!("{{env:{key}}}") }
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
fn catalog_supports_effort(catalog: &serde_json::Value, effort: &str) -> bool {
    catalog["models"]
        .as_array()
        .and_then(|models| models.first())
        .and_then(|model| model["supported_reasoning_levels"].as_array())
        .is_some_and(|levels| {
            levels
                .iter()
                .any(|level| level["effort"].as_str() == Some(effort))
        })
}

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
            { "effort": "medium", "description": "Balanced reasoning" },
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
