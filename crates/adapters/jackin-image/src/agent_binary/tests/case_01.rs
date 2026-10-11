// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test(start_paused = true)]
async fn retry_succeeds_on_first_try() {
    let (retries, _guard) = capture_retries();
    let calls = Cell::new(0u32);
    let r: Result<u32> = retry_with_backoff(3, Duration::from_millis(10), || {
        calls.set(calls.get() + 1);
        async { Ok(42) }
    })
    .await;
    assert_eq!(r.unwrap(), 42);
    assert_eq!(calls.get(), 1);
    assert_eq!(retries.load(Ordering::Relaxed), 0);
}

#[tokio::test(start_paused = true)]
async fn retry_recovers_after_transient_failures() {
    let (retries, _guard) = capture_retries();
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
    assert_eq!(retries.load(Ordering::Relaxed), 2);
}

#[tokio::test(start_paused = true)]
async fn retry_exhausts_and_returns_last_error() {
    let (retries, _guard) = capture_retries();
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
    assert_eq!(retries.load(Ordering::Relaxed), 2);
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
    let (retries, _guard) = capture_retries();
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
    assert_eq!(retries.load(Ordering::Relaxed), 1);
}

#[test]
fn conformance_wire_download_cache_and_retry_are_bounded_and_private() -> Result<()> {
    if dispatch_download_wire_child()? {
        return Ok(());
    }

    use std::io::{Read as _, Write as _};

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let testbed = runtime.block_on(async { jackin_otlp_testbed::Testbed::start() })?;
    jackin_diagnostics::init_wire_test_export(
        &testbed.endpoint(),
        jackin_diagnostics::ServiceIdentity::HOST_ONE_SHOT,
    )?;

    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    let address = listener.local_addr()?;
    let server = std::thread::spawn(move || -> std::io::Result<()> {
        let (mut stream, _) = listener.accept()?;
        let mut request = [0_u8; 1024];
        let _read = stream.read(&mut request)?;
        stream.write_all(
            b"HTTP/1.1 200 OK\r\ncontent-length: 20\r\nconnection: close\r\n\r\nwire-private-payload",
        )
    });
    let private_url = format!("http://{address}/wire-private-artifact?token=wire-private-token");
    let client = jackin_docker::net::http_client(HeaderMap::new())?;
    let payload = runtime.block_on(crate::telemetry_boundary::download_request(
        crate::telemetry_boundary::DownloadRoute::AgentMetadata,
        &private_url,
        jackin_docker::net::get_text(&client, &private_url),
    ))?;
    assert_eq!(payload, "wire-private-payload");
    server.join().expect("download server thread")?;

    let failed_url = "https://downloads.claude.ai/wire-private-failed-artifact?secret=value";
    let failure = runtime.block_on(crate::telemetry_boundary::download_request(
        crate::telemetry_boundary::DownloadRoute::AgentArtifact,
        failed_url,
        async { Err::<(), _>(anyhow::anyhow!("wire-private-download-error")) },
    ));
    assert!(failure.is_err());
    for route in [
        crate::telemetry_boundary::DownloadRoute::CapsuleArtifact,
        crate::telemetry_boundary::DownloadRoute::CapsuleManifest,
        crate::telemetry_boundary::DownloadRoute::CapsuleManifestBundle,
    ] {
        runtime.block_on(crate::telemetry_boundary::download_request(
            route,
            "https://github.com/wire-private-capsule-route?token=wire-private-capsule-token",
            async { Ok::<_, anyhow::Error>(()) },
        ))?;
    }
    let deceptive_host =
        "https://downloads.claude.ai.evil.invalid/wire-private-host?token=wire-private-host-token";
    exercise_deceptive_host(&runtime, deceptive_host)?;

    let (_private_cache_temp, private_cache_root, cached_agent_path) =
        exercise_private_cache(&runtime)?;
    emit_all_cache_decisions();
    let attempts = Cell::new(0_u32);
    let recovered = runtime.block_on(retry_metadata_with_backoff(2, Duration::ZERO, || {
        let attempt = attempts.get() + 1;
        attempts.set(attempt);
        async move {
            if attempt == 1 {
                anyhow::bail!("wire-private-retry-error")
            }
            Ok(attempt)
        }
    }))?;
    assert_eq!(recovered, 2);
    jackin_diagnostics::flush_wire_test_export()?;
    assert!(runtime.block_on(testbed.wait_for_all_signals(Duration::from_secs(2))));

    let http_spans = testbed
        .spans()
        .into_iter()
        .filter(|span| span.name == "http.client")
        .collect::<Vec<_>>();
    assert_eq!(http_spans.len(), 6);
    let span_wire = format!("{http_spans:?}");
    for expected in [
        "/agent-binaries/{version}/metadata",
        "/agent-binaries/{version}/{artifact}",
        "/releases/download/{version}/{artifact}",
        "/releases/download/{version}/capsule-manifest.json",
        "/releases/download/{version}/capsule-manifest.json.bundle",
        "downloads.claude.ai",
        "github.com",
        "success",
        "failure",
        "http_error",
    ] {
        assert!(
            span_wire.contains(expected),
            "missing {expected}: {span_wire}"
        );
    }
    let events = testbed.log_records();
    assert_eq!(
        events
            .iter()
            .filter(|event| event.event_name == "cache.decision")
            .count(),
        26
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.event_name == "retry.scheduled")
            .count(),
        1
    );
    let metric_names = testbed.metric_names();
    for expected in [
        "cache.decisions",
        "cache.decision.active",
        "cache.decision.duration",
    ] {
        assert!(metric_names.iter().any(|name| name == expected));
    }
    let address = address.to_string();
    let prohibited = [
        private_url.as_str(),
        address.as_str(),
        "wire-private-artifact",
        "wire-private-token",
        "wire-private-payload",
        "wire-private-failed-artifact",
        "wire-private-download-error",
        "wire-private-retry-error",
        "wire-private-capsule-route",
        "wire-private-capsule-token",
        deceptive_host,
        "downloads.claude.ai.evil.invalid",
        "wire-private-host-token",
        private_cache_root.as_str(),
        cached_agent_path.as_str(),
        "wire-private-cache-key",
    ];
    assert_eq!(
        testbed.prohibited_value_violations(&prohibited),
        Vec::<String>::new()
    );
    assert_eq!(testbed.legacy_namespace_violations(), Vec::<String>::new());
    jackin_diagnostics::shutdown_capsule_tracing();
    Ok(())
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
