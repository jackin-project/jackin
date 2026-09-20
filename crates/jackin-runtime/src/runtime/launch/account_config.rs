// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Materialize selected API account settings in the private capsule home.
#![expect(
    clippy::disallowed_methods,
    reason = "private config publication runs inside the launch blocking task"
)]

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Context as _;
use jackin_config::{AccountCredential, AiProvider, AppConfig};
use jackin_core::Agent;

static PRIVATE_CONFIG_SWAP_COUNTER: AtomicU64 = AtomicU64::new(0);
const PRIVATE_CONFIG_TRANSACTION_VERSION: u8 = 1;
const PRIVATE_CONFIG_TRANSACTION_FILE: &str = ".jackin-private-config-transaction";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PrivateConfigFailurePoint {
    StagedFile(&'static str),
    BeforeSwap,
    AfterPreviousRename,
    AfterInstall,
    #[cfg(test)]
    SimulatedCrashAfterPreviousRename,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct PrivateConfigTransaction {
    schema_version: u8,
    target: String,
    staged: String,
    previous: Option<String>,
    phase: PrivateConfigTransactionPhase,
}

#[derive(serde::Deserialize, serde::Serialize)]
enum PrivateConfigTransactionPhase {
    Prepared,
    PreviousMoved,
    Installed,
}

#[cfg(test)]
thread_local! {
    static PRIVATE_CONFIG_FAILURE: std::cell::Cell<Option<PrivateConfigFailurePoint>> =
        const { std::cell::Cell::new(None) };
}

fn maybe_inject_private_config_failure(point: PrivateConfigFailurePoint) -> anyhow::Result<()> {
    #[cfg(test)]
    if PRIVATE_CONFIG_FAILURE.with(|failure| failure.get() == Some(point)) {
        anyhow::bail!("injected private-config publication failure at {point:?}");
    }

    #[cfg(not(test))]
    let _ = point;
    Ok(())
}

fn private_config_entry_path(directory: &Path, name: &str) -> anyhow::Result<PathBuf> {
    let components = Path::new(name).components().collect::<Vec<_>>();
    anyhow::ensure!(
        components.len() == 1 && matches!(components[0], Component::Normal(_)),
        "private config entry must be a single file name: {name:?}"
    );
    Ok(directory.join(name))
}

fn sync_private_config_directory(path: &Path) -> std::io::Result<()> {
    std::fs::File::open(path)?.sync_all()
}

fn unique_private_config_sibling(parent: &Path, prefix: &str) -> anyhow::Result<PathBuf> {
    for _ in 0..128 {
        let sequence = PRIVATE_CONFIG_SWAP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(".{prefix}-{}-{sequence}", std::process::id()));
        match std::fs::symlink_metadata(&path) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(path),
            Err(error) => return Err(error.into()),
        }
    }
    anyhow::bail!("could not allocate a private config swap path")
}

fn validate_private_config_root(root: &Path) -> anyhow::Result<()> {
    let metadata = std::fs::symlink_metadata(root)
        .with_context(|| format!("inspect private config root {}", root.display()))?;
    anyhow::ensure!(
        !metadata.file_type().is_symlink() && metadata.is_dir(),
        "private config root is not a real directory: {}",
        root.display()
    );
    Ok(())
}

fn private_config_components_below<'a>(
    root: &Path,
    path: &'a Path,
) -> anyhow::Result<Vec<Component<'a>>> {
    validate_private_config_root(root)?;
    let relative = path.strip_prefix(root).with_context(|| {
        format!(
            "private config path {} escapes root {}",
            path.display(),
            root.display()
        )
    })?;
    let components = relative.components().collect::<Vec<_>>();
    anyhow::ensure!(
        !components
            .iter()
            .any(|component| matches!(component, Component::ParentDir)),
        "private config path contains a parent traversal: {}",
        path.display()
    );
    Ok(components)
}

fn validate_private_config_ancestors(root: &Path, path: &Path) -> anyhow::Result<()> {
    let mut current = root.to_path_buf();
    for component in private_config_components_below(root, path)? {
        anyhow::ensure!(
            !matches!(component, Component::ParentDir),
            "private config path contains a parent traversal: {}",
            path.display()
        );
        current.push(component.as_os_str());
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) => {
                anyhow::ensure!(
                    !metadata.file_type().is_symlink(),
                    "private config ancestor is a symlink: {}",
                    current.display()
                );
                anyhow::ensure!(
                    metadata.is_dir(),
                    "private config ancestor is not a directory: {}",
                    current.display()
                );
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn ensure_private_config_parent(root: &Path, parent: &Path) -> anyhow::Result<()> {
    let mut current = root.to_path_buf();
    for component in private_config_components_below(root, parent)? {
        anyhow::ensure!(
            !matches!(component, Component::ParentDir),
            "private config path contains a parent traversal: {}",
            parent.display()
        );
        current.push(component.as_os_str());
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) => {
                anyhow::ensure!(
                    !metadata.file_type().is_symlink(),
                    "private config ancestor is a symlink: {}",
                    current.display()
                );
                anyhow::ensure!(
                    metadata.is_dir(),
                    "private config ancestor is not a directory: {}",
                    current.display()
                );
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match std::fs::create_dir(&current) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(error.into()),
                }
                let metadata = std::fs::symlink_metadata(&current)?;
                anyhow::ensure!(
                    !metadata.file_type().is_symlink() && metadata.is_dir(),
                    "private config ancestor became non-directory: {}",
                    current.display()
                );
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn private_config_directory_exists(path: &Path) -> anyhow::Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            anyhow::ensure!(
                !metadata.file_type().is_symlink(),
                "private config directory is a symlink: {}",
                path.display()
            );
            anyhow::ensure!(
                metadata.is_dir(),
                "private config path is not a directory: {}",
                path.display()
            );
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn copy_private_config_tree(source: &Path, destination: &Path) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let metadata = std::fs::symlink_metadata(&source_path)?;
        anyhow::ensure!(
            !metadata.file_type().is_symlink(),
            "refusing to publish private config through symlink {}",
            source_path.display()
        );
        if metadata.is_dir() {
            std::fs::create_dir(&destination_path)?;
            copy_private_config_tree(&source_path, &destination_path)?;
            std::fs::set_permissions(&destination_path, metadata.permissions())?;
        } else if metadata.is_file() {
            std::fs::copy(&source_path, &destination_path)?;
            std::fs::set_permissions(&destination_path, metadata.permissions())?;
        } else {
            anyhow::bail!(
                "refusing to publish private config with special entry {}",
                source_path.display()
            );
        }
    }
    Ok(())
}

fn existing_private_config_permissions(
    directory: &Path,
    name: &str,
) -> anyhow::Result<Option<std::fs::Permissions>> {
    let path = private_config_entry_path(directory, name)?;
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) => {
            anyhow::ensure!(
                metadata.is_file(),
                "private config entry is not a regular file: {}",
                path.display()
            );
            Ok(Some(metadata.permissions()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn write_private_config_file(
    directory: &Path,
    name: &str,
    bytes: &[u8],
    permissions: Option<std::fs::Permissions>,
) -> anyhow::Result<()> {
    use std::io::Write as _;

    let path = private_config_entry_path(directory, name)?;
    let mut file = tempfile::NamedTempFile::new_in(directory)?;
    file.write_all(bytes)?;
    if let Some(permissions) = permissions {
        file.as_file().set_permissions(permissions)?;
    }
    file.as_file().sync_all()?;
    file.persist(&path).map_err(|error| error.error)?;
    Ok(())
}

fn remove_private_config_file(directory: &Path, name: &str) -> anyhow::Result<()> {
    let path = private_config_entry_path(directory, name)?;
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) => {
            anyhow::ensure!(
                metadata.is_file(),
                "private config entry is not a regular file: {}",
                path.display()
            );
            std::fs::remove_file(path)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn private_config_transaction_path(parent: &Path) -> PathBuf {
    parent.join(PRIVATE_CONFIG_TRANSACTION_FILE)
}

fn persist_private_config_transaction(
    parent: &Path,
    transaction: &PrivateConfigTransaction,
) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec(transaction)?;
    write_private_config_file(parent, PRIVATE_CONFIG_TRANSACTION_FILE, &bytes, None)?;
    sync_private_config_directory(parent)?;
    Ok(())
}

fn clear_private_config_transaction(parent: &Path) -> anyhow::Result<()> {
    remove_private_config_file(parent, PRIVATE_CONFIG_TRANSACTION_FILE)?;
    sync_private_config_directory(parent)?;
    Ok(())
}

fn require_private_config_previous(previous: Option<&Path>) -> anyhow::Result<&Path> {
    previous.context("private config transaction is missing its previous directory")
}

fn recover_private_config_transaction(parent: &Path) -> anyhow::Result<()> {
    let journal_path = private_config_transaction_path(parent);
    let Some(bytes) = read_private_config_file(&journal_path)? else {
        return Ok(());
    };
    let transaction: PrivateConfigTransaction =
        serde_json::from_slice(&bytes).context("parse private config transaction journal")?;
    anyhow::ensure!(
        transaction.schema_version == PRIVATE_CONFIG_TRANSACTION_VERSION,
        "unsupported private config transaction schema {}",
        transaction.schema_version
    );
    anyhow::ensure!(
        transaction.target != PRIVATE_CONFIG_TRANSACTION_FILE
            && transaction.staged != PRIVATE_CONFIG_TRANSACTION_FILE
            && transaction.previous.as_deref() != Some(PRIVATE_CONFIG_TRANSACTION_FILE),
        "private config transaction journal targets its own journal"
    );
    anyhow::ensure!(
        transaction.target != transaction.staged
            && transaction.previous.as_deref() != Some(transaction.target.as_str())
            && transaction.previous.as_deref() != Some(transaction.staged.as_str()),
        "private config transaction journal contains duplicate paths"
    );

    let target = private_config_entry_path(parent, &transaction.target)?;
    let staged = private_config_entry_path(parent, &transaction.staged)?;
    let previous = transaction
        .previous
        .as_deref()
        .map(|name| private_config_entry_path(parent, name))
        .transpose()?;
    let target_exists = private_config_directory_exists(&target)?;
    let staged_exists = private_config_directory_exists(&staged)?;
    let previous_exists = previous
        .as_deref()
        .map(private_config_directory_exists)
        .transpose()?
        .unwrap_or(false);

    match transaction.phase {
        PrivateConfigTransactionPhase::Prepared => {
            if previous_exists {
                anyhow::ensure!(
                    !target_exists,
                    "private config transaction has both live and previous directories"
                );
                std::fs::rename(
                    require_private_config_previous(previous.as_deref())?,
                    &target,
                )?;
            } else if transaction.previous.is_some() {
                anyhow::ensure!(
                    target_exists,
                    "private config transaction lost both live and previous directories"
                );
            } else {
                anyhow::ensure!(
                    target_exists || staged_exists,
                    "private config transaction lost both live and staged directories"
                );
            }
            if staged_exists {
                std::fs::remove_dir_all(&staged)?;
            }
        }
        PrivateConfigTransactionPhase::PreviousMoved => {
            if target_exists && previous_exists {
                anyhow::ensure!(
                    !staged_exists,
                    "private config transaction has ambiguous live, staged, and previous directories"
                );
                std::fs::remove_dir_all(require_private_config_previous(previous.as_deref())?)?;
            } else if !target_exists && previous_exists {
                if staged_exists {
                    std::fs::remove_dir_all(&staged)?;
                }
                std::fs::rename(
                    require_private_config_previous(previous.as_deref())?,
                    &target,
                )?;
            } else {
                anyhow::bail!(
                    "private config transaction lost its previous directory before recovery"
                );
            }
        }
        PrivateConfigTransactionPhase::Installed => {
            anyhow::ensure!(
                target_exists,
                "installed private config transaction has no live directory"
            );
            if staged_exists {
                std::fs::remove_dir_all(&staged)?;
            }
            if previous_exists {
                std::fs::remove_dir_all(require_private_config_previous(previous.as_deref())?)?;
            }
        }
    }
    sync_private_config_directory(parent)?;
    clear_private_config_transaction(parent)
}

fn restore_private_config_swap(
    directory: &Path,
    previous: Option<&Path>,
    installed: bool,
) -> anyhow::Result<()> {
    let mut rollback_error = None;
    if installed
        && let Err(error) = std::fs::remove_dir_all(directory)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        rollback_error = Some(anyhow::Error::new(error));
    }
    if let Some(previous) = previous
        && rollback_error.is_none()
        && let Err(error) = std::fs::rename(previous, directory)
    {
        rollback_error = Some(anyhow::Error::new(error));
    }
    if let Some(rollback_error) = rollback_error {
        anyhow::bail!("private config rollback failed: {rollback_error:#}");
    }
    Ok(())
}

fn abort_private_config_transaction(
    parent: &Path,
    directory: &Path,
    previous: Option<&Path>,
    installed: bool,
    cause: anyhow::Error,
) -> anyhow::Result<()> {
    if let Err(rollback_error) = restore_private_config_swap(directory, previous, installed) {
        return Err(cause.context(format!(
            "private config publication failed and rollback failed: {rollback_error:#}"
        )));
    }
    if let Err(clear_error) = clear_private_config_transaction(parent) {
        return Err(cause.context(format!(
            "private config publication failed and journal cleanup failed: {clear_error:#}"
        )));
    }
    Err(cause)
}

fn publish_private_config_directory(
    root: &Path,
    directory: &Path,
    files: &[(&'static str, Vec<u8>)],
    remove_files: &[&str],
) -> anyhow::Result<()> {
    let parent = directory
        .parent()
        .context("private config directory has no parent")?;
    ensure_private_config_parent(root, parent)?;
    recover_private_config_transaction(parent)?;
    validate_private_config_ancestors(root, directory)?;
    let existing = match std::fs::symlink_metadata(directory) {
        Ok(metadata) => {
            anyhow::ensure!(
                metadata.is_dir(),
                "private config path is not a directory: {}",
                directory.display()
            );
            Some(metadata)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };

    // Stage the complete directory beside the live path. Copying unrelated
    // entries preserves the private home while making every generated file
    // visible only after one directory publication.
    let staged_directory = tempfile::Builder::new()
        .prefix(".jackin-private-config-stage-")
        .tempdir_in(parent)?;
    if let Some(existing) = existing.as_ref() {
        copy_private_config_tree(directory, staged_directory.path())?;
        std::fs::set_permissions(staged_directory.path(), existing.permissions())?;
    }

    for name in remove_files {
        remove_private_config_file(staged_directory.path(), name)?;
    }
    for (name, bytes) in files {
        let permissions = existing_private_config_permissions(directory, name)?;
        write_private_config_file(staged_directory.path(), name, bytes, permissions)?;
        maybe_inject_private_config_failure(PrivateConfigFailurePoint::StagedFile(name))?;
    }
    sync_private_config_directory(staged_directory.path())?;
    maybe_inject_private_config_failure(PrivateConfigFailurePoint::BeforeSwap)?;

    let target_name = directory
        .file_name()
        .and_then(|name| name.to_str())
        .context("private config directory name is not valid UTF-8")?;
    let staged_name = staged_directory
        .path()
        .file_name()
        .and_then(|name| name.to_str())
        .context("private config staging directory name is not valid UTF-8")?;
    let previous_directory = if existing.is_some() {
        let previous = unique_private_config_sibling(parent, "jackin-private-config-previous")?;
        Some(previous)
    } else {
        None
    };
    let previous_name = previous_directory
        .as_deref()
        .map(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned)
                .context("generated private config swap path is not valid UTF-8")
        })
        .transpose()?;
    let mut transaction = PrivateConfigTransaction {
        schema_version: PRIVATE_CONFIG_TRANSACTION_VERSION,
        target: target_name.to_owned(),
        staged: staged_name.to_owned(),
        previous: previous_name,
        phase: PrivateConfigTransactionPhase::Prepared,
    };
    persist_private_config_transaction(parent, &transaction)?;

    if let Some(previous_directory) = previous_directory.as_deref() {
        // The journal is durable before this rename. A restart can therefore
        // restore the previous directory even if the process dies in the
        // absence gap before the staged directory is installed.
        std::fs::rename(directory, previous_directory)?;
        transaction.phase = PrivateConfigTransactionPhase::PreviousMoved;
        persist_private_config_transaction(parent, &transaction)?;
    }

    #[cfg(test)]
    if PRIVATE_CONFIG_FAILURE.with(|failure| {
        failure.get() == Some(PrivateConfigFailurePoint::SimulatedCrashAfterPreviousRename)
    }) {
        // Model process death: do not run the in-process rollback or TempDir
        // cleanup. The next launch must recover from the journal and siblings.
        std::mem::forget(staged_directory);
        return Err(anyhow::anyhow!(
            "simulated process crash after private config rename"
        ));
    }

    if let Err(error) =
        maybe_inject_private_config_failure(PrivateConfigFailurePoint::AfterPreviousRename)
    {
        return abort_private_config_transaction(
            parent,
            directory,
            previous_directory.as_deref(),
            false,
            error,
        );
    }

    if let Err(error) = std::fs::rename(staged_directory.path(), directory) {
        return abort_private_config_transaction(
            parent,
            directory,
            previous_directory.as_deref(),
            false,
            error.into(),
        );
    }
    transaction.phase = PrivateConfigTransactionPhase::Installed;
    // If this phase write fails, leave the journal in its previous durable
    // phase. Recovery will infer an installed tree from the live/staged
    // siblings and finish or roll back safely on the next launch.
    persist_private_config_transaction(parent, &transaction)?;
    if let Err(error) = maybe_inject_private_config_failure(PrivateConfigFailurePoint::AfterInstall)
    {
        return abort_private_config_transaction(
            parent,
            directory,
            previous_directory.as_deref(),
            true,
            error,
        );
    }
    if let Err(error) = sync_private_config_directory(parent) {
        return abort_private_config_transaction(
            parent,
            directory,
            previous_directory.as_deref(),
            true,
            error.into(),
        );
    }
    if let Some(previous_directory) = previous_directory {
        std::fs::remove_dir_all(previous_directory)?;
        sync_private_config_directory(parent)?;
    }
    clear_private_config_transaction(parent)
}

fn read_private_config_file(path: &Path) -> anyhow::Result<Option<Vec<u8>>> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            anyhow::ensure!(
                metadata.is_file(),
                "private config path is not a regular file: {}",
                path.display()
            );
            Ok(Some(std::fs::read(path)?))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
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

fn configure_codex(
    root: &Path,
    config: &AppConfig,
    instance: &jackin_config::ResolvedInstance,
    slot: &crate::instance::ProvisionedInstanceAuth,
    model: Option<&str>,
    effort: Option<&str>,
) -> anyhow::Result<()> {
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
    let directory = root.join("home").join(&slot.container_home_rel);
    validate_private_config_ancestors(root, &directory)?;
    let path = directory.join("config.toml");
    let mut document: toml::Table =
        match read_private_config_file(&path).context("read private Codex configuration")? {
            Some(contents) => {
                toml::from_slice(&contents).context("parse private Codex configuration")?
            }
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
    let mut files = Vec::new();
    if let Some(model) = model {
        document.insert("model".into(), model.into());
        if let Some(catalog) = model_catalog(account.provider, model) {
            if let Some(effort) = effort {
                anyhow::ensure!(
                    catalog_supports_effort(&catalog, effort),
                    "Codex model {model:?} does not support reasoning effort {effort:?}"
                );
            }
            files.push(("account-models.json", serde_json::to_vec_pretty(&catalog)?));
            let catalog_target = Path::new(&slot.folder_target)
                .join("account-models.json")
                .to_string_lossy()
                .into_owned();
            document.insert("model_catalog_json".into(), catalog_target.into());
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
    files.push((
        "config.toml",
        toml::to_string_pretty(&document)?.into_bytes(),
    ));
    publish_private_config_directory(root, &directory, &files, &["account-models.json"])
        .context("publish private Codex account configuration")
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
    publish_private_config_directory(
        root,
        &directory,
        &[("opencode.json", serde_json::to_vec_pretty(&document)?)],
        &[],
    )
    .context("publish private OpenCode account configuration")
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
