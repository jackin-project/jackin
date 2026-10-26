// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::ImageError;
use crate::binary_artifact::{
    chmod_executable, container_arch, extract_tar_gz_member, hash_file_sha256, hash_file_sha512,
    is_executable_file, parse_sha256_hex, parse_sha512_hex, repair_executable_file,
};
use anyhow::{Context, Result};
use jackin_core::{Agent, Clock, JackinPaths, SystemClock};
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tokio::sync::OnceCell;

const CACHE_TTL: Duration = Duration::from_hours(1);
const KIMI_DOWNLOAD_BASE_URL: &str = "https://code.kimi.com/kimi-code";
const KIMI_BINARY_BASE_URL: &str = "https://code.kimi.com/kimi-code/binaries";

const GROK_BASE_PRIMARY: &str = "https://x.ai/cli";
const GROK_BASE_FALLBACK: &str = "https://storage.googleapis.com/grok-build-public-artifacts/cli";

static GITHUB_AUTH_TOKEN: OnceCell<Option<String>> = OnceCell::const_new();

fn record_agent_binary_failure(body: &'static str) {
    let _error = jackin_telemetry::record_error(
        jackin_telemetry::schema::enums::ErrorType::CapsuleDownloadFailed,
    );
    jackin_diagnostics::emit_operator_notice(body);
}

#[derive(Debug, Clone)]
pub struct AgentBinary {
    pub agent: Agent,
    pub path: PathBuf,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRelease {
    pub agent: Agent,
    pub version: String,
    pub url: String,
    pub checksum: Option<String>,
    pub archive_member: Option<String>,
    #[serde(default)]
    pub extra_files: Vec<AgentExtraFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentExtraFile {
    pub name: String,
    pub url: String,
    pub checksum: Option<String>,
    pub archive_member: Option<String>,
}

fn release_has_required_extras(release: &AgentRelease) -> bool {
    if release.agent != Agent::Codex {
        return true;
    }
    !release.extra_files.is_empty()
}

pub async fn latest_release(paths: &JackinPaths, agent: Agent) -> Option<AgentRelease> {
    if let Some(cached) = read_cached_release_async(paths, agent).await {
        return Some(cached);
    }
    match resolve_latest_release(agent).await {
        Ok(release) => {
            persist_release_cache_async(paths, &release).await;
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
            newest_cached_executable_release_async(paths, agent)
                .await
                .map(|(_, release, _)| release)
        }
    }
}

pub async fn ensure_available(paths: &JackinPaths, agent: Agent) -> Result<AgentBinary> {
    ensure_available_impl(paths, agent, true).await
}

async fn ensure_available_impl(
    paths: &JackinPaths,
    agent: Agent,
    install_stub: bool,
) -> Result<AgentBinary> {
    let _ = install_stub;
    let stub_path = test_stub_path(paths, agent);

    if is_executable_file_async(&stub_path).await {
        crate::telemetry_boundary::cache_decision(
            jackin_telemetry::schema::enums::CacheName::AgentBinary,
            jackin_telemetry::schema::enums::CacheResult::Hit,
        );
        record(
            "agent_binary_cache_hit",
            &format!("{} test stub at {}", agent.slug(), stub_path.display()),
        );
        return Ok(AgentBinary {
            agent,
            path: stub_path,
            version: None,
        });
    }

    if let Some(cached_release) = read_cached_release_async(paths, agent).await {
        crate::telemetry_boundary::cache_decision(
            jackin_telemetry::schema::enums::CacheName::AgentBinary,
            jackin_telemetry::schema::enums::CacheResult::Hit,
        );
        let cached = cached_binary_path(paths, &cached_release);
        return ensure_binary_or_cached_fallback(
            paths,
            agent,
            &cached_release,
            &cached,
            "cached release download failed",
        )
        .await
        .with_context(|| format!("preparing cached {} binary", agent.slug()));
    }

    if let Some((_, fallback_release, fallback_path)) =
        newest_cached_executable_release_async(paths, agent).await
    {
        crate::telemetry_boundary::cache_decision(
            jackin_telemetry::schema::enums::CacheName::AgentBinary,
            jackin_telemetry::schema::enums::CacheResult::Stale,
        );
        spawn_release_metadata_refresh(paths.clone(), agent);
        return ensure_binary_for_release(agent, &fallback_release, &fallback_path).await;
    }

    record(
        "agent_binary_resolve_started",
        &format!("{} latest release", agent.slug()),
    );
    let release = match resolve_latest_release(agent).await {
        Ok(release) => release,
        Err(error) => {
            if let Some((fallback_release, fallback_path)) = cached_executable_after_failure_async(
                paths,
                agent,
                &error,
                "latest version lookup failed",
            )
            .await
            {
                return ensure_binary_for_release(agent, &fallback_release, &fallback_path).await;
            }
            record(
                "agent_binary_failed",
                &format!("{} resolve failed: {error:#}", agent.slug()),
            );
            record_agent_binary_failure("agent binary release resolution failed");
            return Err(error).with_context(|| format!("resolving latest {} binary", agent.slug()));
        }
    };
    record(
        "agent_binary_resolved",
        &format!("{} {} from {}", agent.slug(), release.version, release.url),
    );
    persist_release_cache_async(paths, &release).await;
    let cached = cached_binary_path(paths, &release);
    ensure_binary_or_cached_fallback(
        paths,
        agent,
        &release,
        &cached,
        "latest binary download failed",
    )
    .await
}

fn spawn_release_metadata_refresh(paths: JackinPaths, agent: Agent) {
    jackin_telemetry::spawn::spawn_prewarm_job(
        jackin_telemetry::schema::enums::JobType::ImagePrewarm,
        async move {
            record(
                "agent_binary_resolve_started",
                &format!("{} latest release background", agent.slug()),
            );
            match resolve_latest_release(agent).await {
                Ok(release) => {
                    record(
                        "agent_binary_resolved",
                        &format!(
                            "{} {} from {} background",
                            agent.slug(),
                            release.version,
                            release.url
                        ),
                    );
                    persist_release_cache_async(&paths, &release).await;
                    jackin_telemetry::spawn::DetachedCompletion::success()
                }
                Err(error) => {
                    record(
                        "warning",
                        &format!(
                            "{} background latest version lookup failed: {error:#}",
                            agent.slug()
                        ),
                    );
                    jackin_telemetry::spawn::DetachedCompletion::failure(
                        jackin_telemetry::schema::enums::ErrorType::HttpError,
                    )
                }
            }
        },
        |completion| *completion,
    );
}

async fn ensure_binary_or_cached_fallback(
    paths: &JackinPaths,
    agent: Agent,
    release: &AgentRelease,
    cached: &Path,
    failure: &str,
) -> Result<AgentBinary> {
    match ensure_binary_for_release(agent, release, cached).await {
        Ok(binary) => Ok(binary),
        Err(error) => {
            if let Some((fallback_release, fallback_path)) =
                cached_executable_after_failure_async(paths, agent, &error, failure).await
            {
                return ensure_binary_for_release(agent, &fallback_release, &fallback_path).await;
            }
            Err(error)
        }
    }
}

async fn cached_executable_after_failure_async(
    paths: &JackinPaths,
    agent: Agent,
    error: &anyhow::Error,
    failure: &str,
) -> Option<(AgentRelease, PathBuf)> {
    let (_, fallback_release, fallback_path) =
        newest_cached_executable_release_async(paths, agent).await?;
    record(
        "warning",
        &format!(
            "{} {failure}; using cached {} binary at {}: {error:#}",
            agent.slug(),
            fallback_release.version,
            fallback_path.display()
        ),
    );
    Some((fallback_release, fallback_path))
}

async fn ensure_extra_files(release: &AgentRelease, cached: &Path) -> Result<()> {
    for extra in &release.extra_files {
        let dest = cached.with_file_name(&extra.name);
        if is_executable_file_async(&dest).await {
            continue;
        }
        let sub_release = AgentRelease {
            agent: release.agent,
            version: release.version.clone(),
            url: extra.url.clone(),
            checksum: extra.checksum.clone(),
            archive_member: extra.archive_member.clone(),
            extra_files: Vec::new(),
        };
        record(
            "agent_binary_download_started",
            &format!(
                "{} {} sibling {} from {} to {}",
                release.agent.slug(),
                release.version,
                extra.name,
                extra.url,
                dest.display()
            ),
        );
        download_and_cache(&sub_release, &dest)
            .await
            .with_context(|| {
                format!(
                    "downloading {} {} sibling {} from {}",
                    release.agent.slug(),
                    release.version,
                    extra.name,
                    extra.url
                )
            })?;
        record(
            "agent_binary_ready",
            &format!(
                "{} {} sibling {} at {}",
                release.agent.slug(),
                release.version,
                extra.name,
                dest.display()
            ),
        );
    }
    Ok(())
}

async fn ensure_binary_for_release(
    agent: Agent,
    release: &AgentRelease,
    cached: &Path,
) -> Result<AgentBinary> {
    if is_executable_file_async(cached).await {
        crate::telemetry_boundary::cache_decision(
            jackin_telemetry::schema::enums::CacheName::AgentBinary,
            jackin_telemetry::schema::enums::CacheResult::Hit,
        );
        record(
            "agent_binary_cache_hit",
            &format!(
                "{} {} at {}",
                agent.slug(),
                release.version,
                cached.display()
            ),
        );
        ensure_extra_files(release, cached).await?;
        return Ok(AgentBinary {
            agent,
            path: cached.to_path_buf(),
            version: Some(release.version.clone()),
        });
    }
    if repair_cached_binary_mode_async(cached).await? {
        crate::telemetry_boundary::cache_decision(
            jackin_telemetry::schema::enums::CacheName::AgentBinary,
            jackin_telemetry::schema::enums::CacheResult::Reuse,
        );
        record(
            "agent_binary_cache_repaired",
            &format!(
                "{} {} at {}",
                agent.slug(),
                release.version,
                cached.display()
            ),
        );
        ensure_extra_files(release, cached).await?;
        return Ok(AgentBinary {
            agent,
            path: cached.to_path_buf(),
            version: Some(release.version.clone()),
        });
    }
    crate::telemetry_boundary::cache_decision(
        jackin_telemetry::schema::enums::CacheName::AgentBinary,
        jackin_telemetry::schema::enums::CacheResult::Miss,
    );
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
            record_agent_binary_failure("agent binary download failed");
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
    ensure_extra_files(release, cached).await?;
    Ok(AgentBinary {
        agent,
        path: cached.to_path_buf(),
        version: Some(release.version.clone()),
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
        Agent::Grok => resolve_grok().await,
        Agent::Antigravity => resolve_antigravity().await,
        Agent::Gemini => resolve_npm_only(Agent::Gemini, "@google/gemini-cli"),
        Agent::Cursor => resolve_no_binary(
            Agent::Cursor,
            "the official installer lays down a versioned directory plus wrapper, not a relocatable binary",
        ),
        Agent::Muse => resolve_no_installer(Agent::Muse),
        Agent::Omp => resolve_npm_only(Agent::Omp, "@oh-my-pi/pi-coding-agent"),
        Agent::Hermes => resolve_no_binary(
            Agent::Hermes,
            "the official installer lays down a Python/uv environment, not a relocatable binary",
        ),
    }
}

#[derive(Debug, Deserialize)]
struct AntigravityManifest {
    version: String,
    url: String,
    sha512: String,
}

async fn resolve_antigravity() -> Result<AgentRelease> {
    const BASE: &str = "https://antigravity-cli-auto-updater-974169037036.us-central1.run.app";
    let platform = match container_arch() {
        "arm64" => "linux_arm64",
        _ => "linux_amd64",
    };
    let manifest: AntigravityManifest = serde_json::from_str(
        &fetch_text_with_retry(&format!("{BASE}/manifests/{platform}.json")).await?,
    )
    .context("parsing Antigravity platform manifest")?;
    if manifest.version.trim().is_empty() || manifest.url.trim().is_empty() {
        return Err(ImageError::msg(format!(
            "Antigravity manifest for {platform} is missing version/url"
        ))
        .into());
    }
    let checksum = parse_sha512_hex(&manifest.sha512)
        .with_context(|| format!("Antigravity published checksum for {}", manifest.version))?;
    Ok(AgentRelease {
        agent: Agent::Antigravity,
        version: manifest.version,
        url: manifest.url,
        checksum: Some(checksum),
        archive_member: Some("antigravity".to_owned()),
        extra_files: Vec::new(),
    })
}

fn resolve_npm_only(agent: Agent, package: &str) -> Result<AgentRelease> {
    Err(ImageError::msg(format!(
        "{} is distributed via npm ({package}); host binary prefetch is not applicable",
        agent.slug(),
    ))
    .into())
}

fn resolve_no_installer(agent: Agent) -> Result<AgentRelease> {
    Err(ImageError::msg(format!(
        "{} has no verified standalone installer; install it on the host",
        agent.slug(),
    ))
    .into())
}

fn resolve_no_binary(agent: Agent, reason: &str) -> Result<AgentRelease> {
    Err(ImageError::msg(format!(
        "{} host binary prefetch is not applicable: {reason}",
        agent.slug(),
    ))
    .into())
}

async fn resolve_claude() -> Result<AgentRelease> {
    let base = "https://downloads.claude.ai/claude-code-releases";
    let version = fetch_text_with_retry(&format!("{base}/latest")).await?;
    let version = version.trim().to_owned();
    let platform = platform_x64_arm64();
    let manifest: ClaudeManifest = serde_json::from_str(
        &fetch_text_with_retry(&format!("{base}/{version}/manifest.json")).await?,
    )?;
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
        extra_files: Vec::new(),
    })
}

async fn resolve_amp() -> Result<AgentRelease> {
    let base = "https://static.ampcode.com";
    let version = fetch_text_with_retry(&format!("{base}/cli/cli-version.txt"))
        .await?
        .trim()
        .to_owned();
    let platform = match container_arch() {
        "arm64" => "linux-arm64",
        _ => "linux-x64",
    };
    let sha_text =
        fetch_text_with_retry(&format!("{base}/cli/{version}/{platform}-amp.sha256")).await?;
    let checksum = parse_sha256_hex(&sha_text)
        .with_context(|| format!("amp published checksum for {version} {platform}"))?;
    Ok(AgentRelease {
        agent: Agent::Amp,
        version: version.clone(),
        url: format!("{base}/cli/{version}/amp-{platform}"),
        checksum: Some(checksum),
        archive_member: None,
        extra_files: Vec::new(),
    })
}

async fn resolve_kimi() -> Result<AgentRelease> {
    let version = fetch_text_with_retry(&format!("{KIMI_DOWNLOAD_BASE_URL}/latest"))
        .await?
        .trim()
        .to_owned();
    let platform = platform_x64_arm64();
    let manifest: KimiManifest = serde_json::from_str(
        &fetch_text_with_retry(&format!("{KIMI_BINARY_BASE_URL}/{version}/manifest.json")).await?,
    )?;
    let entry = manifest
        .platforms
        .get(platform)
        .with_context(|| format!("Kimi manifest missing platform {platform}"))?;
    Ok(AgentRelease {
        agent: Agent::Kimi,
        version: version.clone(),
        url: format!("{KIMI_BINARY_BASE_URL}/{version}/{}", entry.filename),
        checksum: Some(entry.checksum.clone()),
        archive_member: None,
        extra_files: Vec::new(),
    })
}

async fn resolve_codex() -> Result<AgentRelease> {
    let arch = match container_arch() {
        "arm64" => "aarch64-unknown-linux-musl",
        _ => "x86_64-unknown-linux-musl",
    };
    let release = github_release("openai/codex").await?;
    let asset = release
        .asset_named(&format!("codex-{arch}.tar.gz"))
        .with_context(|| {
            format!("openai/codex latest release missing asset codex-{arch}.tar.gz")
        })?;
    let host_asset_name = format!("{}-{arch}.tar.gz", jackin_core::CODEX_CODE_MODE_HOST_BINARY);
    let host_asset = release
        .asset_named(&host_asset_name)
        .with_context(|| format!("openai/codex latest release missing asset {host_asset_name}"))?;
    let host_member = format!("{}-{arch}", jackin_core::CODEX_CODE_MODE_HOST_BINARY);
    Ok(AgentRelease {
        agent: Agent::Codex,
        version: release
            .tag_name
            .trim_start_matches("rust-v")
            .trim_start_matches('v')
            .to_owned(),
        url: asset.browser_download_url.clone(),
        checksum: asset.sha256_digest(),
        archive_member: Some(format!("codex-{arch}")),
        extra_files: vec![AgentExtraFile {
            name: jackin_core::CODEX_CODE_MODE_HOST_BINARY.to_owned(),
            url: host_asset.browser_download_url.clone(),
            checksum: host_asset.sha256_digest(),
            archive_member: Some(host_member),
        }],
    })
}

async fn resolve_grok() -> Result<AgentRelease> {
    let primary = GROK_BASE_PRIMARY;
    let fallback = GROK_BASE_FALLBACK;

    let (base, version) =
        if let Ok(text) = fetch_text_with_retry(&format!("{primary}/stable")).await {
            let v = text.trim().to_owned();
            if v.is_empty() {
                let v = fetch_text_with_retry(&format!("{fallback}/stable"))
                    .await?
                    .trim()
                    .to_owned();
                (fallback.to_owned(), v)
            } else {
                (primary.to_owned(), v)
            }
        } else {
            let v = fetch_text_with_retry(&format!("{fallback}/stable"))
                .await?
                .trim()
                .to_owned();
            (fallback.to_owned(), v)
        };

    if version.is_empty() {
        return Err(ImageError::msg(format!(
            "failed to fetch Grok version pointer from {base}/stable"
        ))
        .into());
    }

    let grok_arch = match container_arch() {
        "arm64" => "aarch64",
        _ => "x86_64",
    };
    let platform = format!("linux-{grok_arch}");
    let url = format!("{base}/grok-{version}-{platform}");

    Ok(AgentRelease {
        agent: Agent::Grok,
        version,
        url,
        checksum: None,
        archive_member: None,
        extra_files: Vec::new(),
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
        version: release.tag_name.trim_start_matches('v').to_owned(),
        url: release.asset.browser_download_url,
        checksum,
        archive_member: Some("opencode".to_owned()),
        extra_files: Vec::new(),
    })
}

async fn fetch_text(url: &str) -> Result<String> {
    record("agent_binary_http_get", url);
    crate::telemetry_boundary::download_request(
        crate::telemetry_boundary::DownloadRoute::AgentMetadata,
        url,
        jackin_docker::net::fetch_text(url),
    )
    .await
}

fn is_connect_timeout(error: &anyhow::Error) -> bool {
    error.chain().any(|e| {
        e.downcast_ref::<reqwest::Error>()
            .is_some_and(|re| re.is_connect() && re.is_timeout())
    })
}

async fn fetch_text_with_retry(url: &str) -> Result<String> {
    let url = url.to_owned();
    retry_metadata_with_backoff(2, Duration::from_millis(500), || {
        let url = url.clone();
        async move { fetch_text(&url).await }
    })
    .await
}

async fn github_auth_token() -> Option<String> {
    GITHUB_AUTH_TOKEN
        .get_or_init(github_auth_token_uncached)
        .await
        .clone()
}

async fn github_auth_token_uncached() -> Option<String> {
    let request =
        jackin_process::ExecRequest::new("gh", ["auth", "token", "--hostname", "github.com"])
            .timeout(Duration::from_secs(15));
    match crate::process_telemetry::exec_async(
        &request,
        jackin_telemetry::schema::enums::ProcessExecutableName::Gh,
    )
    .await
    {
        Ok(output) if output.success => {
            let token = String::from_utf8(output.stdout).ok()?.trim().to_owned();
            (!token.is_empty()).then_some(token)
        }
        Ok(_) | Err(_) => None,
    }
}

async fn github_latest_asset(repo: &str, asset_name: &str) -> Result<GithubReleaseAssetMatch> {
    let release = github_release(repo).await?;
    let asset = release
        .asset_named(asset_name)
        .with_context(|| format!("{repo} latest release missing asset {asset_name}"))?;
    Ok(GithubReleaseAssetMatch {
        tag_name: release.tag_name,
        asset,
    })
}

async fn github_release(repo: &str) -> Result<GithubRelease> {
    let api_url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let token = github_auth_token().await;
    let mut headers = HeaderMap::new();
    if let Some(ref t) = token {
        let val = reqwest::header::HeaderValue::from_str(&format!("Bearer {t}"))
            .context("building Authorization header from gh token")?;
        headers.insert(reqwest::header::AUTHORIZATION, val);
    }
    let client = jackin_docker::net::http_client(headers)?;
    let body = retry_metadata_with_backoff(2, Duration::from_millis(500), || {
        let c = client.clone();
        let u = api_url.clone();
        async move {
            record("agent_binary_http_get", &u);
            let telemetry_url = u.clone();
            crate::telemetry_boundary::download_request(
                crate::telemetry_boundary::DownloadRoute::AgentMetadata,
                &telemetry_url,
                jackin_docker::net::get_text(&c, &u),
            )
            .await
        }
    })
    .await
    .with_context(|| format!("fetching latest GitHub release metadata for {repo}"))?;
    let release: GithubRelease = serde_json::from_str(&body)
        .with_context(|| format!("parsing latest GitHub release metadata for {repo}"))?;
    Ok(release)
}

async fn retry_metadata_with_backoff<T, F, Fut>(
    max_attempts: u32,
    initial_delay: Duration,
    f: F,
) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut last_err = ImageError::NoAttemptsMade.into();
    for attempt in 0..max_attempts {
        if attempt > 0 {
            let delay = initial_delay * (1 << (attempt - 1));
            let _retry = jackin_telemetry::record_retry_scheduled();
            record(
                "retry_backoff",
                &format!("attempt {attempt}/{max_attempts}, waiting {delay:?}"),
            );
            tokio::time::sleep(delay).await;
        }
        match f().await {
            Ok(v) => return Ok(v),
            Err(e) if is_connect_timeout(&e) => return Err(e),
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

async fn retry_with_backoff<T, F, Fut>(
    max_attempts: u32,
    initial_delay: Duration,
    f: F,
) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut last_err = ImageError::NoAttemptsMade.into();
    for attempt in 0..max_attempts {
        if attempt > 0 {
            let delay = initial_delay * (1 << (attempt - 1));
            let _retry = jackin_telemetry::record_retry_scheduled();
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
        tokio::fs::create_dir_all(parent).await?;
    }
    let tmp_download = dest.with_extension("download.tmp");
    let tmp_binary = dest.with_extension("tmp");
    drop(tokio::fs::remove_file(&tmp_download).await);
    drop(tokio::fs::remove_file(&tmp_binary).await);
    let result = download_and_cache_inner(release, dest, &tmp_download, &tmp_binary).await;
    if result.is_err() {
        drop(tokio::fs::remove_file(&tmp_download).await);
        drop(tokio::fs::remove_file(&tmp_binary).await);
    }
    result
}

async fn download_and_cache_inner(
    release: &AgentRelease,
    dest: &Path,
    tmp_download: &Path,
    tmp_binary: &Path,
) -> Result<()> {
    retry_with_backoff(3, Duration::from_millis(500), || {
        let url = release.url.clone();
        let tmp_download = tmp_download.to_owned();
        async move {
            let telemetry_url = url.clone();
            crate::telemetry_boundary::download_request(
                crate::telemetry_boundary::DownloadRoute::AgentArtifact,
                &telemetry_url,
                jackin_docker::net::download_parallel(&url, &tmp_download, None),
            )
            .await
        }
    })
    .await
    .with_context(|| format!("downloading {}", release.url))?;
    if let Some(expected) = release.checksum.as_deref() {
        let use_sha512 = match expected.len() {
            64 => false,
            128 => true,
            _ => {
                return Err(ImageError::msg(format!(
                    "{} release {} carries an unrecognized checksum length {}",
                    release.agent.slug(),
                    release.version,
                    expected.len()
                ))
                .into());
            }
        };
        let tmp_for_hash = tmp_download.to_owned();
        let actual = jackin_telemetry::spawn::joined_blocking(move || {
            if use_sha512 {
                hash_file_sha512(&tmp_for_hash)
            } else {
                hash_file_sha256(&tmp_for_hash)
            }
        })
        .await
        .context("hash worker join")?
        .with_context(|| format!("hashing {}", tmp_download.display()))?;
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(ImageError::msg(format!(
                "{} checksum mismatch for {}\n  expected {}\n  actual   {}",
                release.agent.slug(),
                release.url,
                expected,
                actual
            ))
            .into());
        }
    } else if release.agent != Agent::Grok {
        return Err(ImageError::msg(format!(
            "{} release {} has no published checksum; refusing to install an unverified binary",
            release.agent.slug(),
            release.version
        ))
        .into());
    }
    if let Some(member) = &release.archive_member {
        let archive = tmp_download.to_owned();
        let member = member.clone();
        let output = tmp_binary.to_owned();
        jackin_telemetry::spawn::joined_blocking(move || {
            extract_tar_gz_member(&archive, &member, &output)
        })
        .await
        .context("archive extraction worker join")??;
        drop(tokio::fs::remove_file(tmp_download).await);
    } else {
        tokio::fs::rename(tmp_download, tmp_binary).await?;
    }
    let binary_for_chmod = tmp_binary.to_owned();
    jackin_telemetry::spawn::joined_blocking(move || chmod_executable(&binary_for_chmod))
        .await
        .context("chmod worker join")??;

    if release.checksum.is_none() && cfg!(target_os = "linux") {
        let request = jackin_process::ExecRequest::new(tmp_binary, ["--version"])
            .stdin_mode(jackin_process::StdioMode::Null)
            .stdout_mode(jackin_process::StdioMode::Null)
            .stderr_mode(jackin_process::StdioMode::Null)
            .timeout(Duration::from_secs(15));
        let output = crate::process_telemetry::exec_async(
            &request,
            crate::process_telemetry::agent_executable(release.agent),
        )
        .await
        .context("running downloaded agent verification process")?;
        if !output.success {
            return Err(ImageError::msg("downloaded agent verification failed").into());
        }
    }

    tokio::fs::rename(tmp_binary, dest).await?;
    Ok(())
}

fn record(kind: &str, message: &str) {
    let _ = message;
    if let Some(run) = jackin_diagnostics::active_run() {
        run.compact(kind, kind);
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
    if agent == Agent::Codex {
        install_codex_code_mode_host_test_stub(&stub)?;
    }
    Ok(())
}

fn install_codex_code_mode_host_test_stub(codex_stub: &Path) -> Result<()> {
    let sibling = codex_stub.with_file_name(jackin_core::CODEX_CODE_MODE_HOST_BINARY);
    if is_executable_file(&sibling) {
        return Ok(());
    }
    std::fs::write(
        &sibling,
        b"#!/bin/sh\nprintf '%s\\n' 'Codex code-mode host is not implemented by this test stub' >&2\nexit 127\n",
    )?;
    chmod_executable(&sibling)?;
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
    read_cached_release_with_clock(paths, agent, &SystemClock)
}

fn read_cached_release_with_clock(
    paths: &JackinPaths,
    agent: Agent,
    clock: &dyn Clock,
) -> Option<AgentRelease> {
    read_cached_release_at(paths, agent, clock.now_system())
}

fn read_cached_release_at(
    paths: &JackinPaths,
    agent: Agent,
    now: SystemTime,
) -> Option<AgentRelease> {
    let path = metadata_cache_path(paths, agent);
    let metadata = std::fs::metadata(&path).ok()?;
    let modified = metadata.modified().ok()?;
    if now.duration_since(modified).ok()? >= CACHE_TTL {
        return None;
    }
    read_release_file(&path).filter(release_has_required_extras)
}

async fn read_cached_release_async(paths: &JackinPaths, agent: Agent) -> Option<AgentRelease> {
    let paths = paths.clone();
    jackin_telemetry::spawn::joined_blocking(move || read_cached_release(&paths, agent))
        .await
        .unwrap_or_default()
}

pub fn newest_cached_executable_release(
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
            release.agent == agent
                && release_has_required_extras(release)
                && is_executable_file(&cached_binary_path(paths, release))
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

async fn newest_cached_executable_release_async(
    paths: &JackinPaths,
    agent: Agent,
) -> Option<(SystemTime, AgentRelease, PathBuf)> {
    let paths = paths.clone();
    jackin_telemetry::spawn::joined_blocking(move || {
        newest_cached_executable_release(&paths, agent)
    })
    .await
    .unwrap_or_default()
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

async fn persist_release_cache_async(paths: &JackinPaths, release: &AgentRelease) {
    let paths = paths.clone();
    let release = release.clone();
    let result = jackin_telemetry::spawn::joined_blocking(move || {
        (
            write_cached_release(&paths, &release),
            write_version_release(&paths, &release),
        )
    })
    .await;
    drop(result);
}

async fn is_executable_file_async(path: &Path) -> bool {
    let path = path.to_owned();
    jackin_telemetry::spawn::joined_blocking(move || is_executable_file(&path))
        .await
        .unwrap_or(false)
}

async fn repair_cached_binary_mode_async(path: &Path) -> Result<bool> {
    let path = path.to_owned();
    jackin_telemetry::spawn::joined_blocking(move || repair_executable_file(&path))
        .await
        .context("cache chmod worker join")?
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

impl GithubRelease {
    fn asset_named(&self, asset_name: &str) -> Option<GithubAsset> {
        self.assets
            .iter()
            .find(|asset| asset.name == asset_name)
            .cloned()
    }
}

#[derive(Debug, Clone, Deserialize)]
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
