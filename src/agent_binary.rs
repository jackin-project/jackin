use crate::agent::Agent;
use crate::paths::JackinPaths;
use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read as _;
use std::path::{Path, PathBuf};

const CACHE_TTL: std::time::Duration = std::time::Duration::from_hours(1);

#[derive(Debug, Clone)]
pub struct AgentBinary {
    pub agent: Agent,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRelease {
    pub agent: Agent,
    pub version: String,
    pub url: String,
    pub checksum: Option<String>,
    pub archive_member: Option<String>,
}

pub async fn latest_release(paths: &JackinPaths, agent: Agent) -> Option<AgentRelease> {
    if let Some(cached) = read_cached_release(paths, agent) {
        return Some(cached);
    }
    let release = resolve_latest_release(agent).await.ok()?;
    let _ = write_cached_release(paths, &release);
    let _ = write_version_release(paths, &release);
    Some(release)
}

pub async fn ensure_available(paths: &JackinPaths, agent: Agent) -> Result<AgentBinary> {
    let stub_path = test_stub_path(paths, agent);
    if is_valid_cached_binary(&stub_path) {
        return Ok(AgentBinary {
            agent,
            path: stub_path,
        });
    }

    let release = resolve_latest_release(agent)
        .await
        .with_context(|| format!("resolving latest {} binary", agent.slug()))?;
    let _ = write_cached_release(paths, &release);
    let _ = write_version_release(paths, &release);
    let cached = cached_binary_path(paths, &release);
    if is_valid_cached_binary(&cached) {
        crate::debug_log!(
            "agent_binary",
            "cache hit for {} {} at {}",
            agent.slug(),
            release.version,
            cached.display()
        );
        return Ok(AgentBinary {
            agent,
            path: cached,
        });
    }
    download_and_cache(&release, &cached).await?;
    Ok(AgentBinary {
        agent,
        path: cached,
    })
}

pub fn cached_binary_path(paths: &JackinPaths, release: &AgentRelease) -> PathBuf {
    paths
        .cache_dir
        .join("agent-binaries")
        .join(release.agent.slug())
        .join(release.version.replace('+', "_"))
        .join(format!("linux-{}", container_arch()))
        .join(release.agent.slug())
}

pub const fn container_arch() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "amd64"
    }
}

async fn resolve_latest_release(agent: Agent) -> Result<AgentRelease> {
    match agent {
        Agent::Claude => resolve_claude().await,
        Agent::Codex => resolve_codex().await,
        Agent::Amp => resolve_amp().await,
        Agent::Kimi => resolve_kimi().await,
        Agent::Opencode => resolve_opencode().await,
    }
}

async fn resolve_claude() -> Result<AgentRelease> {
    let base = "https://downloads.claude.ai/claude-code-releases";
    let version = curl_text(&format!("{base}/latest")).await?;
    let version = version.trim().to_string();
    let platform = platform_x64_arm64();
    let manifest: ClaudeManifest =
        serde_json::from_str(&curl_text(&format!("{base}/{version}/manifest.json")).await?)?;
    let entry = manifest
        .platforms
        .get(platform)
        .with_context(|| format!("Claude manifest missing platform {platform}"))?;
    Ok(AgentRelease {
        agent: Agent::Claude,
        version: version.clone(),
        url: format!("{base}/{version}/{platform}/{}", entry.binary),
        checksum: Some(entry.checksum.clone()),
        archive_member: None,
    })
}

async fn resolve_amp() -> Result<AgentRelease> {
    let base = "https://static.ampcode.com";
    let version = curl_text(&format!("{base}/cli/cli-version.txt"))
        .await?
        .trim()
        .to_string();
    let platform = match container_arch() {
        "arm64" => "linux-arm64",
        _ => "linux-x64",
    };
    let checksum = curl_text(&format!("{base}/cli/{version}/{platform}-amp.sha256"))
        .await?
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string();
    Ok(AgentRelease {
        agent: Agent::Amp,
        version: version.clone(),
        url: format!("{base}/cli/{version}/amp-{platform}"),
        checksum: Some(checksum),
        archive_member: None,
    })
}

async fn resolve_kimi() -> Result<AgentRelease> {
    let base = "https://code.kimi.com/kimi-code";
    let version = curl_text(&format!("{base}/latest"))
        .await?
        .trim()
        .to_string();
    let platform = platform_x64_arm64();
    let manifest: KimiManifest =
        serde_json::from_str(&curl_text(&format!("{base}/{version}/manifest.json")).await?)?;
    let entry = manifest
        .platforms
        .get(platform)
        .with_context(|| format!("Kimi manifest missing platform {platform}"))?;
    Ok(AgentRelease {
        agent: Agent::Kimi,
        version: version.clone(),
        url: format!("{base}/{version}/{}", entry.filename),
        checksum: Some(entry.checksum.clone()),
        archive_member: None,
    })
}

async fn resolve_codex() -> Result<AgentRelease> {
    let arch = match container_arch() {
        "arm64" => "aarch64-unknown-linux-musl",
        _ => "x86_64-unknown-linux-musl",
    };
    let asset = format!("codex-{arch}.tar.gz");
    let release = github_latest_asset("openai/codex", &asset).await?;
    let checksum = release.asset.sha256_digest();
    Ok(AgentRelease {
        agent: Agent::Codex,
        version: release
            .tag_name
            .trim_start_matches("rust-v")
            .trim_start_matches('v')
            .to_string(),
        url: release.asset.browser_download_url,
        checksum,
        archive_member: Some(format!("codex-{arch}")),
    })
}

async fn resolve_opencode() -> Result<AgentRelease> {
    let arch = match container_arch() {
        "arm64" => "arm64",
        _ => "x64",
    };
    let asset = format!("opencode-linux-{arch}.tar.gz");
    let release = github_latest_asset("anomalyco/opencode", &asset).await?;
    let checksum = release.asset.sha256_digest();
    Ok(AgentRelease {
        agent: Agent::Opencode,
        version: release.tag_name.trim_start_matches('v').to_string(),
        url: release.asset.browser_download_url,
        checksum,
        archive_member: Some("opencode".to_string()),
    })
}

async fn github_latest_asset(repo: &str, asset_name: &str) -> Result<GithubReleaseAssetMatch> {
    let release: GithubRelease = serde_json::from_str(
        &curl_text(&format!(
            "https://api.github.com/repos/{repo}/releases/latest"
        ))
        .await?,
    )
    .with_context(|| format!("parsing latest GitHub release metadata for {repo}"))?;
    let asset = release
        .assets
        .into_iter()
        .find(|asset| asset.name == asset_name)
        .with_context(|| format!("{repo} latest release missing asset {asset_name}"))?;
    Ok(GithubReleaseAssetMatch {
        tag_name: release.tag_name,
        asset,
    })
}

fn platform_x64_arm64() -> &'static str {
    match container_arch() {
        "arm64" => "linux-arm64",
        _ => "linux-x64",
    }
}

async fn download_and_cache(release: &AgentRelease, dest: &Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp_download = dest.with_extension("download.tmp");
    let tmp_binary = dest.with_extension("tmp");
    download_file(&release.url, &tmp_download).await?;
    if let Some(expected) = &release.checksum {
        let actual = hash_file_sha256(&tmp_download)?;
        anyhow::ensure!(
            actual.eq_ignore_ascii_case(expected),
            "{} checksum mismatch for {}\n  expected {}\n  actual   {}",
            release.agent.slug(),
            release.url,
            expected,
            actual
        );
    }
    if let Some(member) = &release.archive_member {
        extract_tar_gz_member(&tmp_download, member, &tmp_binary)?;
        let _ = std::fs::remove_file(&tmp_download);
    } else {
        std::fs::rename(&tmp_download, &tmp_binary)?;
    }
    chmod_executable(&tmp_binary)?;
    std::fs::rename(&tmp_binary, dest)?;
    Ok(())
}

async fn curl_text(url: &str) -> Result<String> {
    let output = tokio::process::Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--max-time",
            "30",
            url,
        ])
        .output()
        .await
        .with_context(|| format!("running curl for {url}"))?;
    anyhow::ensure!(
        output.status.success(),
        "{url} failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    String::from_utf8(output.stdout).with_context(|| format!("{url} returned non-UTF-8 body"))
}

async fn download_file(url: &str, dest: &Path) -> Result<()> {
    let dest_str = dest
        .to_str()
        .with_context(|| format!("download path {} is not UTF-8", dest.display()))?;
    let status = tokio::process::Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--output",
            dest_str,
            url,
        ])
        .status()
        .await
        .with_context(|| format!("running curl for {url}"))?;
    if !status.success() {
        let _ = std::fs::remove_file(dest);
        anyhow::bail!("{url} download failed with {status}");
    }
    Ok(())
}

fn extract_tar_gz_member(archive: &Path, member_name: &str, dest: &Path) -> Result<()> {
    let file = std::fs::File::open(archive)?;
    let decoder = GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;
        if path.file_name().and_then(|n| n.to_str()) == Some(member_name) {
            let mut out = std::fs::File::create(dest)?;
            std::io::copy(&mut entry, &mut out)?;
            return Ok(());
        }
    }
    anyhow::bail!("archive missing {member_name}")
}

fn hash_file_sha256(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut hex, "{byte:02x}")?;
    }
    Ok(hex)
}

#[cfg(unix)]
fn chmod_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn chmod_executable(_path: &Path) -> Result<()> {
    Ok(())
}

fn is_valid_cached_binary(path: &Path) -> bool {
    path.is_file()
}

fn test_stub_path(paths: &JackinPaths, agent: Agent) -> PathBuf {
    paths
        .cache_dir
        .join("agent-binaries-test-stub")
        .join(agent.slug())
}

pub fn install_test_stub(paths: &JackinPaths, agent: Agent) -> Result<()> {
    let stub = test_stub_path(paths, agent);
    if let Some(parent) = stub.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&stub, b"#!/bin/sh\nprintf 'stub agent binary\\n'\n")?;
    chmod_executable(&stub)?;
    Ok(())
}

fn metadata_cache_path(paths: &JackinPaths, agent: Agent) -> PathBuf {
    paths
        .cache_dir
        .join("agent-binaries")
        .join(agent.slug())
        .join("latest.json")
}

fn version_metadata_path(paths: &JackinPaths, release: &AgentRelease) -> PathBuf {
    cached_binary_path(paths, release).with_file_name("metadata.json")
}

fn read_cached_release(paths: &JackinPaths, agent: Agent) -> Option<AgentRelease> {
    let path = metadata_cache_path(paths, agent);
    let metadata = std::fs::metadata(&path).ok()?;
    let modified = metadata.modified().ok()?;
    if std::time::SystemTime::now().duration_since(modified).ok()? >= CACHE_TTL {
        return None;
    }
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn write_cached_release(paths: &JackinPaths, release: &AgentRelease) -> Result<()> {
    let path = metadata_cache_path(paths, release.agent);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string(release)?)?;
    Ok(())
}

fn write_version_release(paths: &JackinPaths, release: &AgentRelease) -> Result<()> {
    let path = version_metadata_path(paths, release);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(release)?)?;
    Ok(())
}

#[derive(Debug, Deserialize)]
struct ClaudeManifest {
    platforms: std::collections::HashMap<String, ClaudePlatform>,
}

#[derive(Debug, Deserialize)]
struct ClaudePlatform {
    binary: String,
    checksum: String,
}

#[derive(Debug, Deserialize)]
struct KimiManifest {
    platforms: std::collections::HashMap<String, KimiPlatform>,
}

#[derive(Debug, Deserialize)]
struct KimiPlatform {
    filename: String,
    checksum: String,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    digest: Option<String>,
}

impl GithubAsset {
    fn sha256_digest(&self) -> Option<String> {
        self.digest
            .as_ref()?
            .strip_prefix("sha256:")
            .map(str::to_string)
    }
}

struct GithubReleaseAssetMatch {
    tag_name: String,
    asset: GithubAsset,
}
