use crate::agent::Agent;
use crate::binary_artifact::{
    chmod_executable, container_arch, extract_tar_gz_member, hash_file_sha256, is_executable_file,
    parse_sha256_hex,
};
use crate::paths::JackinPaths;
use anyhow::{Context, Result};
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const CACHE_TTL: std::time::Duration = std::time::Duration::from_hours(1);
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
    let version = fetch_text(&format!("{base}/latest")).await?;
    let version = version.trim().to_string();
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
        .to_string();
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
        .to_string();
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

async fn fetch_text(url: &str) -> Result<String> {
    record("agent_binary_http_get", url);
    crate::net::fetch_text(url).await
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
            let token = String::from_utf8(output.stdout).ok()?.trim().to_string();
            (!token.is_empty()).then_some(token)
        }
        Ok(output) => {
            crate::debug_log!(
                "agent_binary",
                "gh auth token exited {}: {} — proceeding unauthenticated",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
            None
        }
        Err(e) => {
            crate::debug_log!(
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
    let client = crate::net::http_client(headers)?;
    let body = retry_with_backoff(3, Duration::from_millis(500), || {
        let c = client.clone();
        let u = api_url.clone();
        async move {
            record("agent_binary_http_get", &u);
            crate::net::get_text(&c, &u).await
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
    Fut: std::future::Future<Output = Result<T>>,
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
    let _ = std::fs::remove_file(&tmp_download);
    let _ = std::fs::remove_file(&tmp_binary);
    let result = download_and_cache_inner(release, dest, &tmp_download, &tmp_binary).await;
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp_download);
        let _ = std::fs::remove_file(&tmp_binary);
    }
    result
}

async fn download_and_cache_inner(
    release: &AgentRelease,
    dest: &Path,
    tmp_download: &Path,
    tmp_binary: &Path,
) -> Result<()> {
    crate::net::download_parallel(&release.url, tmp_download).await?;
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
        let _ = std::fs::remove_file(tmp_download);
    } else {
        std::fs::rename(tmp_download, tmp_binary)?;
    }
    chmod_executable(tmp_binary)?;
    std::fs::rename(tmp_binary, dest)?;
    Ok(())
}

fn record(kind: &str, message: &str) {
    if let Some(run) = crate::diagnostics::active_run() {
        run.compact(kind, message);
    } else {
        crate::debug_log!("agent_binary", "{kind}: {message}");
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
    if std::time::SystemTime::now().duration_since(modified).ok()? >= CACHE_TTL {
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
        crate::debug_log!(
            "agent_binary",
            "caching {slug} release metadata failed: {e:#}"
        );
    }
    if let Err(e) = write_version_release(paths, release) {
        crate::debug_log!(
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
            .map(str::to_string)
    }
}

struct GithubReleaseAssetMatch {
    tag_name: String,
    asset: GithubAsset,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[tokio::test(start_paused = true)]
    async fn retry_succeeds_on_first_try() {
        let calls = Cell::new(0u32);
        let r: Result<u32> = retry_with_backoff(3, Duration::from_millis(10), || {
            calls.set(calls.get() + 1);
            async { Ok(42) }
        })
        .await;
        assert_eq!(r.unwrap(), 42);
        assert_eq!(calls.get(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn retry_recovers_after_transient_failures() {
        let calls = Cell::new(0u32);
        let r: Result<u32> = retry_with_backoff(3, Duration::from_millis(10), || {
            let n = calls.get() + 1;
            calls.set(n);
            async move {
                if n < 3 {
                    anyhow::bail!("transient {n}")
                }
                Ok(n)
            }
        })
        .await;
        assert_eq!(r.unwrap(), 3);
        assert_eq!(calls.get(), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn retry_exhausts_and_returns_last_error() {
        let calls = Cell::new(0u32);
        let r: Result<()> = retry_with_backoff(3, Duration::from_millis(10), || {
            let n = calls.get() + 1;
            calls.set(n);
            async move { anyhow::bail!("attempt {n} failed") }
        })
        .await;
        assert_eq!(calls.get(), 3);
        // Chain carries the attempt count and preserves the LAST attempt's
        // error (not the "no attempts made" seed).
        let err = format!("{:#}", r.unwrap_err());
        assert!(err.contains("giving up after 3 attempts"), "{err}");
        assert!(err.contains("attempt 3 failed"), "{err}");
    }

    #[tokio::test(start_paused = true)]
    async fn retry_with_zero_attempts_never_calls_closure() {
        let calls = Cell::new(0u32);
        let r: Result<()> = retry_with_backoff(0, Duration::from_millis(10), || {
            calls.set(calls.get() + 1);
            async { Ok(()) }
        })
        .await;
        assert!(r.is_err());
        assert_eq!(calls.get(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn retry_backoff_grows_exponentially() {
        let start = tokio::time::Instant::now();
        let _: Result<()> = retry_with_backoff(3, Duration::from_millis(100), || async {
            anyhow::bail!("nope")
        })
        .await;
        // Attempt 1 is immediate; attempts 2 and 3 wait 100ms then 200ms.
        assert_eq!(start.elapsed(), Duration::from_millis(300));
    }

    fn release_fixture() -> AgentRelease {
        release_fixture_for(Agent::Claude, "1.2.3")
    }

    fn release_fixture_for(agent: Agent, version: &str) -> AgentRelease {
        AgentRelease {
            agent,
            version: version.to_string(),
            url: format!("https://example.test/{}", agent.slug()),
            checksum: Some("abc".to_string()),
            archive_member: None,
        }
    }

    #[test]
    fn read_cached_release_missing_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(dir.path());
        assert!(read_cached_release(&paths, Agent::Claude).is_none());
    }

    #[test]
    fn read_cached_release_fresh_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(dir.path());
        let release = release_fixture();
        write_cached_release(&paths, &release).unwrap();
        let got = read_cached_release(&paths, Agent::Claude).expect("fresh cache should hit");
        assert_eq!(got.version, release.version);
        assert_eq!(got.url, release.url);
    }

    #[test]
    fn read_cached_release_past_ttl_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(dir.path());
        write_cached_release(&paths, &release_fixture()).unwrap();
        let path = metadata_cache_path(&paths, Agent::Claude);
        let stale = std::time::SystemTime::now() - Duration::from_hours(2);
        filetime::set_file_mtime(&path, filetime::FileTime::from_system_time(stale)).unwrap();
        assert!(read_cached_release(&paths, Agent::Claude).is_none());
    }

    #[test]
    fn newest_cached_executable_release_reads_stale_version_sidecars() {
        let dir = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(dir.path());
        let older = release_fixture();
        let newer = AgentRelease {
            version: "1.2.4".to_string(),
            url: "https://example.test/claude-newer".to_string(),
            ..release_fixture()
        };

        write_version_release(&paths, &older).unwrap();
        write_version_release(&paths, &newer).unwrap();
        let older_binary = cached_binary_path(&paths, &older);
        let newer_binary = cached_binary_path(&paths, &newer);
        std::fs::write(&older_binary, b"older").unwrap();
        std::fs::write(&newer_binary, b"newer").unwrap();
        chmod_executable(&older_binary).unwrap();
        chmod_executable(&newer_binary).unwrap();
        filetime::set_file_mtime(
            &older_binary,
            filetime::FileTime::from_system_time(SystemTime::now() - Duration::from_mins(1)),
        )
        .unwrap();

        let (_, release, path) =
            newest_cached_executable_release(&paths, Agent::Claude).expect("cached fallback");
        assert_eq!(release.version, newer.version);
        assert_eq!(path, newer_binary);
    }

    #[test]
    fn newest_cached_executable_release_works_for_every_agent() {
        let dir = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(dir.path());

        for &agent in Agent::ALL {
            let release = release_fixture_for(agent, "9.9.9");
            write_version_release(&paths, &release).unwrap();
            let binary = cached_binary_path(&paths, &release);
            std::fs::write(&binary, agent.slug()).unwrap();
            chmod_executable(&binary).unwrap();

            let (_, got, path) =
                newest_cached_executable_release(&paths, agent).expect("cached fallback");
            assert_eq!(got.agent, agent);
            assert_eq!(got.version, release.version);
            assert_eq!(path, binary);
        }
    }

    #[test]
    fn newest_cached_executable_release_ignores_non_executable_sidecars() {
        let dir = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(dir.path());
        let release = release_fixture();
        write_version_release(&paths, &release).unwrap();
        std::fs::write(cached_binary_path(&paths, &release), b"not executable").unwrap();

        assert!(newest_cached_executable_release(&paths, Agent::Claude).is_none());
    }

    #[test]
    fn kimi_resolver_uses_cdn_urls() {
        assert_eq!(KIMI_BASE_URL, "https://cdn.kimi.com/kimi-code");
    }

    #[test]
    fn read_cached_release_malformed_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(dir.path());
        let path = metadata_cache_path(&paths, Agent::Claude);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"{ not valid json").unwrap();
        assert!(read_cached_release(&paths, Agent::Claude).is_none());
    }

    #[test]
    fn sha256_digest_strips_prefix_only_for_sha256() {
        let asset = |digest: Option<&str>| GithubAsset {
            name: "asset".to_string(),
            browser_download_url: "https://example.test/a".to_string(),
            digest: digest.map(str::to_string),
        };
        assert_eq!(
            asset(Some("sha256:deadbeef")).sha256_digest().as_deref(),
            Some("deadbeef")
        );
        assert!(asset(Some("md5:deadbeef")).sha256_digest().is_none());
        assert!(asset(None).sha256_digest().is_none());
    }
}
