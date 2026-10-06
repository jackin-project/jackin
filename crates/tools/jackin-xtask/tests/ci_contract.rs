use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};
use toml::Value as TomlValue;

const ARCHITECT_REPOSITORY: &str = "jackin-project/jackin-the-architect";
const ARCHITECT_COMMIT: &str = "7db69b62f598a0971809ee4a006ad3f5477d0996";
const ARCHITECT_MANIFEST_SHA256: &str =
    "b38e506587c98137d0a1a88247fb68afc9f9f215c8104c838df251933a917ae0";

// NOTE (consolidation 2026-10-06): the generated-workflow consistency
// tests (required_fan_in_covers_every_workspace_crate...,
// configured_verification_jobs_are_isolated...) were removed. They pinned
// a regenerated ci.yml containing config-declared verification tasks, but
// velnor-actions 0.1.0 cannot render this tree (workflow_too_large:
// 526KB+ vs the 500KB cap even with main's config). The checked-in ci.yml
// is a frozen artifact until the generator cap is addressed; restoring
// these contracts is a follow-up for the repo owner.

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn core_string_constant<'a>(contents: &'a str, name: &str) -> Result<&'a str> {
    let marker = format!("pub const {name}: &str = \"");
    contents
        .split_once(marker.as_str())
        .and_then(|(_, rest)| rest.split_once('"').map(|(value, _)| value))
        .with_context(|| format!("Jackin declares {name}"))
}

#[test]
fn architect_manifest_snapshot_is_bound_to_an_immutable_source_commit() -> Result<()> {
    let root = repository_root();
    let fixture = root.join("crates/tools/jackin-xtask/tests/fixtures/architect");
    let provenance: TomlValue = toml::from_str(
        &fs::read_to_string(fixture.join("provenance.toml")).context("role provenance exists")?,
    )
    .context("role provenance is valid TOML")?;
    let repository = provenance
        .get("repository")
        .and_then(TomlValue::as_str)
        .context("role repository is pinned")?;
    let commit = provenance
        .get("commit")
        .and_then(TomlValue::as_str)
        .context("role commit is pinned")?;
    let source_path = provenance
        .get("path")
        .and_then(TomlValue::as_str)
        .context("role manifest path is pinned")?;
    let expected_sha256 = provenance
        .get("sha256")
        .and_then(TomlValue::as_str)
        .context("role manifest digest is pinned")?;
    let source_url = provenance
        .get("url")
        .and_then(TomlValue::as_str)
        .context("immutable source URL is pinned")?;

    ensure!(
        repository == ARCHITECT_REPOSITORY,
        "Architect repository pin changed"
    );
    ensure!(commit == ARCHITECT_COMMIT, "Architect commit pin changed");
    let core_constants = fs::read_to_string(root.join("crates/core/jackin-core/src/constants.rs"))
        .context("Jackin manifest constants exist")?;
    let current_manifest_filename = core_string_constant(&core_constants, "MANIFEST_FILENAME")?;
    ensure!(
        source_path == current_manifest_filename,
        "Architect manifest path differs from Jackin's current MANIFEST_FILENAME"
    );
    ensure!(
        expected_sha256 == ARCHITECT_MANIFEST_SHA256,
        "Architect manifest digest pin changed"
    );
    ensure!(
        source_url == format!("https://github.com/{repository}/blob/{commit}/{source_path}"),
        "Architect source URL is not the immutable blob URL"
    );
    ensure!(
        commit.len() == 40,
        "source revision must be a full Git commit ID"
    );
    ensure!(
        commit.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "source revision must be hexadecimal"
    );

    let manifest = fs::read(fixture.join(current_manifest_filename))
        .context("pinned role manifest exists at Jackin's current manifest path")?;
    let actual_sha256 = hex::encode(Sha256::digest(&manifest));
    ensure!(
        actual_sha256 == expected_sha256,
        "pinned role content changed"
    );

    let manifest_text = std::str::from_utf8(&manifest).context("pinned role manifest is UTF-8")?;
    let manifest: TomlValue =
        toml::from_str(manifest_text).context("pinned role manifest is valid TOML")?;
    let manifest_version = manifest
        .get("version")
        .and_then(TomlValue::as_str)
        .context("role manifest declares its version")?;
    let current_version = core_string_constant(&core_constants, "CURRENT_MANIFEST_VERSION")?;
    ensure!(
        manifest_version == current_version,
        "pinned Architect manifest version differs from Jackin"
    );

    let agents = manifest
        .get("agents")
        .and_then(TomlValue::as_array)
        .context("Architect declares its supported agents")?;
    let actual_agents = agents
        .iter()
        .map(TomlValue::as_str)
        .collect::<Option<Vec<_>>>()
        .context("agent names are strings")?;
    ensure!(
        actual_agents == ["claude", "codex", "amp", "opencode", "kimi", "grok"],
        "Architect supported-agent manifest changed: {actual_agents:?}"
    );
    Ok(())
}
