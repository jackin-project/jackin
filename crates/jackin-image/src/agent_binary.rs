//! Download and cache agent CLI binaries (Kimi, etc.) for injection into derived images.
//!
//! Fetches from upstream CDN, verifies SHA-256, caches under `~/.jackin/cache/`
//! with a 1-hour TTL. Not responsible for injecting binaries into the Docker
//! build context — callers in `runtime::image` handle that step.

use crate::binary_artifact::{
    chmod_executable, container_arch, extract_tar_gz_member, hash_file_sha256, is_executable_file,
    parse_sha256_hex,
};
use anyhow::{Context, Result};
use jackin_core::{Agent, JackinPaths};
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const CACHE_TTL: Duration = Duration::from_hours(1);
const KIMI_BASE_URL: &str = "https://cdn.kimi.com/kimi-code";

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
    match resolve_latest_release(agent).await {
        Ok(release) => {
            persist_release_cache(paths, &release);
            Some(release)
        }
        Err(error) => {
            record(
                "warning",
                &format!(
                    "{} latest version lookup failed; using cached metadata if available: {error:#}",
                    agent.slug()
                ),
            );
            newest_cached_executable_release(paths, agent).map(|(_, release, _)| release)
        }
    }
}

pub async fn ensure_available(paths: &JackinPaths, agent: Agent) -> Result<AgentBinary> {
    let stub_path = test_stub_path(paths, agent);
    #[cfg(test)]
    if !is_executable_file(&stub_path) {
        install_test_stub(paths, agent).context("installing in-process agent binary test stub")?;
    }
    if is_executable_file(&stub_path) {
        record(
            "agent_binary_cache_hit",
            &format!("{} test stub at {}", agent.slug(), stub_path.display()),
        );
        return Ok(AgentBinary {
            agent,
            path: stub_path,
        });
    }

    // Metadata cache hit (TTL: 1hr): skip the network resolve. Absence of the
    // preceding `agent_binary_resolve_started` record is what marks this path
    // in the diagnostics run.
    if let Some(cached_release) = read_cached_release(paths, agent) {
        let cached = cached_binary_path(paths, &cached_release);
        return ensure_binary_for_release(agent, &cached_release, &cached).await;
    }

    record(
        "agent_binary_resolve_started",
        &format!("{} latest release", agent.slug()),
    );
    let release = match resolve_latest_release(agent).await {
        Ok(release) => release,
        Err(error) => {
            if let Some((_, fallback_release, fallback_path)) =
                newest_cached_executable_release(paths, agent)
            {
                record(
                    "warning",
                    &format!(
                        "{} latest version lookup failed; using cached {} binary at {}: {error:#}",
                        agent.slug(),
                        fallback_release.version,
                        fallback_path.display()
                    ),
                );
                return ensure_binary_for_release(agent, &fallback_release, &fallback_path).await;
            }
            record(
                "agent_binary_failed",
                &format!("{} resolve failed: {error:#}", agent.slug()),
            );
            return Err(error).with_context(|| format!("resolving latest {} binary", agent.slug()));
        }
    };
    record(
        "agent_binary_resolved",
        &format!("{} {} from {}", agent.slug(), release.version, release.url),
    );
    persist_release_cache(paths, &release);
    let cached = cached_binary_path(paths, &release);
    ensure_binary_for_release(agent, &release, &cached).await
}

/// Return the cached binary if present, otherwise download `release` to
/// `cached` and return it. Shared by the metadata-cache-hit and post-resolve
/// paths so both emit the same breadcrumbs and run the same download sequence.
async fn ensure_binary_for_release(
    agent: Agent,
    release: &AgentRelease,
    cached: &Path,
) -> Result<AgentBinary> {
    if is_executable_file(cached) {
        record(
            "agent_binary_cache_hit",
            &format!(
                "{} {} at {}",
                agent.slug(),
                release.version,
                cached.display()
            ),
        );
        return Ok(AgentBinary {
            agent,
            path: cached.to_path_buf(),
        });
    }
    record(
        "agent_binary_download_started",
        &format!(
            "{} {} from {} to {}",
            agent.slug(),
            release.version,
            release.url,
            cached.display()
        ),
    );
    download_and_cache(release, cached)
        .await
        .with_context(|| {
            format!(
                "downloading {} {} from {}",
                agent.slug(),
                release.version,
                release.url
            )
        })
        .inspect_err(|error| {
            record(
                "agent_binary_failed",
                &format!("{} download failed: {error:#}", agent.slug()),
            );
        })?;
    record(
        "agent_binary_ready",
        &format!(
            "{} {} at {}",
            agent.slug(),
            release.version,
            cached.display()
        ),
    );
    Ok(AgentBinary {
        agent,
        path: cached.to_path_buf(),
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

async fn resolve_latest_release(agent: Agent) -> Result<AgentRelease> {
    // Central resolver exception: each upstream publishes releases through a
    // different mechanism, so this match is the single dispatch point instead
    // of scattering agent-specific release lookup across launch/image code.
    match agent {
        Agent::Claude => resolve_claude().await,
        Agent::Codex => resolve_codex().await,
        Agent::Amp => resolve_amp().await,
        Agent::Kimi => resolve_kimi().await,
        Agent::Opencode => resolve_opencode().await,
        Agent::Grok => resolve_grok().await,
    }
}

async fn resolve_claude() -> Result<AgentRelease> {
    let base = "https://downloads.claude.ai/claude-code-releases";
    let version = fetch_text(&format!("{base}/latest")).await?;
    let version = version.trim().to_owned();
    let platform = platform_x64_arm64();
    let manifest: ClaudeManifest =
        serde_json::from_str(&fetch_text(&format!("{base}/{version}/manifest.json")).await?)?;
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
    let version = fetch_text(&format!("{base}/cli/cli-version.txt"))
        .await?
        .trim()
        .to_owned();
    let platform = match container_arch() {
        "arm64" => "linux-arm64",
        _ => "linux-x64",
    };
    let sha_text = fetch_text(&format!("{base}/cli/{version}/{platform}-amp.sha256")).await?;
    let checksum = parse_sha256_hex(&sha_text)
        .with_context(|| format!("amp published checksum for {version} {platform}"))?;
    Ok(AgentRelease {
        agent: Agent::Amp,
        version: version.clone(),
        url: format!("{base}/cli/{version}/amp-{platform}"),
        checksum: Some(checksum),
        archive_member: None,
    })
}

async fn resolve_kimi() -> Result<AgentRelease> {
    let base = KIMI_BASE_URL;
    let version = fetch_text(&format!("{base}/latest"))
        .await?
        .trim()
        .to_owned();
    let platform = platform_x64_arm64();
    let manifest: KimiManifest =
        serde_json::from_str(&fetch_text(&format!("{base}/{version}/manifest.json")).await?)?;
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
            .to_owned(),
        url: release.asset.browser_download_url,
        checksum,
        archive_member: Some(format!("codex-{arch}")),
    })
}

async fn resolve_grok() -> Result<AgentRelease> {
    // Grok Build binaries are distributed via the official installer
    // (x.ai/cli or GCS fallback) which does platform detection, parallel
    // chunked download, auth.json handling, and PATH/symlink setup.
    // The jackin agent-binary pre-bake path (used by some `jackin load`
    // flows and derived image construction) does not yet have a stable
    // "latest + manifest" URL surface for Grok equivalent to the other
    // agents. Roles should install via the documented curl | bash inside
    // their Dockerfile (or vendor a pinned linux binary) until this path
    // is implemented.
    anyhow::bail!(
        "Grok agent binary provisioning via `jackin` is not yet supported; \
         install the CLI inside your role with `curl -fsSL https://x.ai/cli/install.sh | bash` \
         (or copy a pre-built linux binary into the image and ensure `grok` is on PATH). \
         Runtime launch + auth forwarding for Grok are fully supported once the binary is present."
    )
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
        version: release.tag_name.trim_start_matches('v').to_owned(),
        url: release.asset.browser_download_url,
        checksum,
        archive_member: Some("opencode".to_owned()),
    })
}

async fn fetch_text(url: &str) -> Result<String> {
    record("agent_binary_http_get", url);
    jackin_docker::net::fetch_text(url).await
}

async fn github_auth_token() -> Option<String> {
    // Degrade to unauthenticated (60 req/hr) on any failure, but log which one:
    // `gh` missing is expected on CI, while present-but-erroring (not logged in)
    // is the case an operator hitting a rate-limit 403 needs to see in --debug.
    match tokio::process::Command::new("gh")
        .args(["auth", "token", "--hostname", "github.com"])
        .output()
        .await
    {
        Ok(output) if output.status.success() => {
            let token = String::from_utf8(output.stdout).ok()?.trim().to_owned();
            (!token.is_empty()).then_some(token)
        }
        Ok(output) => {
            jackin_diagnostics::debug_log!(
                "agent_binary",
                "gh auth token exited {}: {} — proceeding unauthenticated",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
            None
        }
        Err(e) => {
            jackin_diagnostics::debug_log!(
                "agent_binary",
                "gh auth token not runnable ({e}) — proceeding unauthenticated"
            );
            None
        }
    }
}

async fn github_latest_asset(repo: &str, asset_name: &str) -> Result<GithubReleaseAssetMatch> {
    let api_url = format!("https://api.github.com/repos/{repo}/releases/latest");
    // Authenticated requests have 5 000 req/hr vs 60 req/hr unauthenticated.
    let token = github_auth_token().await;
    let mut headers = HeaderMap::new();
    if let Some(ref t) = token {
        let val = reqwest::header::HeaderValue::from_str(&format!("Bearer {t}"))
            .context("building Authorization header from gh token")?;
        headers.insert(reqwest::header::AUTHORIZATION, val);
    }
    let client = jackin_docker::net::http_client(headers)?;
    let body = retry_with_backoff(3, Duration::from_millis(500), || {
        let c = client.clone();
        let u = api_url.clone();
        async move {
            record("agent_binary_http_get", &u);
            jackin_docker::net::get_text(&c, &u).await
        }
    })
    .await
    .with_context(|| format!("fetching latest GitHub release metadata for {repo}"))?;
    let release: GithubRelease = serde_json::from_str(&body)
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

async fn retry_with_backoff<T, F, Fut>(
    max_attempts: u32,
    initial_delay: Duration,
    f: F,
) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut last_err = anyhow::anyhow!("no attempts made");
    for attempt in 0..max_attempts {
        if attempt > 0 {
            let delay = initial_delay * (1 << (attempt - 1));
            record(
                "retry_backoff",
                &format!("attempt {attempt}/{max_attempts}, waiting {delay:?}"),
            );
            tokio::time::sleep(delay).await;
        }
        match f().await {
            Ok(v) => return Ok(v),
            Err(e) => {
                record(
                    "retry_failed",
                    &format!("attempt {}/{max_attempts}: {e:#}", attempt + 1),
                );
                last_err = e;
            }
        }
    }
    Err(last_err).with_context(|| format!("giving up after {max_attempts} attempts"))
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
    drop(std::fs::remove_file(&tmp_download));
    drop(std::fs::remove_file(&tmp_binary));
    let result = download_and_cache_inner(release, dest, &tmp_download, &tmp_binary).await;
    if result.is_err() {
        drop(std::fs::remove_file(&tmp_download));
        drop(std::fs::remove_file(&tmp_binary));
    }
    result
}

async fn download_and_cache_inner(
    release: &AgentRelease,
    dest: &Path,
    tmp_download: &Path,
    tmp_binary: &Path,
) -> Result<()> {
    jackin_docker::net::download_parallel(&release.url, tmp_download).await?;
    // A dropped chunk leaves a zeroed hole in the pre-sized file rather than a
    // short file, so the SHA-256 is the only integrity guard — require it. Every
    // resolver populates a checksum (claude/kimi/amp from their manifests,
    // codex/opencode from the GitHub asset digest); a missing one means an
    // unverifiable binary we refuse to install rather than exec blind.
    let expected = release.checksum.as_deref().with_context(|| {
        format!(
            "{} release {} has no published checksum; refusing to install an unverified binary",
            release.agent.slug(),
            release.version
        )
    })?;
    let tmp_for_hash = tmp_download.to_owned();
    let actual = tokio::task::spawn_blocking(move || hash_file_sha256(&tmp_for_hash))
        .await
        .context("hash worker join")?
        .with_context(|| format!("hashing {}", tmp_download.display()))?;
    anyhow::ensure!(
        actual.eq_ignore_ascii_case(expected),
        "{} checksum mismatch for {}\n  expected {}\n  actual   {}",
        release.agent.slug(),
        release.url,
        expected,
        actual
    );
    if let Some(member) = &release.archive_member {
        extract_tar_gz_member(tmp_download, member, tmp_binary)?;
        drop(std::fs::remove_file(tmp_download));
    } else {
        std::fs::rename(tmp_download, tmp_binary)?;
    }
    chmod_executable(tmp_binary)?;
    std::fs::rename(tmp_binary, dest)?;
    Ok(())
}

fn record(kind: &str, message: &str) {
    if let Some(run) = jackin_diagnostics::active_run() {
        run.compact(kind, message);
    } else {
        jackin_diagnostics::debug_log!("agent_binary", "{kind}: {message}");
    }
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

fn read_release_file(path: &Path) -> Option<AgentRelease> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn read_cached_release(paths: &JackinPaths, agent: Agent) -> Option<AgentRelease> {
    let path = metadata_cache_path(paths, agent);
    let metadata = std::fs::metadata(&path).ok()?;
    let modified = metadata.modified().ok()?;
    if SystemTime::now().duration_since(modified).ok()? >= CACHE_TTL {
        return None;
    }
    read_release_file(&path)
}

fn newest_cached_executable_release(
    paths: &JackinPaths,
    agent: Agent,
) -> Option<(SystemTime, AgentRelease, PathBuf)> {
    let root = paths.cache_dir.join("agent-binaries").join(agent.slug());
    let arch_dir_name = format!("linux-{}", container_arch());
    let mut candidates = Vec::new();
    for version_entry in std::fs::read_dir(root).ok()?.flatten() {
        let metadata_path = version_entry
            .path()
            .join(&arch_dir_name)
            .join("metadata.json");
        let Some(release) = read_release_file(&metadata_path).filter(|release| {
            release.agent == agent && is_executable_file(&cached_binary_path(paths, release))
        }) else {
            continue;
        };
        let binary_path = cached_binary_path(paths, &release);
        let modified = std::fs::metadata(&binary_path)
            .and_then(|metadata| metadata.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        candidates.push((modified, release, binary_path));
    }
    candidates.sort_by_key(|(modified, _, _)| std::cmp::Reverse(*modified));
    candidates.into_iter().next()
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

/// Best-effort write of the resolved release to the TTL metadata cache and the
/// per-version sidecar. A failure only costs an extra network resolve next
/// launch, so it's logged (to explain a re-resolve loop under `--debug`) rather
/// than propagated.
fn persist_release_cache(paths: &JackinPaths, release: &AgentRelease) {
    let slug = release.agent.slug();
    if let Err(e) = write_cached_release(paths, release) {
        jackin_diagnostics::debug_log!(
            "agent_binary",
            "caching {slug} release metadata failed: {e:#}"
        );
    }
    if let Err(e) = write_version_release(paths, release) {
        jackin_diagnostics::debug_log!(
            "agent_binary",
            "writing {slug} version sidecar failed: {e:#}"
        );
    }
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
            .map(str::to_owned)
    }
}

struct GithubReleaseAssetMatch {
    tag_name: String,
    asset: GithubAsset,
}

#[cfg(test)]
mod tests;
