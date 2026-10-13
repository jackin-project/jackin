// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) const DOWNLOAD_WIRE_CHILD: &str = "JACKIN_DOWNLOAD_WIRE_CHILD";

pub(super) const DOWNLOAD_WIRE_TEST: &str = "agent_binary::tests::case_01::conformance_wire_download_cache_and_retry_are_bounded_and_private";

pub(super) fn dispatch_download_wire_child() -> Result<bool> {
    if std::env::var_os(DOWNLOAD_WIRE_CHILD).is_some() {
        return Ok(false);
    }
    let status = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", DOWNLOAD_WIRE_TEST, "--nocapture"])
        .env(DOWNLOAD_WIRE_CHILD, "1")
        .status()?;
    anyhow::ensure!(status.success(), "isolated download wire test failed");
    Ok(true)
}

#[derive(Clone)]
pub(super) struct RetryCounter(Arc<AtomicUsize>);

impl<S: tracing::Subscriber> Layer<S> for RetryCounter {
    fn on_event(&self, event: &tracing::Event<'_>, _context: Context<'_, S>) {
        if event.metadata().name() == jackin_telemetry::schema::events::RETRY_SCHEDULED {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
}

pub(super) fn capture_retries() -> (Arc<AtomicUsize>, tracing::subscriber::DefaultGuard) {
    let count = Arc::new(AtomicUsize::new(0));
    let guard = tracing::subscriber::set_default(tracing_subscriber::layer::SubscriberExt::with(
        tracing_subscriber::registry(),
        RetryCounter(Arc::clone(&count)),
    ));
    (count, guard)
}

pub(super) fn exercise_private_cache(
    runtime: &tokio::runtime::Runtime,
) -> Result<(tempfile::TempDir, String, String)> {
    let temp = tempfile::tempdir()?;
    let root = temp.path().join("wire-private-cache-key");
    let paths = JackinPaths::for_tests(&root);
    let cached_agent = runtime.block_on(ensure_available_impl(&paths, Agent::Kimi, true))?;
    assert!(
        cached_agent.path.starts_with(&root),
        "real cache path did not consume private root: {}",
        cached_agent.path.display()
    );
    Ok((
        temp,
        root.to_string_lossy().into_owned(),
        cached_agent.path.to_string_lossy().into_owned(),
    ))
}

pub(super) fn exercise_deceptive_host(runtime: &tokio::runtime::Runtime, url: &str) -> Result<()> {
    runtime.block_on(crate::telemetry_boundary::download_request(
        crate::telemetry_boundary::DownloadRoute::AgentMetadata,
        url,
        async { Ok::<_, anyhow::Error>(()) },
    ))
}

pub(super) fn emit_all_cache_decisions() {
    for name in jackin_telemetry::schema::enums::CacheName::ALL
        .iter()
        .copied()
    {
        for result in jackin_telemetry::schema::enums::CacheResult::ALL
            .iter()
            .copied()
        {
            crate::telemetry_boundary::cache_decision(name, result);
        }
    }
}

pub(super) fn release_fixture() -> AgentRelease {
    release_fixture_for(Agent::Claude, "1.2.3")
}

pub(super) fn release_fixture_for(agent: Agent, version: &str) -> AgentRelease {
    AgentRelease {
        agent,
        version: version.to_owned(),
        url: format!("https://example.test/{}", agent.slug()),
        checksum: Some("abc".to_owned()),
        archive_member: None,
    }
}
