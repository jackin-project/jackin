// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
