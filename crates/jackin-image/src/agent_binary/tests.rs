// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `agent_binary`.
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
    r.unwrap_err();
    assert_eq!(calls.get(), 0);
}

#[tokio::test(start_paused = true)]
async fn retry_backoff_grows_exponentially() {
    let start = tokio::time::Instant::now();
    let _unused: Result<()> = retry_with_backoff(3, Duration::from_millis(100), || async {
        anyhow::bail!("nope")
    })
    .await;
    // Attempt 1 is immediate; attempts 2 and 3 wait 100ms then 200ms.
    assert_eq!(start.elapsed(), Duration::from_millis(300));
}

#[tokio::test(start_paused = true)]
async fn metadata_retry_uses_two_attempts_for_transient_failures() {
    let calls = Cell::new(0u32);
    let r: Result<()> = retry_metadata_with_backoff(2, Duration::from_millis(10), || {
        let n = calls.get() + 1;
        calls.set(n);
        async move { anyhow::bail!("metadata attempt {n} failed") }
    })
    .await;

    assert_eq!(calls.get(), 2);
    let err = format!("{:#}", r.unwrap_err());
    assert!(err.contains("giving up after 2 attempts"), "{err}");
    assert!(err.contains("metadata attempt 2 failed"), "{err}");
}

fn release_fixture() -> AgentRelease {
    release_fixture_for(Agent::Claude, "1.2.3")
}

fn release_fixture_for(agent: Agent, version: &str) -> AgentRelease {
    AgentRelease {
        agent,
        version: version.to_owned(),
        url: format!("https://example.test/{}", agent.slug()),
        checksum: Some("abc".to_owned()),
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

#[tokio::test]
async fn read_cached_release_async_fresh_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(dir.path());
    let release = release_fixture();
    write_cached_release(&paths, &release).unwrap();
    let got = read_cached_release_async(&paths, Agent::Claude)
        .await
        .expect("fresh cache should hit");
    assert_eq!(got.version, release.version);
    assert_eq!(got.url, release.url);
}

#[test]
fn read_cached_release_past_ttl_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(dir.path());
    write_cached_release(&paths, &release_fixture()).unwrap();
    let path = metadata_cache_path(&paths, Agent::Claude);
    let stale = SystemTime::now() - Duration::from_hours(2);
    filetime::set_file_mtime(&path, filetime::FileTime::from_system_time(stale)).unwrap();
    assert!(read_cached_release(&paths, Agent::Claude).is_none());
}

#[tokio::test]
async fn newest_cached_executable_release_async_finds_newest_binary() {
    let dir = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(dir.path());
    let older = release_fixture();
    let newer = AgentRelease {
        version: "1.2.4".to_owned(),
        url: "https://example.test/claude-newer".to_owned(),
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

    let (_, release, path) = newest_cached_executable_release_async(&paths, Agent::Claude)
        .await
        .expect("cached fallback");
    assert_eq!(release.version, newer.version);
    assert_eq!(path, newer_binary);
}

#[tokio::test]
async fn ensure_available_uses_stale_cached_executable_without_foreground_resolve() {
    let dir = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(dir.path());
    let release = release_fixture();

    write_cached_release(&paths, &release).unwrap();
    let latest_path = metadata_cache_path(&paths, Agent::Claude);
    let stale = SystemTime::now() - Duration::from_hours(2);
    filetime::set_file_mtime(&latest_path, filetime::FileTime::from_system_time(stale)).unwrap();
    write_version_release(&paths, &release).unwrap();
    let binary_path = cached_binary_path(&paths, &release);
    std::fs::write(&binary_path, b"cached").unwrap();
    chmod_executable(&binary_path).unwrap();

    let diagnostics = jackin_diagnostics::RunDiagnostics::start(&paths, false, "prewarm").unwrap();
    let _guard = diagnostics.activate();

    let binary = ensure_available_impl(&paths, Agent::Claude, false)
        .await
        .expect("stale metadata should still use cached executable");

    assert_eq!(binary.path, binary_path);
    assert_eq!(binary.version.as_deref(), Some(release.version.as_str()));
    let diagnostics_log = std::fs::read_to_string(diagnostics.path()).unwrap();
    assert!(
        diagnostics_log.contains("agent_binary_cache_hit"),
        "{diagnostics_log}"
    );
    assert!(
        !diagnostics_log.contains("agent_binary_resolve_started"),
        "foreground path must not resolve before using stale cached executable: {diagnostics_log}"
    );
}

#[tokio::test]
async fn ensure_binary_or_cached_fallback_uses_cached_binary_when_primary_download_fails() {
    let dir = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(dir.path());

    // Last-known-good cached executable — the fallback target.
    let cached_good = release_fixture();
    write_version_release(&paths, &cached_good).unwrap();
    let cached_good_binary = cached_binary_path(&paths, &cached_good);
    std::fs::write(&cached_good_binary, b"good").unwrap();
    chmod_executable(&cached_good_binary).unwrap();

    // Primary release with an unparseable URL and no cached binary: the download
    // fails offline at URL parse, exercising the fallback without a network call.
    let primary = AgentRelease {
        version: "1.2.5".to_owned(),
        url: "not-a-valid-url".to_owned(),
        checksum: None,
        ..release_fixture()
    };
    let primary_cached = cached_binary_path(&paths, &primary);
    assert!(
        !is_executable_file(&primary_cached),
        "primary must start uncached"
    );

    let binary = ensure_binary_or_cached_fallback(
        &paths,
        Agent::Claude,
        &primary,
        &primary_cached,
        "test primary download failed",
    )
    .await
    .expect("falls back to the cached executable");
    assert_eq!(binary.path, cached_good_binary);
}

#[cfg(unix)]
#[tokio::test]
async fn ensure_binary_for_release_repairs_non_executable_cached_binary() {
    use std::os::unix::fs::PermissionsExt as _;

    let dir = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(dir.path());
    let release = AgentRelease {
        url: "not-a-valid-url".to_owned(),
        ..release_fixture()
    };
    let cached = cached_binary_path(&paths, &release);
    std::fs::create_dir_all(cached.parent().unwrap()).unwrap();
    std::fs::write(&cached, b"cached").unwrap();
    let mut permissions = std::fs::metadata(&cached).unwrap().permissions();
    permissions.set_mode(0o644);
    std::fs::set_permissions(&cached, permissions).unwrap();

    let diagnostics = jackin_diagnostics::RunDiagnostics::start(&paths, false, "prewarm").unwrap();
    let _guard = diagnostics.activate();

    let binary = ensure_binary_for_release(Agent::Claude, &release, &cached)
        .await
        .expect("cached binary mode should be repaired without download");

    assert_eq!(binary.path, cached);
    assert_eq!(binary.version.as_deref(), Some(release.version.as_str()));
    assert!(is_executable_file(&binary.path));
    let diagnostics_log = std::fs::read_to_string(diagnostics.path()).unwrap();
    assert!(diagnostics_log.contains("agent_binary_cache_repaired"));
}

#[tokio::test]
async fn ensure_binary_or_cached_fallback_surfaces_error_when_no_cache_exists() {
    let dir = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(dir.path());

    let primary = AgentRelease {
        version: "1.2.5".to_owned(),
        url: "not-a-valid-url".to_owned(),
        checksum: None,
        ..release_fixture()
    };
    let primary_cached = cached_binary_path(&paths, &primary);

    let error = ensure_binary_or_cached_fallback(
        &paths,
        Agent::Claude,
        &primary,
        &primary_cached,
        "test primary download failed",
    )
    .await
    .expect_err("no cached fallback leaves the original error to surface");
    let msg = format!("{error:#}");
    assert!(msg.contains("not-a-valid-url"), "{msg}");
}

#[test]
fn newest_cached_executable_release_reads_stale_version_sidecars() {
    let dir = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(dir.path());
    let older = release_fixture();
    let newer = AgentRelease {
        version: "1.2.4".to_owned(),
        url: "https://example.test/claude-newer".to_owned(),
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
fn kimi_resolver_uses_official_installer_urls() {
    assert_eq!(KIMI_DOWNLOAD_BASE_URL, "https://code.kimi.com/kimi-code");
    assert_eq!(
        KIMI_BINARY_BASE_URL,
        "https://code.kimi.com/kimi-code/binaries"
    );
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
        name: "asset".to_owned(),
        browser_download_url: "https://example.test/a".to_owned(),
        digest: digest.map(str::to_owned),
    };
    assert_eq!(
        asset(Some("sha256:deadbeef")).sha256_digest().as_deref(),
        Some("deadbeef")
    );
    assert!(asset(Some("md5:deadbeef")).sha256_digest().is_none());
    assert!(asset(None).sha256_digest().is_none());
}

#[test]
fn read_cached_release_at_past_ttl_without_wall_clock() {
    use jackin_core::ManualClock;
    let dir = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(dir.path());
    write_cached_release(&paths, &release_fixture()).unwrap();
    let path = metadata_cache_path(&paths, Agent::Claude);
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    let clock = ManualClock::with_system_base(modified);
    clock.advance(CACHE_TTL);
    assert!(
        read_cached_release_with_clock(&paths, Agent::Claude, &clock).is_none(),
        "exactly CACHE_TTL old must miss"
    );
    let fresh_clock = ManualClock::with_system_base(modified);
    fresh_clock.advance(
        CACHE_TTL
            .checked_sub(Duration::from_secs(1))
            .expect("TTL exceeds one second"),
    );
    assert!(
        read_cached_release_with_clock(&paths, Agent::Claude, &fresh_clock).is_some(),
        "under TTL must hit"
    );
}
