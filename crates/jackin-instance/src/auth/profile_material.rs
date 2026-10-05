// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Credential proofs derived exclusively from an already captured snapshot.

use std::path::Path;

use jackin_config::{AiProvider, KimiRuntimeAuthSlot, ProfileSelector};
use jackin_core::{Agent, ProfileCredentialSourceMaterial};

/// Bind captured primary credential bytes to the complete selected descriptor.
pub(super) fn captured_profile_material(
    agent: Agent,
    provider: Option<AiProvider>,
    selector: Option<&ProfileSelector>,
    effective_source_dir: &Path,
    kimi_auth_slot: Option<&KimiRuntimeAuthSlot>,
    snapshot_root: &Path,
) -> anyhow::Result<Option<ProfileCredentialSourceMaterial>> {
    let provider = provider.or_else(|| AiProvider::for_agent(agent));
    let Some(provider) = provider else {
        return Ok(None);
    };
    if agent == Agent::Antigravity {
        return Ok(None);
    }
    capture_material(
        agent,
        provider,
        selector,
        effective_source_dir,
        kimi_auth_slot,
        snapshot_root,
    )
    .map(Some)
}

#[cfg(unix)]
fn capture_material(
    agent: Agent,
    provider: AiProvider,
    selector: Option<&ProfileSelector>,
    effective_source_dir: &Path,
    kimi_auth_slot: Option<&KimiRuntimeAuthSlot>,
    snapshot_root: &Path,
) -> anyhow::Result<ProfileCredentialSourceMaterial> {
    use jackin_core::{profile_credential_material_revision, profile_credential_source_identity};

    let root = super::auth_directory::open_directory_path(snapshot_root)?;
    let bytes = if agent == Agent::Kimi {
        let selected = kimi_auth_slot
            .ok_or_else(|| anyhow::anyhow!("selected Kimi credential path is missing"))?;
        let selected = &selected.credential_relative_path;
        let name = selected
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow::anyhow!("selected Kimi credential filename is invalid"))?;
        super::auth_directory::read_locked_source_file(
            &root,
            &["credentials", name],
            "captured Kimi profile credential payload",
        )?
    } else {
        let components: &[&str] = match agent {
            Agent::Claude => &[".credentials.json"],
            Agent::Codex
            | Agent::Grok
            | Agent::Opencode
            | Agent::Cursor
            | Agent::Muse
            | Agent::Hermes => &["auth.json"],
            Agent::Amp => &["secrets.json"],
            Agent::Gemini => &["oauth_creds.json"],
            Agent::Omp => &["agent", "agent.db"],
            Agent::Antigravity => {
                anyhow::bail!("Antigravity has no profile credential payload")
            }
            Agent::Kimi => unreachable!("Kimi uses its selected credential file"),
        };
        super::auth_directory::read_locked_source_file(
            &root,
            components,
            "captured profile credential payload",
        )?
    }
    .ok_or_else(|| anyhow::anyhow!("captured profile credential payload is missing"))?;
    let material_revision = profile_credential_material_revision(agent, &bytes)
        .map_err(|_| anyhow::anyhow!("captured profile credential payload is invalid JSON"))?;
    let selector_value = profile_selector_value(selector, kimi_auth_slot)?;
    Ok(ProfileCredentialSourceMaterial {
        source: profile_credential_source_identity(
            agent,
            provider.slug(),
            effective_source_dir,
            selector_value.as_ref(),
        ),
        material_revision,
    })
}

#[cfg(not(unix))]
fn capture_material(
    agent: Agent,
    provider: AiProvider,
    selector: Option<&ProfileSelector>,
    effective_source_dir: &Path,
    kimi_auth_slot: Option<&KimiRuntimeAuthSlot>,
    snapshot_root: &Path,
) -> anyhow::Result<ProfileCredentialSourceMaterial> {
    use jackin_core::{profile_credential_material_revision, profile_credential_source_identity};

    let relative = if agent == Agent::Kimi {
        kimi_auth_slot
            .ok_or_else(|| anyhow::anyhow!("selected Kimi credential path is missing"))?
            .credential_relative_path
            .to_path_buf()
    } else {
        let components: &[&str] = match agent {
            Agent::Claude => &[".credentials.json"],
            Agent::Codex
            | Agent::Grok
            | Agent::Opencode
            | Agent::Cursor
            | Agent::Muse
            | Agent::Hermes => &["auth.json"],
            Agent::Amp => &["secrets.json"],
            Agent::Gemini => &["oauth_creds.json"],
            Agent::Omp => &["agent", "agent.db"],
            Agent::Antigravity => {
                anyhow::bail!("Antigravity has no profile credential payload")
            }
            Agent::Kimi => unreachable!("Kimi uses its selected credential file"),
        };
        components
            .iter()
            .fold(std::path::PathBuf::new(), |path, component| {
                path.join(component)
            })
    };
    anyhow::ensure!(
        relative.is_relative()
            && relative
                .components()
                .all(|component| matches!(component, std::path::Component::Normal(_))),
        "captured profile credential path is invalid"
    );
    let path = snapshot_root.join(&relative);
    let metadata = std::fs::symlink_metadata(&path)?;
    anyhow::ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "captured profile credential payload is not a regular file"
    );
    let bytes = super::read_bounded_local_file(&path)?;
    let material_revision = profile_credential_material_revision(agent, &bytes)
        .map_err(|_| anyhow::anyhow!("captured profile credential payload is invalid JSON"))?;
    let selector_value = profile_selector_value(selector, kimi_auth_slot)?;
    Ok(ProfileCredentialSourceMaterial {
        source: profile_credential_source_identity(
            agent,
            provider.slug(),
            effective_source_dir,
            selector_value.as_ref(),
        ),
        material_revision,
    })
}

fn profile_selector_value(
    selector: Option<&ProfileSelector>,
    kimi_auth_slot: Option<&KimiRuntimeAuthSlot>,
) -> anyhow::Result<Option<serde_json::Value>> {
    let configured = selector
        .map(serde_json::to_value)
        .transpose()
        .map_err(|_| anyhow::anyhow!("profile credential selector cannot be represented"))?;
    if let Some(slot) = kimi_auth_slot {
        let mut value = serde_json::Map::new();
        if let Some(configured) = configured {
            value.insert("profile".to_owned(), configured);
        }
        value.insert(
            "kimi_runtime_auth_slot".to_owned(),
            serde_json::to_value(slot)
                .map_err(|_| anyhow::anyhow!("Kimi runtime auth slot cannot be represented"))?,
        );
        Ok(Some(serde_json::Value::Object(value)))
    } else {
        Ok(configured)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::captured_profile_material;
    use jackin_config::AiProvider;
    use jackin_core::{Agent, profile_credential_material_revision};
    use std::path::Path;

    #[test]
    fn proof_reads_snapshot_without_reopening_original_source() -> anyhow::Result<()> {
        let snapshot = tempfile::tempdir()?;
        let bytes = br#"{"token":"fixture"}"#;
        std::fs::write(snapshot.path().join("auth.json"), bytes)?;
        let material = captured_profile_material(
            Agent::Hermes,
            Some(AiProvider::OpenAi),
            None,
            Path::new("/missing-original-fixture-source"),
            None,
            snapshot.path(),
        )?
        .ok_or_else(|| anyhow::anyhow!("expected captured profile proof"))?;
        assert_eq!(
            material.material_revision,
            profile_credential_material_revision(Agent::Hermes, bytes)?
        );
        Ok(())
    }

    #[test]
    fn missing_primary_payload_fails_closed() -> anyhow::Result<()> {
        let snapshot = tempfile::tempdir()?;
        let result = captured_profile_material(
            Agent::Hermes,
            Some(AiProvider::OpenAi),
            None,
            Path::new("/fixture-source"),
            None,
            snapshot.path(),
        );
        assert!(matches!(result, Err(_)));
        Ok(())
    }

    #[test]
    fn amp_proof_uses_only_the_canonical_server_credential() -> anyhow::Result<()> {
        let snapshot = tempfile::tempdir()?;
        let captured = br#"{
            "apiKey@https://ampcode.com/": "canonical-fixture",
            "apiKey@https://foreign.example/": "foreign-fixture",
            "mcp:github": "mcp-fixture"
        }"#;
        std::fs::write(snapshot.path().join("secrets.json"), captured)?;
        let material = captured_profile_material(
            Agent::Amp,
            Some(AiProvider::Amp),
            None,
            Path::new("/fixture/amp/data/amp"),
            None,
            snapshot.path(),
        )?
        .ok_or_else(|| anyhow::anyhow!("expected captured Amp profile proof"))?;
        let canonical = br#"{"apiKey@https://ampcode.com/":"canonical-fixture"}"#;
        assert_eq!(
            material.material_revision,
            profile_credential_material_revision(Agent::Amp, canonical)?
        );
        Ok(())
    }

    #[test]
    #[cfg(unix)]
    fn amp_capture_filters_before_workers_and_binds_nested_source_path() -> anyhow::Result<()> {
        let fixture = tempfile::tempdir()?;
        let source = fixture.path().join("amp-source");
        let effective = source.join("data/amp");
        std::fs::create_dir_all(&effective)?;
        std::fs::write(
            effective.join("secrets.json"),
            br#"{
                "apiKey@https://ampcode.com/": "selected-fixture",
                "apiKey@https://foreign.example/": "foreign-fixture",
                "mcp:github": "mcp-fixture"
            }"#,
        )?;
        let snapshot = super::super::capture_selected_source(
            Agent::Amp,
            Some(AiProvider::Amp),
            None,
            &source,
            fixture.path(),
            &fixture.path().join("protected/snapshots"),
        )?
        .ok_or_else(|| anyhow::anyhow!("expected Amp source snapshot"))?;
        std::fs::rename(&source, fixture.path().join("replaced-amp-source"))?;
        assert_eq!(
            std::fs::read(snapshot.materialized_source_dir().join("secrets.json"))?,
            br#"{"apiKey@https://ampcode.com/":"selected-fixture"}"#
        );
        let material = snapshot
            .profile_material()
            .ok_or_else(|| anyhow::anyhow!("expected Amp profile material"))?;
        assert_eq!(
            material.source,
            jackin_core::profile_credential_source_identity(Agent::Amp, "amp", &effective, None,)
        );
        Ok(())
    }

    #[test]
    fn presence_only_and_unbound_sources_need_no_payload() -> anyhow::Result<()> {
        let missing = Path::new("/missing-fixture-snapshot");
        assert_eq!(
            captured_profile_material(
                Agent::Antigravity,
                Some(AiProvider::Google),
                None,
                missing,
                None,
                missing
            )?,
            None
        );
        assert_eq!(
            captured_profile_material(Agent::Hermes, None, None, missing, None, missing)?,
            None
        );
        Ok(())
    }
}
