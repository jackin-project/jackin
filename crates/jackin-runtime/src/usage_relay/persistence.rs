// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Host-only immutable launch proof. Nothing in this record accepts credential bytes.

use std::collections::BTreeMap;

use anyhow::{Context as _, Result};
use jackin_core::JackinPaths;
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCredentialScope, UsageUnresolvedGrantV2,
};
use serde::{Deserialize, Serialize};

use super::inventory::InventoryAccountAuthority;

const MAX_PROOF_BYTES: usize = 1024 * 1024;
const PROOF_FILE: &str = "launch.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SavedUsageRelay {
    schema_version: u32,
    pub(super) container_name: String,
    pub(super) container_id: String,
    pub(super) workspace: Option<String>,
    pub(super) role_key: String,
    pub(super) config_generation: String,
    pub(super) capabilities: Vec<UsageAccountCapability>,
    pub(super) credential_scope: UsageCredentialScope,
    pub(super) instance_capabilities: BTreeMap<String, UsageAccountCapability>,
    pub(super) inventory_accounts: Vec<InventoryAccountAuthority>,
    pub(super) unresolved_grants: Vec<UsageUnresolvedGrantV2>,
}

#[expect(
    clippy::too_many_arguments,
    reason = "the persisted proof binds every independent launch authority fact"
)]
pub(super) fn save(
    paths: &JackinPaths,
    container_name: &str,
    container_id: &str,
    workspace: Option<&str>,
    role_key: &str,
    config_generation: &str,
    capabilities: &[UsageAccountCapability],
    credential_scope: &UsageCredentialScope,
    instance_capabilities: &BTreeMap<String, UsageAccountCapability>,
    inventory_accounts: &[InventoryAccountAuthority],
    unresolved_grants: &[UsageUnresolvedGrantV2],
) -> Result<()> {
    let proof = SavedUsageRelay {
        schema_version: 3,
        container_name: container_name.to_owned(),
        container_id: container_id.to_owned(),
        workspace: workspace.map(str::to_owned),
        role_key: role_key.to_owned(),
        config_generation: config_generation.to_owned(),
        capabilities: capabilities.to_vec(),
        credential_scope: credential_scope.clone(),
        instance_capabilities: instance_capabilities.clone(),
        inventory_accounts: inventory_accounts.to_vec(),
        unresolved_grants: unresolved_grants.to_vec(),
    };
    validate_instance_capabilities(&proof)?;
    let bytes = serde_json::to_vec(&proof).context("serialize host relay proof")?;
    anyhow::ensure!(
        bytes.len() <= MAX_PROOF_BYTES,
        "host relay proof exceeds size bound"
    );
    anyhow::ensure!(
        !container_id.is_empty(),
        "host relay proof container ID is empty"
    );
    platform::save(paths, container_name, container_id, &bytes)
}

pub(super) fn load(paths: &JackinPaths, container_name: &str) -> Result<Option<SavedUsageRelay>> {
    let Some(bytes) = platform::load(paths, container_name)? else {
        return Ok(None);
    };
    // Do not include parser diagnostics: malformed host input must not echo its contents.
    let proof: SavedUsageRelay = serde_json::from_slice(&bytes).map_err(|_| {
        anyhow::anyhow!("host relay proof is malformed or obsolete; rebuild this Capsule")
    })?;
    anyhow::ensure!(
        proof.schema_version == 3,
        "unsupported host relay proof schema; rebuild this Capsule"
    );
    anyhow::ensure!(
        proof.container_name == container_name,
        "host relay proof container mismatch"
    );
    anyhow::ensure!(
        !proof.container_id.is_empty(),
        "host relay proof container ID is empty"
    );
    validate_instance_capabilities(&proof)?;
    Ok(Some(proof))
}

fn validate_instance_capabilities(proof: &SavedUsageRelay) -> Result<()> {
    let allowed = proof.capabilities.iter().collect::<std::collections::BTreeSet<_>>();
    anyhow::ensure!(
        proof.instance_capabilities.iter().all(|(instance_id, capability)| {
            !instance_id.is_empty() && allowed.contains(capability)
        }),
        "host relay instance capability exceeds launch authority"
    );
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use std::fs;
    use std::os::unix::fs::{PermissionsExt as _, symlink};
    use std::path::PathBuf;

    use super::*;
    use jackin_protocol::usage_broker::{
        UsageCredentialSourceIdentity, UsageCredentialSourceProof,
    };

    const NAME: &str = "jackin-relay-proof-test";

    fn layout() -> (tempfile::TempDir, JackinPaths) {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let root = paths.data_dir.join(NAME);
        fs::create_dir_all(&root).unwrap();
        fs::set_permissions(root, fs::Permissions::from_mode(0o700)).unwrap();
        (temp, paths)
    }

    fn proof_directory(paths: &JackinPaths) -> PathBuf {
        paths
            .data_dir
            .join(NAME)
            .join("provider-config/relay-proof")
    }

    fn save_test(paths: &JackinPaths, id: &str, generation: &str) -> Result<()> {
        save(
            paths,
            NAME,
            id,
            Some("workspace"),
            "role",
            generation,
            &[UsageAccountCapability {
                account_id: "opaque-account".into(),
                surface_id: "codex".into(),
            }],
            &UsageCredentialScope {
                profiles: std::collections::BTreeSet::new(),
                sources: [UsageCredentialSourceProof {
                    instance_id: "codex-instance".into(),
                    account_id: "opaque-account".into(),
                    surface_id: "codex".into(),
                    key: "OPENAI_API_KEY".into(),
                    source: UsageCredentialSourceIdentity::HostEnv {
                        name: "OPERATOR_KEY".into(),
                    },
                    material_fingerprint: "a".repeat(64),
                }]
                .into_iter()
                .collect(),
            },
            &BTreeMap::from([("codex-instance".into(), UsageAccountCapability {
                account_id: "opaque-account".into(),
                surface_id: "codex".into(),
            })]),
            &[],
            &[],
        )
    }

    #[test]
    fn immutable_lifetime_and_new_container_identity() {
        let (_temp, paths) = layout();
        save_test(&paths, "docker-id-1", "generation-1").unwrap();
        save_test(&paths, "docker-id-1", "generation-1").unwrap();
        assert!(save_test(&paths, "docker-id-1", "generation-2").is_err());
        let original = load(&paths, NAME).unwrap().unwrap();
        assert_eq!(original.container_id, "docker-id-1");
        assert_eq!(original.config_generation, "generation-1");
        save_test(&paths, "docker-id-2", "generation-2").unwrap();
        let replacement = load(&paths, NAME).unwrap().unwrap();
        assert_eq!(replacement.container_id, "docker-id-2");
        assert_eq!(replacement.workspace.as_deref(), Some("workspace"));
        assert_eq!(replacement.role_key, "role");
        assert_eq!(replacement.credential_scope, original.credential_scope);
        assert_eq!(replacement.instance_capabilities, original.instance_capabilities);
    }

    #[test]
    fn instance_map_is_mandatory_and_cannot_expand_capabilities() {
        let (_temp, paths) = layout();
        save_test(&paths, "docker-id", "generation").unwrap();
        let target = proof_directory(&paths).join(PROOF_FILE);
        let bytes = fs::read(&target).unwrap();
        let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["instance_capabilities"]["codex-instance"]["account_id"], "opaque-account");
        value.as_object_mut().unwrap().remove("instance_capabilities");
        fs::write(&target, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(load(&paths, NAME).is_err());
        let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        value["instance_capabilities"]["codex-instance"]["account_id"] = "unauthorized".into();
        fs::write(&target, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(load(&paths, NAME).is_err());
    }

    #[test]
    fn instance_map_cannot_rotate_during_same_container_lifetime() {
        let (_temp, paths) = layout();
        save_test(&paths, "docker-id", "generation").unwrap();
        let mut proof = load(&paths, NAME).unwrap().unwrap();
        let authority = proof.instance_capabilities.remove("codex-instance").unwrap();
        proof.instance_capabilities.insert("different-instance".into(), authority);
        let bytes = serde_json::to_vec(&proof).unwrap();
        assert!(platform::save(&paths, NAME, "docker-id", &bytes).is_err());
        assert!(load(&paths, NAME).unwrap().unwrap().instance_capabilities.contains_key("codex-instance"));
    }

    #[test]
    fn symlinked_directories_and_proof_fail_closed() {
        let (temp, paths) = layout();
        let outside = temp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        let provider = paths.data_dir.join(NAME).join("provider-config");
        symlink(&outside, &provider).unwrap();
        assert!(save_test(&paths, "docker-id", "generation").is_err());
        assert!(load(&paths, NAME).is_err());
        assert!(fs::read_dir(&outside).unwrap().next().is_none());
        fs::remove_file(provider).unwrap();
        save_test(&paths, "docker-id", "generation").unwrap();
        let target = proof_directory(&paths).join(PROOF_FILE);
        fs::rename(&target, outside.join("proof")).unwrap();
        symlink(outside.join("proof"), target).unwrap();
        assert!(load(&paths, NAME).is_err());
        assert!(save_test(&paths, "docker-id-2", "generation").is_err());
    }

    #[test]
    fn permissions_hardlinks_and_size_are_rejected() {
        let (temp, paths) = layout();
        save_test(&paths, "docker-id", "generation").unwrap();
        let target = proof_directory(&paths).join(PROOF_FILE);
        fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(load(&paths, NAME).is_err());
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        let alias = temp.path().join("alias");
        fs::hard_link(&target, &alias).unwrap();
        assert!(load(&paths, NAME).is_err());
        fs::remove_file(alias).unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(&target)
            .unwrap()
            .set_len(MAX_PROOF_BYTES as u64 + 1)
            .unwrap();
        assert!(load(&paths, NAME).is_err());
        fs::set_permissions(proof_directory(&paths), fs::Permissions::from_mode(0o755)).unwrap();
        assert!(load(&paths, NAME).is_err());
    }

    #[test]
    fn interrupted_temp_and_torn_record_do_not_restore_authority() {
        let (_temp, paths) = layout();
        assert!(load(&paths, NAME).unwrap().is_none());
        // An interrupted prepublication write cannot acquire the canonical filename.
        save_test(&paths, "docker-id", "generation").unwrap();
        let dir = proof_directory(&paths);
        let target = dir.join(PROOF_FILE);
        fs::remove_file(&target).unwrap();
        fs::write(dir.join(".launch-interrupted"), b"{\"schema_version\":").unwrap();
        assert!(load(&paths, NAME).unwrap().is_none());
        fs::write(&target, b"{\"schema_version\":").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(load(&paths, NAME).is_err());
        assert!(save_test(&paths, "docker-id-2", "generation").is_err());
    }

    #[test]
    fn persisted_schema_contains_only_secret_free_source_evidence() {
        let (_temp, paths) = layout();
        save_test(&paths, "docker-id", "generation").unwrap();
        let target = proof_directory(&paths).join(PROOF_FILE);
        let bytes = fs::read(&target).unwrap();
        let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let source = &value["credential_scope"]["sources"][0];
        assert_eq!(source["source"]["kind"], "host_env");
        assert_eq!(source["material_fingerprint"], "a".repeat(64));
        assert!(source.get("material").is_none());
        assert!(source.get("token").is_none());
        value["unexpected"] = serde_json::json!(true);
        fs::write(&target, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(load(&paths, NAME).is_err());
    }

    #[test]
    fn parent_symlink_and_traversal_are_rejected() {
        let (_temp, paths) = layout();
        assert!(load(&paths, "../outside").is_err());
        let moved = paths.data_dir.with_extension("moved");
        fs::rename(&paths.data_dir, &moved).unwrap();
        symlink(&moved, &paths.data_dir).unwrap();
        assert!(load(&paths, NAME).is_err());
        assert!(save_test(&paths, "docker-id", "generation").is_err());
    }
}

#[cfg(unix)]
mod platform {
    use std::ffi::OsStr;
    use std::fs::File;
    use std::io::{Read as _, Write as _};
    use std::path::{Component, Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use anyhow::{Context as _, Result};
    use nix::errno::Errno;
    use nix::fcntl::{OFlag, open, openat, renameat};
    use nix::sys::stat::{Mode, SFlag, fchmod, fstat, mkdirat};
    use nix::unistd::{UnlinkatFlags, unlinkat};

    use super::{JackinPaths, MAX_PROOF_BYTES, PROOF_FILE};

    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    const TEMP_ATTEMPTS: u64 = 32;

    fn temporary_name(sequence: u64) -> String {
        format!(".launch-{}-{sequence}", std::process::id())
    }

    fn create_temporary(directory: &File, sequence_start: u64) -> Result<(String, File)> {
        for offset in 0..TEMP_ATTEMPTS {
            let name = temporary_name(sequence_start.wrapping_add(offset));
            match openat(
                directory,
                name.as_str(),
                OFlag::O_WRONLY
                    | OFlag::O_CREAT
                    | OFlag::O_EXCL
                    | OFlag::O_CLOEXEC
                    | OFlag::O_NOFOLLOW,
                Mode::from_bits_truncate(0o600),
            ) {
                Ok(fd) => return Ok((name, File::from(fd))),
                // Interrupted prior lifetimes can leave this name behind after PID reuse.
                // Do not unlink or modify it; only a successfully created inode is ours.
                Err(Errno::EEXIST) => {}
                Err(error) => return Err(error).context("create host relay proof temporary"),
            }
        }
        anyhow::bail!("host relay proof temporary collision bound exhausted")
    }

    fn normalize(path: &Path) -> Result<PathBuf> {
        let mut root = if path.is_absolute() {
            PathBuf::new()
        } else {
            std::env::current_dir()?
        };
        for component in path.components() {
            match component {
                Component::RootDir => root.push("/"),
                Component::Normal(name) => root.push(name),
                Component::CurDir => {}
                _ => anyhow::bail!("host relay proof root has parent traversal"),
            }
        }
        #[cfg(target_os = "macos")]
        {
            use std::os::unix::ffi::{OsStrExt as _, OsStringExt as _};
            let bytes = root.as_os_str().as_bytes();
            for alias in [b"/var".as_slice(), b"/tmp".as_slice(), b"/etc".as_slice()] {
                if bytes == alias
                    || bytes
                        .strip_prefix(alias)
                        .is_some_and(|tail| tail.starts_with(b"/"))
                {
                    let mut expanded = b"/private".to_vec();
                    expanded.extend_from_slice(bytes);
                    return Ok(PathBuf::from(std::ffi::OsString::from_vec(expanded)));
                }
            }
        }
        Ok(root)
    }

    fn private_directory(file: &File) -> Result<()> {
        let stat = fstat(file)?;
        anyhow::ensure!(
            SFlag::from_bits_truncate(stat.st_mode).contains(SFlag::S_IFDIR)
                && stat.st_uid == nix::unistd::geteuid().as_raw()
                && stat.st_mode & 0o7777 == 0o700,
            "host relay proof directory must be owned by current user with mode 0700"
        );
        Ok(())
    }

    fn child(parent: &File, name: &OsStr, create: bool) -> Result<Option<File>> {
        let flags = OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC | OFlag::O_NOFOLLOW;
        let fd = match openat(parent, name, flags, Mode::empty()) {
            Ok(fd) => fd,
            Err(Errno::ENOENT) if create => {
                match mkdirat(parent, name, Mode::S_IRWXU) {
                    Ok(()) => parent.sync_all()?,
                    Err(Errno::EEXIST) => {}
                    Err(error) => return Err(error).context("create host relay proof directory"),
                }
                openat(parent, name, flags, Mode::empty())?
            }
            Err(Errno::ENOENT) => return Ok(None),
            Err(error) => return Err(error).context("open host relay proof directory"),
        };
        let file = File::from(fd);
        private_directory(&file)?;
        Ok(Some(file))
    }

    fn directory(paths: &JackinPaths, container: &str, create: bool) -> Result<Option<File>> {
        anyhow::ensure!(
            !container.is_empty()
                && !container.as_bytes().contains(&0)
                && matches!(
                    Path::new(container).components().next(),
                    Some(Component::Normal(_))
                )
                && Path::new(container).components().count() == 1,
            "invalid host relay proof container name"
        );
        let root = normalize(&paths.data_dir.join(container))?;
        let mut directory = File::from(open(
            Path::new("/"),
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC | OFlag::O_NOFOLLOW,
            Mode::empty(),
        )?);
        let components = root
            .components()
            .filter_map(|part| {
                if let Component::Normal(name) = part {
                    Some(name)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        for (index, name) in components.iter().enumerate() {
            if index + 1 == components.len() {
                let Some(next) = child(&directory, name, false)? else {
                    return Ok(None);
                };
                directory = next;
            } else {
                match openat(
                    &directory,
                    *name,
                    OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC | OFlag::O_NOFOLLOW,
                    Mode::empty(),
                ) {
                    Ok(fd) => directory = File::from(fd),
                    Err(Errno::ENOENT) => return Ok(None),
                    Err(error) => {
                        return Err(error).context("open host relay proof path component");
                    }
                }
            }
        }
        // Mount admission rejects this whole subtree and ancestor binds for both runtimes.
        for name in ["provider-config", "relay-proof"] {
            let Some(next) = child(&directory, OsStr::new(name), create)? else {
                return Ok(None);
            };
            directory = next;
        }
        Ok(Some(directory))
    }

    fn read(directory: &File) -> Result<Option<Vec<u8>>> {
        let fd = match openat(
            directory,
            PROOF_FILE,
            OFlag::O_RDONLY | OFlag::O_CLOEXEC | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK,
            Mode::empty(),
        ) {
            Ok(fd) => fd,
            Err(Errno::ENOENT) => return Ok(None),
            Err(error) => return Err(error).context("open host relay proof"),
        };
        let file = File::from(fd);
        let stat = fstat(&file)?;
        anyhow::ensure!(
            SFlag::from_bits_truncate(stat.st_mode).contains(SFlag::S_IFREG)
                && stat.st_uid == nix::unistd::geteuid().as_raw()
                && stat.st_mode & 0o7777 == 0o600
                && stat.st_nlink == 1,
            "host relay proof must be a private, singly linked regular file"
        );
        let size = u64::try_from(stat.st_size).context("host relay proof has invalid file size")?;
        anyhow::ensure!(
            size <= MAX_PROOF_BYTES as u64,
            "host relay proof exceeds size bound"
        );
        let mut bytes = Vec::new();
        file.take(MAX_PROOF_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        anyhow::ensure!(
            bytes.len() <= MAX_PROOF_BYTES,
            "host relay proof exceeds size bound"
        );
        Ok(Some(bytes))
    }

    pub(super) fn save(
        paths: &JackinPaths,
        container: &str,
        container_id: &str,
        bytes: &[u8],
    ) -> Result<()> {
        save_from_sequence(
            paths,
            container,
            container_id,
            bytes,
            SEQUENCE.fetch_add(TEMP_ATTEMPTS, Ordering::Relaxed),
        )
    }

    fn save_from_sequence(
        paths: &JackinPaths,
        container: &str,
        container_id: &str,
        bytes: &[u8],
        sequence_start: u64,
    ) -> Result<()> {
        let directory = directory(paths, container, true)?
            .ok_or_else(|| anyhow::anyhow!("host relay proof instance directory is missing"))?;
        let lock = File::from(openat(
            &directory,
            ".publish.lock",
            OFlag::O_RDWR
                | OFlag::O_CREAT
                | OFlag::O_CLOEXEC
                | OFlag::O_NOFOLLOW
                | OFlag::O_NONBLOCK,
            Mode::from_bits_truncate(0o600),
        )?);
        let stat = fstat(&lock)?;
        anyhow::ensure!(
            SFlag::from_bits_truncate(stat.st_mode).contains(SFlag::S_IFREG)
                && stat.st_uid == nix::unistd::geteuid().as_raw()
                && stat.st_mode & 0o7777 == 0o600
                && stat.st_nlink == 1,
            "host relay proof publication lock is not private"
        );
        fs4::FileExt::lock(&lock)?;
        if let Some(existing) = read(&directory)? {
            let previous: super::SavedUsageRelay =
                serde_json::from_slice(&existing).map_err(|_| {
                    anyhow::anyhow!(
                        "host relay proof is malformed or obsolete; rebuild this Capsule"
                    )
                })?;
            anyhow::ensure!(
                previous.schema_version == 3 && previous.container_name == container,
                "host relay proof has invalid binding"
            );
            if previous.container_id == container_id {
                anyhow::ensure!(
                    existing == bytes,
                    "host relay launch proof is immutable for a container lifetime"
                );
                return Ok(());
            }
        }
        let (temporary, mut file) = create_temporary(&directory, sequence_start)?;
        let publish = (|| -> Result<()> {
            fchmod(&file, Mode::from_bits_truncate(0o600))?;
            file.write_all(bytes)?;
            file.sync_all()?;
            // The caller validated ownership of the newly created container. A new
            // lifetime replaces the old proof; a running lifetime cannot rotate it.
            renameat(&directory, temporary.as_str(), &directory, PROOF_FILE)?;
            Ok(())
        })();
        let cleanup = unlinkat(&directory, temporary.as_str(), UnlinkatFlags::NoRemoveDir);
        publish.context("publish host relay proof")?;
        if let Err(error) = cleanup
            && error != Errno::ENOENT
        {
            return Err(error).context("remove host relay proof temporary");
        }
        directory
            .sync_all()
            .context("sync host relay proof publication")
    }

    pub(super) fn load(paths: &JackinPaths, container: &str) -> Result<Option<Vec<u8>>> {
        let Some(directory) = directory(paths, container, false)? else {
            return Ok(None);
        };
        read(&directory)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::os::unix::fs::PermissionsExt as _;

        #[test]
        fn interrupted_actual_candidate_is_preserved_and_next_candidate_publishes() {
            let temp = tempfile::tempdir().unwrap();
            let paths = JackinPaths::for_tests(temp.path());
            let container = "relay-collision";
            let root = paths.data_dir.join(container);
            std::fs::create_dir_all(&root).unwrap();
            std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
            let directory = directory(&paths, container, true).unwrap().unwrap();
            let name = temporary_name(0);
            let mut interrupted = File::from(
                openat(
                    &directory,
                    name.as_str(),
                    OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW,
                    Mode::from_bits_truncate(0o600),
                )
                .unwrap(),
            );
            interrupted.write_all(b"interrupted").unwrap();
            let proof = super::super::SavedUsageRelay {
                schema_version: 3,
                container_name: container.into(),
                container_id: "new-id".into(),
                workspace: None,
                role_key: "role".into(),
                config_generation: "generation".into(),
                capabilities: Vec::new(),
                credential_scope: Default::default(),
                instance_capabilities: Default::default(),
                inventory_accounts: Vec::new(),
                unresolved_grants: Vec::new(),
            };
            let bytes = serde_json::to_vec(&proof).unwrap();
            save_from_sequence(&paths, container, "new-id", &bytes, 0).unwrap();
            assert_eq!(super::super::load(&paths, container).unwrap(), Some(proof));
            let mut existing = File::from(
                openat(
                    &directory,
                    name.as_str(),
                    OFlag::O_RDONLY | OFlag::O_NOFOLLOW,
                    Mode::empty(),
                )
                .unwrap(),
            );
            let mut contents = String::new();
            existing.read_to_string(&mut contents).unwrap();
            assert_eq!(contents, "interrupted");
        }
    }
}

#[cfg(not(unix))]
mod platform {
    use super::*;
    pub(super) fn save(_: &JackinPaths, _: &str, _: &str, _: &[u8]) -> Result<()> {
        anyhow::bail!("host relay proof requires Unix descriptor-relative filesystem protection")
    }
    pub(super) fn load(_: &JackinPaths, _: &str) -> Result<Option<Vec<u8>>> {
        anyhow::bail!("host relay proof requires Unix descriptor-relative filesystem protection")
    }
}
