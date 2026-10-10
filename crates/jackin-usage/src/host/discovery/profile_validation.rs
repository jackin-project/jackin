// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Profile credential validation and refresh for discovered sources.

use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

use jackin_core::Agent;

use super::{
    CanonicalAccountIdentity, CanonicalAccountSubject, DiscoveredAccountDescriptor,
    DiscoveredCredentialSource, HostSurfaceId, ProfileCredentialMaterial,
    ProviderCredentialEnvResolver, ProviderCredentialIdentityOutcome,
    ProviderCredentialRefreshOutcome, UsageDiscoveryCatalog, UsageDiscoveryDiagnostic,
    UsageDiscoveryIssue, UsageDiscoveryUnresolvedSource, ValidatedCredentialBinding,
    ValidatedCredentialSource, ValidatedUsageDiscovery,
};

#[derive(Clone)]
pub(super) enum ProfileReadOutcome {
    Bytes(Vec<u8>),
    /// Claude profile/keychain data, retained and copied only in zeroizing
    /// buffers while profile identity is validated.
    SecretBytes(Zeroizing<Vec<u8>>),
    Missing,
    Denied,
    ConsentRequired,
}

pub(super) trait ProfileCredentialReader {
    fn read(&self, path: &Path) -> ProfileReadOutcome;
    fn read_claude_file(&self, path: &Path) -> ProfileReadOutcome {
        match self.read(path) {
            ProfileReadOutcome::Bytes(bytes) => {
                ProfileReadOutcome::SecretBytes(Zeroizing::new(bytes))
            }
            other => other,
        }
    }
    fn exists(&self, path: &Path) -> bool;
    fn read_claude_keychain(&self, scope: &jackin_core::ClaudeKeychainScope) -> ProfileReadOutcome;
    /// Presence-only probe for the Antigravity Keychain grant singleton.
    /// `Bytes` is always empty and never carries the grant: the CLI owns the
    /// secret, discovery only learns whether it exists.
    fn read_antigravity_keychain(&self) -> ProfileReadOutcome;
}

struct CachingProfileCredentialReader<'a> {
    inner: &'a dyn ProfileCredentialReader,
    exists: std::cell::RefCell<BTreeMap<PathBuf, bool>>,
    files: std::cell::RefCell<BTreeMap<PathBuf, ProfileReadOutcome>>,
    claude_files: std::cell::RefCell<BTreeMap<PathBuf, ProfileReadOutcome>>,
    keychain: std::cell::RefCell<BTreeMap<String, ProfileReadOutcome>>,
    antigravity_grant: std::cell::RefCell<Option<ProfileReadOutcome>>,
}

impl<'a> CachingProfileCredentialReader<'a> {
    fn new(inner: &'a dyn ProfileCredentialReader) -> Self {
        Self {
            inner,
            exists: std::cell::RefCell::new(BTreeMap::new()),
            files: std::cell::RefCell::new(BTreeMap::new()),
            claude_files: std::cell::RefCell::new(BTreeMap::new()),
            keychain: std::cell::RefCell::new(BTreeMap::new()),
            antigravity_grant: std::cell::RefCell::new(None),
        }
    }
}

impl ProfileCredentialReader for CachingProfileCredentialReader<'_> {
    fn read(&self, path: &Path) -> ProfileReadOutcome {
        if let Some(outcome) = self.files.borrow().get(path).cloned() {
            return outcome;
        }
        let outcome = self.inner.read(path);
        self.files
            .borrow_mut()
            .insert(path.to_path_buf(), outcome.clone());
        outcome
    }

    fn read_claude_file(&self, path: &Path) -> ProfileReadOutcome {
        if let Some(outcome) = self.claude_files.borrow().get(path).cloned() {
            return outcome;
        }
        let outcome = self.inner.read_claude_file(path);
        self.claude_files
            .borrow_mut()
            .insert(path.to_path_buf(), outcome.clone());
        outcome
    }

    fn exists(&self, path: &Path) -> bool {
        if let Some(exists) = self.exists.borrow().get(path).copied() {
            return exists;
        }
        let exists = self.inner.exists(path);
        self.exists.borrow_mut().insert(path.to_path_buf(), exists);
        exists
    }

    fn read_claude_keychain(&self, scope: &jackin_core::ClaudeKeychainScope) -> ProfileReadOutcome {
        if let Some(outcome) = self.keychain.borrow().get(&scope.service).cloned() {
            return outcome;
        }
        let outcome = match self.inner.read_claude_keychain(scope) {
            ProfileReadOutcome::Bytes(bytes) => {
                ProfileReadOutcome::SecretBytes(Zeroizing::new(bytes))
            }
            other => other,
        };
        self.keychain
            .borrow_mut()
            .insert(scope.service.clone(), outcome.clone());
        outcome
    }

    fn read_antigravity_keychain(&self) -> ProfileReadOutcome {
        if let Some(outcome) = self.antigravity_grant.borrow().clone() {
            return outcome;
        }
        let outcome = self.inner.read_antigravity_keychain();
        *self.antigravity_grant.borrow_mut() = Some(outcome.clone());
        outcome
    }
}

struct SystemProfileCredentialReader;

impl ProfileCredentialReader for SystemProfileCredentialReader {
    fn read(&self, path: &Path) -> ProfileReadOutcome {
        match std::fs::read(path) {
            Ok(bytes) => ProfileReadOutcome::Bytes(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                ProfileReadOutcome::Missing
            }
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                ProfileReadOutcome::Denied
            }
            Err(_) => ProfileReadOutcome::Missing,
        }
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn read_claude_keychain(&self, scope: &jackin_core::ClaudeKeychainScope) -> ProfileReadOutcome {
        match crate::usage::read_claude_keychain_item(&scope.service) {
            #[cfg(any(target_os = "macos", test))]
            crate::usage::ClaudeKeychainRead::Payload { json } => {
                ProfileReadOutcome::SecretBytes(Zeroizing::new(json.as_bytes().to_vec()))
            }
            crate::usage::ClaudeKeychainRead::Denied => ProfileReadOutcome::Denied,
            crate::usage::ClaudeKeychainRead::Missing => ProfileReadOutcome::Missing,
            crate::usage::ClaudeKeychainRead::ConsentRequired => {
                ProfileReadOutcome::ConsentRequired
            }
        }
    }

    fn read_antigravity_keychain(&self) -> ProfileReadOutcome {
        #[cfg(target_os = "macos")]
        {
            use security_framework::item::{ItemClass, ItemSearchOptions};

            // Reference-only search: no `load_data`, so the grant payload is
            // never read into this process — presence is the whole answer.
            let mut options = ItemSearchOptions::new();
            options
                .class(ItemClass::generic_password())
                .service(crate::usage::ANTIGRAVITY_KEYCHAIN_SERVICE)
                .limit(1);
            match options.search() {
                Ok(results) if !results.is_empty() => ProfileReadOutcome::Bytes(Vec::new()),
                Ok(_) => ProfileReadOutcome::Missing,
                Err(error) => match crate::usage::classify_claude_keychain_status(error.code()) {
                    // Unreachable: the classifier only emits Denied/Missing.
                    // Fail closed to absence either way.
                    crate::usage::ClaudeKeychainRead::Payload { .. } => ProfileReadOutcome::Missing,
                    crate::usage::ClaudeKeychainRead::Denied => ProfileReadOutcome::Denied,
                    crate::usage::ClaudeKeychainRead::Missing => ProfileReadOutcome::Missing,
                    crate::usage::ClaudeKeychainRead::ConsentRequired => {
                        ProfileReadOutcome::ConsentRequired
                    }
                },
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            ProfileReadOutcome::Missing
        }
    }
}

pub(super) enum ProfileValidation {
    Authenticated {
        provider_id: Option<String>,
        account_label: Option<String>,
        material: Option<Box<ProfileCredentialMaterial>>,
    },
    Anonymous(Option<Box<ProfileCredentialMaterial>>),
    Missing,
    Denied,
    ConsentRequired,
    Malformed,
}

struct AccountAccumulator {
    label: String,
    provenance: BTreeSet<String>,
    source_ids: BTreeSet<String>,
}

/// Validate every pre-deduplicated source and merge authenticated identities.
///
/// Missing/malformed/denied sources produce diagnostics and never account rows.
pub fn validate_usage_sources(
    catalog: UsageDiscoveryCatalog,
    env_resolver: &dyn ProviderCredentialEnvResolver,
) -> ValidatedUsageDiscovery {
    validate_usage_sources_with_reader(catalog, env_resolver, &SystemProfileCredentialReader)
}

pub(super) fn validate_usage_sources_with_reader(
    catalog: UsageDiscoveryCatalog,
    env_resolver: &dyn ProviderCredentialEnvResolver,
    profile_reader: &dyn ProfileCredentialReader,
) -> ValidatedUsageDiscovery {
    let mut diagnostics = catalog.diagnostics;
    let mut bindings = Vec::new();
    let mut accounts = BTreeMap::<CanonicalAccountIdentity, AccountAccumulator>::new();

    let profile_reader = CachingProfileCredentialReader::new(profile_reader);
    let validated: Vec<ValidatedSourceParts> = catalog
        .sources
        .into_iter()
        .map(|source| validate_source(source, env_resolver, &profile_reader))
        .collect();
    // Provider-issued identities per surface, from any source form. An
    // anonymous env/key credential carries no identity evidence of its own;
    // when exactly one same-surface provider identity exists, the key joins
    // that canonical account instead of minting a source-scoped row.
    let mut strong = BTreeMap::<HostSurfaceId, BTreeSet<CanonicalAccountIdentity>>::new();
    for (surface, _, _, _, _, _, outcome) in &validated {
        if let ProfileValidation::Authenticated {
            provider_id: Some(id),
            ..
        } = outcome
            && !id.trim().is_empty()
        {
            strong
                .entry(*surface)
                .or_default()
                .insert(CanonicalAccountIdentity {
                    surface: *surface,
                    subject: CanonicalAccountSubject::ProviderId(id.trim().to_owned()),
                });
        }
    }
    let (primary, attachable): (Vec<ValidatedSourceParts>, Vec<ValidatedSourceParts>) = validated
        .into_iter()
        .partition(|parts| !is_attachable_env_source(&parts.5, &parts.6));
    // Strong sources accumulate first so canonical labels come from
    // authenticated evidence, never from an attached anonymous key.
    for parts in primary {
        accumulate_validated_source(parts, None, &mut diagnostics, &mut bindings, &mut accounts);
    }
    for parts in attachable {
        let attach_to = match strong.get(&parts.0) {
            Some(ids) if ids.len() == 1 => ids.iter().next().cloned(),
            _ => None,
        };
        accumulate_validated_source(
            parts,
            attach_to,
            &mut diagnostics,
            &mut bindings,
            &mut accounts,
        );
    }

    let accounts = accounts
        .into_iter()
        .map(|(identity, account)| DiscoveredAccountDescriptor {
            surface_id: identity.surface.id().to_owned(),
            account_key: identity.account_key(),
            account_label: account.label,
            provenance: account.provenance.into_iter().collect(),
            source_ids: account.source_ids.into_iter().collect(),
            identity,
        })
        .collect();

    ValidatedUsageDiscovery {
        config_generation: catalog.config_generation,
        accounts,
        diagnostics,
        candidates: catalog.candidates,
        bindings,
    }
}

/// Whether an env/key source proved no identity of its own.
///
/// Anonymous API-key/OAuth-token credentials are bearer material without
/// local identity evidence. Unlike profiles (distinct local logins) and
/// forwarded capabilities (a separate trust domain), they may join the one
/// same-surface provider-authenticated account when it exists.
fn is_attachable_env_source(
    source: &ValidatedCredentialSource,
    outcome: &ProfileValidation,
) -> bool {
    if !matches!(source, ValidatedCredentialSource::Env { .. }) {
        return false;
    }
    match outcome {
        ProfileValidation::Authenticated { provider_id, .. } => {
            provider_id.as_deref().is_none_or(|id| id.trim().is_empty())
        }
        ProfileValidation::Anonymous(_) => true,
        ProfileValidation::Missing
        | ProfileValidation::Denied
        | ProfileValidation::ConsentRequired
        | ProfileValidation::Malformed => false,
    }
}

fn accumulate_validated_source(
    parts: ValidatedSourceParts,
    attach_to: Option<CanonicalAccountIdentity>,
    diagnostics: &mut Vec<UsageDiscoveryDiagnostic>,
    bindings: &mut Vec<ValidatedCredentialBinding>,
    accounts: &mut BTreeMap<CanonicalAccountIdentity, AccountAccumulator>,
) {
    let (surface, source_id, capability_id, credential_revision, provenance, source, outcome) =
        parts;
    if let Some(identity) = attach_to {
        let label = match &outcome {
            ProfileValidation::Authenticated {
                provider_id,
                account_label,
                ..
            } => account_label
                .as_deref()
                .map(str::trim)
                .filter(|label| !label.is_empty())
                .map(str::to_owned)
                .or_else(|| provider_id.clone())
                .unwrap_or_default(),
            _ => String::new(),
        };
        let entry = accounts.entry(identity.clone()).or_insert_with(|| {
            // Unreachable: the strong target accumulates first and always
            // mints its account. The fallback keeps the merge total.
            AccountAccumulator {
                label,
                provenance: BTreeSet::new(),
                source_ids: BTreeSet::new(),
            }
        });
        entry.provenance.extend(provenance.iter().cloned());
        entry.source_ids.insert(source_id.clone());
        bindings.push(ValidatedCredentialBinding {
            surface,
            identity: Some(identity),
            capability_id,
            credential_revision,
            provenance,
            source,
        });
        return;
    }

    match outcome {
        ProfileValidation::Authenticated {
            provider_id,
            account_label,
            material: _,
        } => {
            let subject = provider_id
                .as_ref()
                .filter(|id| !id.trim().is_empty())
                .map(|id| CanonicalAccountSubject::ProviderId(id.trim().to_owned()))
                .or_else(|| {
                    account_label
                        .as_ref()
                        .filter(|label| !label.trim().is_empty())
                        .map(|_| {
                            // A label is presentation evidence only. Keep
                            // source identity when the provider did not
                            // return a stronger canonical subject.
                            CanonicalAccountSubject::SourceCapability(capability_id.clone())
                        })
                });
            let Some(subject) = subject else {
                bindings.push(ValidatedCredentialBinding {
                    surface,
                    identity: None,
                    capability_id,
                    credential_revision,
                    provenance,
                    source,
                });
                return;
            };
            let identity = CanonicalAccountIdentity { surface, subject };
            let label = account_label
                .as_deref()
                .map(str::trim)
                .filter(|label| !label.is_empty())
                .map(str::to_owned)
                .or_else(|| provider_id.clone())
                .unwrap_or_default();
            let entry = accounts
                .entry(identity.clone())
                .or_insert_with(|| AccountAccumulator {
                    label,
                    provenance: BTreeSet::new(),
                    source_ids: BTreeSet::new(),
                });
            entry.provenance.extend(provenance.iter().cloned());
            entry.source_ids.insert(source_id.clone());
            bindings.push(ValidatedCredentialBinding {
                surface,
                identity: Some(identity),
                capability_id,
                credential_revision,
                provenance,
                source,
            });
        }
        ProfileValidation::Anonymous(_) => bindings.push(ValidatedCredentialBinding {
            surface,
            identity: None,
            capability_id,
            credential_revision,
            provenance,
            source,
        }),
        ProfileValidation::Missing => diagnostics.push(source_diagnostic(
            surface,
            &provenance,
            &capability_id,
            UsageDiscoveryIssue::CredentialMissing,
        )),
        ProfileValidation::Denied => diagnostics.push(source_diagnostic(
            surface,
            &provenance,
            &capability_id,
            UsageDiscoveryIssue::CredentialDenied,
        )),
        ProfileValidation::ConsentRequired => diagnostics.push(source_diagnostic(
            surface,
            &provenance,
            &capability_id,
            UsageDiscoveryIssue::KeychainConsentRequired,
        )),
        ProfileValidation::Malformed => diagnostics.push(source_diagnostic(
            surface,
            &provenance,
            &capability_id,
            UsageDiscoveryIssue::CredentialMalformed,
        )),
    }
}

pub(super) type ValidatedSourceParts = (
    HostSurfaceId,
    String,
    String,
    String,
    BTreeSet<String>,
    ValidatedCredentialSource,
    ProfileValidation,
);

pub(super) fn validate_source(
    source: DiscoveredCredentialSource,
    env_resolver: &dyn ProviderCredentialEnvResolver,
    profile_reader: &dyn ProfileCredentialReader,
) -> ValidatedSourceParts {
    match source {
        DiscoveredCredentialSource::Profile {
            surface,
            agent,
            root,
            operator_home,
            account_label,
            source_id,
            capability_id,
            provenance,
        } => {
            let outcome = profile_identity(profile_reader, agent, &root, &operator_home);
            let credential_revision =
                profile_credential_revision(profile_reader, agent, &root, &operator_home);
            let source = match &outcome {
                ProfileValidation::Authenticated { material, .. }
                | ProfileValidation::Anonymous(material) => material.clone().map_or(
                    // A material-less local profile (Muse identity, omp/hermes
                    // attribution) is unpollable by design — never a forwarded
                    // trust-domain token, so never `Capability`.
                    ValidatedCredentialSource::Unpollable,
                    |material| ValidatedCredentialSource::Profile(*material),
                ),
                _ => ValidatedCredentialSource::Capability,
            };
            let outcome = match outcome {
                ProfileValidation::Authenticated {
                    provider_id,
                    account_label: auth_label,
                    material,
                } => ProfileValidation::Authenticated {
                    provider_id,
                    account_label: auth_label.or(account_label),
                    material,
                },
                ProfileValidation::Anonymous(_) => {
                    if let Some(label) = account_label {
                        ProfileValidation::Authenticated {
                            account_label: Some(label),
                            provider_id: None,
                            material: None,
                        }
                    } else {
                        outcome
                    }
                }
                other => other,
            };
            (
                surface,
                source_id,
                capability_id,
                credential_revision,
                provenance,
                source,
                outcome,
            )
        }
        DiscoveredCredentialSource::Env {
            surface,
            handle,
            key,
            dispatch_key,
            launch_keys,
            kind: _,
            account_label,
            source_id,
            capability_id,
            provenance,
        } => {
            let material = env_resolver.source_material(surface, &key, &handle);
            let outcome = match env_resolver.identify_provider_credential(surface, &handle) {
                ProviderCredentialIdentityOutcome::Authenticated {
                    provider_id,
                    account_label: auth_label,
                } => ProfileValidation::Authenticated {
                    provider_id,
                    account_label: auth_label.or(account_label),
                    material: None,
                },
                ProviderCredentialIdentityOutcome::Anonymous => {
                    if let Some(label) = account_label {
                        ProfileValidation::Authenticated {
                            account_label: Some(label),
                            provider_id: None,
                            material: None,
                        }
                    } else {
                        ProfileValidation::Anonymous(None)
                    }
                }
                ProviderCredentialIdentityOutcome::Missing => ProfileValidation::Missing,
                ProviderCredentialIdentityOutcome::Denied => ProfileValidation::Denied,
                ProviderCredentialIdentityOutcome::Malformed => ProfileValidation::Malformed,
            };
            let credential_revision = opaque_credential_revision(&format!(
                "env:{}:{}:{}:{}",
                surface.id(),
                key,
                dispatch_key,
                handle.0
            ));
            (
                surface,
                source_id,
                capability_id,
                credential_revision,
                provenance,
                ValidatedCredentialSource::Env {
                    handle,
                    key,
                    dispatch_key,
                    launch_keys,
                    material,
                },
                outcome,
            )
        }
        DiscoveredCredentialSource::Capability {
            surface,
            account_label,
            source_id,
            capability_id,
        } => {
            let provenance = BTreeSet::from(["forwarded to Capsule".to_owned()]);
            let outcome = account_label.map_or(ProfileValidation::Anonymous(None), |label| {
                ProfileValidation::Authenticated {
                    provider_id: None,
                    account_label: Some(label),
                    material: None,
                }
            });
            (
                surface,
                source_id,
                capability_id.clone(),
                opaque_credential_revision(&format!("capability:{capability_id}")),
                provenance,
                ValidatedCredentialSource::Capability,
                outcome,
            )
        }
    }
}

fn source_diagnostic(
    surface: HostSurfaceId,
    provenance: &BTreeSet<String>,
    capability_id: &str,
    issue: UsageDiscoveryIssue,
) -> UsageDiscoveryDiagnostic {
    UsageDiscoveryDiagnostic {
        surface_id: Some(surface.id().to_owned()),
        scope_label: provenance.iter().cloned().collect::<Vec<_>>().join(", "),
        unresolved_source: Some(UsageDiscoveryUnresolvedSource {
            capability_id: capability_id.to_owned(),
            configuration_count: u32::try_from(provenance.len()).unwrap_or(u32::MAX),
        }),
        issue,
    }
}

/// Return an opaque revision for the complete credential material read for a
/// profile source. The path-derived source id is intentionally not enough:
/// providers frequently rotate tokens in place without changing the profile
/// path or account identity.
fn profile_credential_revision(
    reader: &dyn ProfileCredentialReader,
    agent: Agent,
    root: &Path,
    operator_home: &Path,
) -> String {
    if agent == Agent::Claude
        && let Some(selected_service) = crate::usage::bootstrapped_claude_service()
    {
        let Some(scope) = jackin_core::claude_keychain_scope(root, operator_home, operator_home)
        else {
            return opaque_credential_revision("claude:invalid-scope");
        };
        if selected_service != scope.service {
            return opaque_credential_revision("claude:other-selected-scope");
        }
        let mut evidence = Vec::new();
        append_profile_read(
            &mut evidence,
            "claude.selected-keychain",
            reader.read_claude_keychain(&scope),
        );
        return opaque_credential_revision(&evidence.join("|"));
    }

    let mut evidence = Vec::new();
    let mut file = |label: &str, path: PathBuf| {
        append_profile_read(&mut evidence, label, reader.read(&path));
    };
    match agent {
        Agent::Claude => {
            append_profile_read(
                &mut evidence,
                "claude.credentials",
                reader.read_claude_file(&root.join(".credentials.json")),
            );
            append_profile_read(
                &mut evidence,
                "claude.config",
                reader.read_claude_file(&root.join(".claude.json")),
            );
            if root == operator_home.join(".claude") {
                append_profile_read(
                    &mut evidence,
                    "claude.home-config",
                    reader.read_claude_file(&operator_home.join(".claude.json")),
                );
            }
            if let Some(scope) =
                jackin_core::claude_keychain_scope(root, operator_home, operator_home)
            {
                append_profile_read(
                    &mut evidence,
                    "claude.keychain",
                    reader.read_claude_keychain(&scope),
                );
            }
        }
        Agent::Codex => file("codex.auth", root.join("auth.json")),
        Agent::Amp => {
            let direct = root.join("secrets.json");
            let path = if reader.exists(&direct) {
                direct
            } else {
                root.join("data/amp/secrets.json")
            };
            file("amp.secrets", path);
        }
        Agent::Kimi => file("kimi.credentials", root.join("credentials/kimi-code.json")),
        Agent::Grok => file("grok.auth", root.join("auth.json")),
        Agent::Opencode => file("opencode.auth", root.join("auth.json")),
        Agent::Antigravity => append_profile_read(
            &mut evidence,
            "antigravity.keychain",
            reader.read_antigravity_keychain(),
        ),
        Agent::Gemini => file("gemini.oauth", root.join("oauth_creds.json")),
        Agent::Cursor => {
            file("cursor.auth", root.join("auth.json"));
            file("cursor.config", root.join("cli-config.json"));
        }
        Agent::Muse => file("muse.auth", root.join("auth.json")),
        Agent::Omp => file("omp.database", root.join("agent/agent.db")),
        Agent::Hermes => file("hermes.auth", root.join("auth.json")),
    }
    opaque_credential_revision(&evidence.join("|"))
}

fn append_profile_read(evidence: &mut Vec<String>, label: &str, outcome: ProfileReadOutcome) {
    match outcome {
        ProfileReadOutcome::Bytes(bytes) => {
            let mut hex = String::with_capacity(bytes.len().saturating_mul(2));
            for byte in &bytes {
                let _ignored = write!(hex, "{byte:02x}");
            }
            evidence.push(format!("{label}:bytes:{}:{hex}", bytes.len()));
        }
        ProfileReadOutcome::SecretBytes(bytes) => {
            let digest = Sha256::digest(bytes.as_slice());
            let mut digest_hex = String::with_capacity(digest.len().saturating_mul(2));
            for byte in &digest {
                let _ignored = write!(digest_hex, "{byte:02x}");
            }
            evidence.push(format!("{label}:secret-bytes:{}:{digest_hex}", bytes.len()));
        }
        ProfileReadOutcome::Missing => evidence.push(format!("{label}:missing")),
        ProfileReadOutcome::Denied => evidence.push(format!("{label}:denied")),
        ProfileReadOutcome::ConsentRequired => {
            evidence.push(format!("{label}:consent-required"));
        }
    }
}

fn opaque_credential_revision(evidence: &str) -> String {
    let hashed = jackin_core::account_key_hash("usage-credential-material-v2", evidence);
    hashed.strip_prefix("sha256:").unwrap_or(&hashed).to_owned()
}

pub(super) fn profile_identity(
    reader: &dyn ProfileCredentialReader,
    agent: Agent,
    root: &Path,
    operator_home: &Path,
) -> ProfileValidation {
    match agent {
        Agent::Claude => claude_profile_identity(reader, root, operator_home),
        Agent::Codex => codex_profile_identity(reader, &root.join("auth.json")),
        Agent::Amp => {
            let direct = root.join("secrets.json");
            let path = if reader.exists(&direct) {
                direct
            } else {
                root.join("data/amp/secrets.json")
            };
            amp_profile_identity(reader, &path)
        }
        Agent::Kimi => {
            let value = match read_json(reader, &root.join("credentials/kimi-code.json")) {
                Ok(Some(value)) => value,
                Ok(None) => return ProfileValidation::Missing,
                Err(outcome) => return outcome,
            };
            crate::usage::kimi_local_token_from_value(&value, chrono::Utc::now().timestamp())
                .map_or(ProfileValidation::Malformed, |token| {
                    ProfileValidation::Anonymous(Some(Box::new(ProfileCredentialMaterial::Kimi {
                        token,
                    })))
                })
        }
        Agent::Grok => grok_profile_identity(reader, &root.join("auth.json")),
        Agent::Opencode => opencode_profile_identity(reader, &root.join("auth.json")),
        // Antigravity wires through the host Keychain grant singleton: the
        // CLI owns the secret, so grant presence alone mints refresh
        // material and refresh shells out to `agy`.
        Agent::Antigravity => antigravity_profile_identity(reader),
        Agent::Gemini => gemini_profile_identity(reader, &root.join("oauth_creds.json")),
        Agent::Cursor => cursor_profile_identity(reader, root),
        // Muse stays explicitly unwired: identity is verified locally but no
        // material is minted — the secret lives in the platform credential
        // store and no pollable usage fetch exists by design
        // (`MuseKeyExchangePolicy::polling_enabled` is false), so refresh
        // cannot dispatch.
        Agent::Muse => muse_profile_identity(reader, &root.join("auth.json")),
        // omp stays explicitly unwired: it is an attribution-only aggregator
        // with no native identity or usage endpoint. SQLite store presence
        // (not content) is verified; table parsing belongs to a later lane.
        Agent::Omp => {
            if reader.exists(&root.join("agent/agent.db")) {
                ProfileValidation::Anonymous(None)
            } else {
                ProfileValidation::Missing
            }
        }
        // Hermes stays explicitly unwired: attribution-only adapter with no
        // Hermes-native quota API; usage needs caller-supplied underlying
        // buckets the refresh lane cannot produce.
        Agent::Hermes => anonymous_when_present(reader, &root.join("auth.json")),
    }
}

/// File present (any JSON shape) → anonymous binding; missing/denied/
/// malformed propagate truthfully. Used for agents whose identity
/// extraction is deferred to the usage lane.
fn anonymous_when_present(reader: &dyn ProfileCredentialReader, path: &Path) -> ProfileValidation {
    match read_json(reader, path) {
        Ok(Some(_)) => ProfileValidation::Anonymous(None),
        Ok(None) => ProfileValidation::Missing,
        Err(outcome) => outcome,
    }
}

/// Cursor identity comes from the sibling `cli-config.json` (`authInfo`
/// email), verified locally; token presence in `auth.json` is proven at
/// discovery and the path is kept as refresh material, so refresh re-reads
/// the registered root instead of a stale discovery-time copy. A
/// present-but-tokenless `auth.json` is malformed, never an anonymous
/// binding refresh cannot serve.
fn cursor_profile_identity(reader: &dyn ProfileCredentialReader, root: &Path) -> ProfileValidation {
    let auth_path = root.join("auth.json");
    let value = match read_json(reader, &auth_path) {
        Ok(Some(value)) => value,
        Ok(None) => return ProfileValidation::Missing,
        Err(outcome) => return outcome,
    };
    if crate::usage::cursor_auth_from_value(&value).is_none() {
        return ProfileValidation::Malformed;
    }
    let material = Some(Box::new(ProfileCredentialMaterial::Cursor { auth_path }));
    let label = read_json(reader, &root.join("cli-config.json"))
        .ok()
        .flatten()
        .and_then(|config| crate::usage::cursor_cli_identity_from_value(&config));
    match label {
        Some(label) => ProfileValidation::Authenticated {
            provider_id: None,
            account_label: Some(label),
            material,
        },
        None => ProfileValidation::Anonymous(material),
    }
}

/// Gemini identity comes from `oauth_creds.json` when it names the login;
/// any valid credential file mints material (the Grok shape), since refresh
/// only needs discovery-proven OAuth presence until an entitlement endpoint
/// lands.
fn gemini_profile_identity(reader: &dyn ProfileCredentialReader, path: &Path) -> ProfileValidation {
    let value = match read_json(reader, path) {
        Ok(Some(value)) => value,
        Ok(None) => return ProfileValidation::Missing,
        Err(outcome) => return outcome,
    };
    let material = Some(Box::new(ProfileCredentialMaterial::Gemini {
        creds_path: path.to_path_buf(),
    }));
    first_recursive_string(&value, &["email", "user_email", "user_id", "account"]).map_or(
        ProfileValidation::Anonymous(material.clone()),
        |label| ProfileValidation::Authenticated {
            provider_id: None,
            account_label: Some(label),
            material,
        },
    )
}

/// Antigravity identity is the host Keychain grant singleton, probed for
/// presence only: the CLI owns the secret, so any payload is ignored and no
/// identity label is extracted. Grant present → anonymous binding with
/// refresh material; absent/denied propagates truthfully.
fn antigravity_profile_identity(reader: &dyn ProfileCredentialReader) -> ProfileValidation {
    match reader.read_antigravity_keychain() {
        ProfileReadOutcome::Bytes(_) => {
            ProfileValidation::Anonymous(Some(Box::new(ProfileCredentialMaterial::Antigravity)))
        }
        ProfileReadOutcome::SecretBytes(_) => ProfileValidation::Malformed,
        ProfileReadOutcome::Missing => ProfileValidation::Missing,
        ProfileReadOutcome::Denied => ProfileValidation::Denied,
        ProfileReadOutcome::ConsentRequired => ProfileValidation::ConsentRequired,
    }
}

/// Muse identity comes from `auth.json` (`providers.meta.user_email`),
/// verified locally; the secret itself stays in the host Keychain.
fn muse_profile_identity(reader: &dyn ProfileCredentialReader, path: &Path) -> ProfileValidation {
    let value = match read_json(reader, path) {
        Ok(Some(value)) => value,
        Ok(None) => return ProfileValidation::Missing,
        Err(outcome) => return outcome,
    };
    let label = value
        .pointer("/providers/meta/user_email")
        .or_else(|| value.pointer("/providers/meta/user_full_name"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map(str::to_owned);
    match label {
        Some(label) => ProfileValidation::Authenticated {
            provider_id: None,
            account_label: Some(label),
            material: None,
        },
        None => ProfileValidation::Anonymous(None),
    }
}

pub(super) fn opencode_profile_identity(
    reader: &dyn ProfileCredentialReader,
    path: &Path,
) -> ProfileValidation {
    match reader.read(path) {
        ProfileReadOutcome::Missing => {
            if path
                .parent()
                .map(|parent| parent.join("opencode.db"))
                .is_some_and(|database| reader.exists(&database))
            {
                // Database-only OpenCode stores have no materializable auth
                // source. Do not advertise a usage profile until the database
                // credential identity can be carried through launch binding.
                ProfileValidation::Malformed
            } else {
                ProfileValidation::Missing
            }
        }
        ProfileReadOutcome::Denied => ProfileValidation::Denied,
        ProfileReadOutcome::ConsentRequired => ProfileValidation::ConsentRequired,
        ProfileReadOutcome::Bytes(bytes) => {
            let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
                return ProfileValidation::Malformed;
            };
            let Some(entries) = value.as_object() else {
                return ProfileValidation::Malformed;
            };
            if entries.len() != 1 {
                return ProfileValidation::Malformed;
            }
            let entry = value.get("opencode-go");
            let Some(entry) = entry else {
                return ProfileValidation::Missing;
            };
            let kind = entry.get("type").and_then(serde_json::Value::as_str);
            let key = entry
                .get("key")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|key| !key.is_empty());
            if kind != Some("api") || key.is_none() {
                return ProfileValidation::Malformed;
            }
            ProfileValidation::Anonymous(Some(Box::new(ProfileCredentialMaterial::OpenCode {
                auth_path: path.to_path_buf(),
            })))
        }
        ProfileReadOutcome::SecretBytes(_) => ProfileValidation::Malformed,
    }
}

fn claude_profile_identity(
    reader: &dyn ProfileCredentialReader,
    root: &Path,
    operator_home: &Path,
) -> ProfileValidation {
    let Some(scope) = jackin_core::claude_keychain_scope(root, operator_home, operator_home) else {
        return ProfileValidation::Malformed;
    };
    if let Some(selected_service) = crate::usage::bootstrapped_claude_service() {
        if selected_service != scope.service {
            return ProfileValidation::ConsentRequired;
        }
        return match read_claude_payload(reader.read_claude_keychain(&scope)) {
            Ok(Some(profile)) => claude_profile_material(
                profile,
                format!("OAuth · macOS Keychain ({})", scope.service),
                &scope.service,
            ),
            Ok(None) => ProfileValidation::Missing,
            Err(outcome) => outcome,
        };
    }

    let mut paths = vec![root.join(".credentials.json"), root.join(".claude.json")];
    if root == operator_home.join(".claude") {
        paths.push(operator_home.join(".claude.json"));
    }
    let mut selected_profile = None;
    let mut account_email = None;
    let mut organization_type = None;
    for path in paths {
        match read_claude_payload(reader.read_claude_file(&path)) {
            Ok(Some(profile)) => {
                if selected_profile.is_none() && profile.credential.is_some() {
                    selected_profile = Some((path.clone(), profile.credential));
                }
                if account_email.is_none() {
                    account_email = profile.account_email;
                }
                if organization_type.is_none() {
                    organization_type = profile.organization_type;
                }
            }
            Ok(None) => {}
            Err(ProfileValidation::Denied) => return ProfileValidation::Denied,
            Err(ProfileValidation::ConsentRequired) => return ProfileValidation::ConsentRequired,
            Err(_) => return ProfileValidation::Malformed,
        }
    }
    if let Some((path, Some(credential))) = selected_profile {
        return claude_profile_material(
            crate::usage::ClaudeProfilePayload {
                credential: Some(credential),
                account_email,
                organization_type,
            },
            format!("OAuth · {}", path.display()),
            &scope.service,
        );
    }

    let outcome = reader.read_claude_keychain(&scope);
    match read_claude_payload(outcome) {
        Ok(Some(profile)) => claude_profile_material(
            profile,
            format!("OAuth · macOS Keychain ({})", scope.service),
            &scope.service,
        ),
        Ok(None) => ProfileValidation::Missing,
        Err(outcome) => outcome,
    }
}

fn read_claude_payload(
    outcome: ProfileReadOutcome,
) -> Result<Option<crate::usage::ClaudeProfilePayload>, ProfileValidation> {
    match outcome {
        ProfileReadOutcome::SecretBytes(bytes) => {
            crate::usage::parse_claude_profile_payload(bytes.as_slice())
                .map(Some)
                .ok_or(ProfileValidation::Malformed)
        }
        ProfileReadOutcome::Bytes(bytes) => {
            let bytes = Zeroizing::new(bytes);
            crate::usage::parse_claude_profile_payload(bytes.as_slice())
                .map(Some)
                .ok_or(ProfileValidation::Malformed)
        }
        ProfileReadOutcome::Missing => Ok(None),
        ProfileReadOutcome::Denied => Err(ProfileValidation::Denied),
        ProfileReadOutcome::ConsentRequired => Err(ProfileValidation::ConsentRequired),
    }
}

fn claude_profile_material(
    profile: crate::usage::ClaudeProfilePayload,
    origin: String,
    service: &str,
) -> ProfileValidation {
    let Some(credential) = profile.credential else {
        return ProfileValidation::Malformed;
    };
    let label = profile
        .account_email
        .clone()
        .unwrap_or_else(|| "Claude profile".to_owned());
    let material = Some(Box::new(ProfileCredentialMaterial::Claude(
        crate::usage::ClaudeResolved {
            access_token: credential.access_token,
            subscription_type: credential.subscription_type,
            account_email: profile.account_email,
            organization_type: profile.organization_type,
            credential_origin: origin,
            keychain_service: Some(service.to_owned()),
            is_anonymous: false,
        },
    )));
    ProfileValidation::Authenticated {
        provider_id: None,
        account_label: Some(label),
        material,
    }
}

fn codex_profile_identity(reader: &dyn ProfileCredentialReader, path: &Path) -> ProfileValidation {
    let value = match read_json(reader, path) {
        Ok(Some(value)) => value,
        Ok(None) => return ProfileValidation::Missing,
        Err(outcome) => return outcome,
    };
    let Some(credentials) = crate::usage::codex_oauth_from_value(&value) else {
        return ProfileValidation::Malformed;
    };
    let material = Some(Box::new(ProfileCredentialMaterial::Codex {
        credentials: credentials.clone(),
        root: path.parent().unwrap_or_else(|| Path::new("")).to_path_buf(),
    }));
    if credentials.account_id.is_none() && credentials.account_label.is_none() {
        ProfileValidation::Anonymous(material)
    } else {
        ProfileValidation::Authenticated {
            provider_id: credentials.account_id,
            account_label: credentials.account_label,
            material,
        }
    }
}

fn amp_profile_identity(reader: &dyn ProfileCredentialReader, path: &Path) -> ProfileValidation {
    let value = match read_json(reader, path) {
        Ok(Some(value)) => value,
        Ok(None) => return ProfileValidation::Missing,
        Err(outcome) => return outcome,
    };
    let Some(object) = value.as_object() else {
        return ProfileValidation::Malformed;
    };
    let labeled = object.iter().find_map(|(key, value)| {
        let label = key.strip_prefix("apiKey@")?.trim();
        let secret = value.as_str()?.trim();
        (!label.is_empty() && !secret.is_empty()).then(|| (label.to_owned(), secret.to_owned()))
    });
    let fallback_key = object.values().find_map(|value| {
        value
            .as_str()
            .map(str::trim)
            .filter(|secret| !secret.is_empty())
            .map(str::to_owned)
    });
    let Some(key) = labeled
        .as_ref()
        .map(|(_, key)| key.clone())
        .or(fallback_key)
    else {
        return ProfileValidation::Malformed;
    };
    let material = Some(Box::new(ProfileCredentialMaterial::Amp { key }));
    labeled.map_or(
        ProfileValidation::Anonymous(material.clone()),
        |(label, _)| ProfileValidation::Authenticated {
            provider_id: None,
            account_label: Some(label),
            material,
        },
    )
}

fn grok_profile_identity(reader: &dyn ProfileCredentialReader, path: &Path) -> ProfileValidation {
    let value = match read_json(reader, path) {
        Ok(Some(value)) => value,
        Ok(None) => return ProfileValidation::Missing,
        Err(outcome) => return outcome,
    };
    let material = Some(Box::new(ProfileCredentialMaterial::Grok {
        auth_path: path.to_path_buf(),
    }));
    first_recursive_string(&value, &["email", "user_id", "team_id"]).map_or(
        ProfileValidation::Anonymous(material.clone()),
        |label| ProfileValidation::Authenticated {
            provider_id: None,
            account_label: Some(label),
            material,
        },
    )
}

fn read_json(
    reader: &dyn ProfileCredentialReader,
    path: &Path,
) -> Result<Option<serde_json::Value>, ProfileValidation> {
    match reader.read(path) {
        ProfileReadOutcome::Bytes(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|_| ProfileValidation::Malformed),
        ProfileReadOutcome::SecretBytes(_) => Err(ProfileValidation::Malformed),
        ProfileReadOutcome::Missing => Ok(None),
        ProfileReadOutcome::Denied => Err(ProfileValidation::Denied),
        ProfileReadOutcome::ConsentRequired => Err(ProfileValidation::ConsentRequired),
    }
}

fn first_recursive_string(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    match value {
        serde_json::Value::Object(map) => {
            for key in keys {
                if let Some(found) = map
                    .get(*key)
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|found| !found.is_empty())
                {
                    return Some(found.to_owned());
                }
            }
            map.values()
                .find_map(|nested| first_recursive_string(nested, keys))
        }
        serde_json::Value::Array(values) => values
            .iter()
            .find_map(|nested| first_recursive_string(nested, keys)),
        _ => None,
    }
}

pub(in crate::host) fn refresh_credential_binding(
    binding: &ValidatedCredentialBinding,
    env_resolver: &dyn ProviderCredentialEnvResolver,
) -> ProviderCredentialRefreshOutcome {
    let mut failure_metadata = None;
    let (view, rate_limit) = match &binding.source {
        ValidatedCredentialSource::Env {
            handle,
            dispatch_key,
            ..
        } => {
            return env_resolver.refresh_provider_credential(binding.surface, dispatch_key, handle);
        }
        ValidatedCredentialSource::Capability => {
            return ProviderCredentialRefreshOutcome::Malformed;
        }
        // Deliberate no-poll, never a provider outage: the honest
        // `Unsupported` view flows through the success path, outside
        // retry/backoff.
        ValidatedCredentialSource::Unpollable => (
            crate::usage::unpollable_snapshot(
                binding.surface.agent_slug(),
                binding.surface.provider_label(),
                chrono::Utc::now().timestamp(),
            ),
            None,
        ),
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Claude(resolved)) => {
            let (view, rate_limit, metadata) = crate::usage::claude_view_from_wave_with_metadata(
                binding.surface.agent_slug(),
                binding.surface.provider_label(),
                chrono::Utc::now().timestamp(),
                crate::usage::ClaudeWaveResolution::Resolved(Box::new(resolved.clone())),
            );
            failure_metadata = metadata;
            (view, rate_limit)
        }
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Codex {
            credentials,
            root,
        }) => crate::usage::codex_profile_snapshot_with_rate_limit(
            binding.surface.agent_slug(),
            credentials,
            root,
            chrono::Utc::now().timestamp(),
        ),
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Amp { key }) => (
            crate::usage::amp_api_key_snapshot(
                binding.surface.agent_slug(),
                key,
                chrono::Utc::now().timestamp(),
            ),
            None,
        ),
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Grok { auth_path }) => {
            let now = chrono::Utc::now().timestamp();
            let result = crate::usage::fetch_grok_rest_billing(auth_path, now)
                .map(|response| crate::usage::GrokBillingSnapshot::Rest(Box::new(response)));
            crate::usage::grok_snapshot_from_rpc_result_with_rate_limit(
                binding.surface.agent_slug(),
                now,
                auth_path,
                true,
                false,
                false,
                result,
            )
        }
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Kimi { token }) => {
            let now = chrono::Utc::now().timestamp();
            (
                crate::usage::kimi_snapshot(
                    binding.surface.agent_slug(),
                    Some(token.as_str()),
                    now,
                ),
                None,
            )
        }
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::OpenCode { auth_path }) => (
            crate::usage::opencode_profile_snapshot(
                binding.surface.agent_slug(),
                auth_path,
                chrono::Utc::now().timestamp(),
            ),
            None,
        ),
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Cursor { auth_path }) => (
            crate::usage::cursor_profile_snapshot(
                binding.surface.agent_slug(),
                auth_path,
                chrono::Utc::now().timestamp(),
            ),
            None,
        ),
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Gemini { creds_path }) => {
            // Re-prove OAuth presence at refresh: a file deleted after
            // discovery is NeedsSecret, never a stale Unsupported.
            let has_oauth = creds_path.is_file();
            (
                crate::usage::gemini_snapshot_with_presence(
                    binding.surface.agent_slug(),
                    binding.surface.provider_label(),
                    has_oauth,
                    false,
                    "OAuth · configured profile",
                    chrono::Utc::now().timestamp(),
                ),
                None,
            )
        }
        // The Keychain grant needs no secret material here: `agy` owns the
        // grant and the collector shells out to it.
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Antigravity) => (
            crate::usage::antigravity_snapshot(
                binding.surface.agent_slug(),
                binding.surface.provider_label(),
                chrono::Utc::now().timestamp(),
            ),
            None,
        ),
    };
    ProviderCredentialRefreshOutcome::Snapshot {
        view: Box::new(view),
        rate_limit,
        failure_metadata,
    }
}
