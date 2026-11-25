// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

use jackin_console::domain::instance::InstanceStatus;
use jackin_core::{Config, ConfigEdit, GatewayName, OpenShellWorkspace};
use jackin_openshell::{Gateway, GatewayError, GatewayOp};
use jackin_session::{Unexported, WorkState};
use jackin_testkit::{
    ConsoleRig, FakeGateway, Fault, Rig, TestResult, Trigger, guard, session_over,
};
use jackin_tui::{ConsoleLimits, DefaultGateway, Fleet, GatewayState, SessionSet, read_fleet};
use tokio_util::sync::CancellationToken;

#[expect(dead_code, reason = "each test binary uses part of the shared tap")]
#[path = "../../jackin-session/tests/common/tap.rs"]
mod tap;

use tap::Tap;

const DEADLINE: Duration = Duration::from_secs(1);
const STALE: &str = "▲ gateway stale";

async fn refresh(console: &mut ConsoleRig) -> TestResult {
    console.elapse(5);
    console.tick(5);
    console.settle().await?;
    Ok(())
}

fn statuses(console: &mut ConsoleRig) -> Vec<InstanceStatus> {
    let mut found: Vec<InstanceStatus> = console
        .console()
        .app()
        .world
        .instances
        .iter()
        .filter_map(|entry| entry.as_available().map(|instance| instance.status))
        .collect();
    found.sort_by_key(|status| format!("{status:?}"));
    found
}

fn second_gateway(
    name: &str,
) -> Result<(GatewayName, Arc<FakeGateway>), Box<dyn std::error::Error>> {
    let name = GatewayName::parse(name)?;
    let gateway = Arc::new(FakeGateway::new(
        name.clone(),
        OpenShellWorkspace::parse("default")?,
    ));
    Ok((name, gateway))
}

async fn configure(rig: &Rig, name: &GatewayName) -> TestResult {
    let entry = rig
        .config
        .gateways()
        .next()
        .map(|(_, entry)| entry.clone())
        .ok_or("the rig has no gateway")?;
    rig.world
        .config
        .edit(vec![ConfigEdit::AddGateway {
            name: name.clone(),
            entry,
        }])
        .await?;
    Ok(())
}

#[tokio::test]
async fn slow_gateway_keeps_the_fast_listing() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        rig.world.seed_all(&rig.config).await?;
        let (name, slow) = second_gateway("slow-gateway")?;
        configure(&rig, &name).await?;
        slow.inject(Fault::HoldAt {
            trigger: Trigger::Op(GatewayOp::Find),
        });
        let port: Arc<dyn Gateway> = Arc::<FakeGateway>::clone(&slow);
        let session = session_over(&rig.world, port);
        let mut console = ConsoleRig::builder(&rig.world)
            .gateway(name, GatewayState::Connected(Box::new(session)))
            .deadline(DEADLINE)
            .build()
            .await?;
        assert_eq!(statuses(&mut console), [InstanceStatus::Running]);
        let status = console.status_line();
        assert!(status.contains("slow-gateway"), "{status}");
        assert!(status.contains("no answer within 1s"), "{status}");
        assert!(!status.contains("fake-gateway"), "{status}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn slow_inspect_keeps_the_sandbox_listed() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        rig.launch(2).await?;
        rig.world.seed_all(&rig.config).await?;
        rig.world.gateway.inject(Fault::HoldAt {
            trigger: Trigger::Op(GatewayOp::Detail),
        });
        let mut console = ConsoleRig::builder(&rig.world)
            .deadline(DEADLINE)
            .build()
            .await?;
        assert_eq!(
            statuses(&mut console),
            [InstanceStatus::RestoreAvailable, InstanceStatus::Running]
        );
        assert!(
            console.status_line().is_empty(),
            "{}",
            console.status_line()
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn slow_work_read_keeps_the_sandbox_listed() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        rig.handle(1).await?.exit_codex(0);
        rig.launch(2).await?;
        rig.world.seed_all(&rig.config).await?;
        rig.world.gateway.inject(Fault::HoldAt {
            trigger: Trigger::Op(GatewayOp::Program),
        });
        let mut console = ConsoleRig::builder(&rig.world)
            .deadline(DEADLINE)
            .build()
            .await?;
        assert_eq!(
            statuses(&mut console),
            [InstanceStatus::CleanExited, InstanceStatus::Running]
        );
        assert!(
            console.status_line().is_empty(),
            "{}",
            console.status_line()
        );
        Ok(())
    })
    .await
}

fn refused() -> GatewayError {
    GatewayError::Unreachable {
        endpoint: "127.0.0.1:8080".to_owned(),
        message: "connection refused".to_owned(),
    }
}

async fn with_second_gateway(
    failing_at_start: bool,
) -> Result<(Rig, Arc<FakeGateway>, ConsoleRig), Box<dyn std::error::Error>> {
    let rig = Rig::new().await?;
    rig.launch(1).await?;
    rig.world.seed_all(&rig.config).await?;
    let (name, second) = second_gateway("second-gateway")?;
    configure(&rig, &name).await?;
    if failing_at_start {
        second.inject(Fault::FailNext {
            op: GatewayOp::Find,
            error: refused(),
        });
    }
    let port: Arc<dyn Gateway> = Arc::<FakeGateway>::clone(&second);
    let session = session_over(&rig.world, port);
    let mut console = ConsoleRig::builder(&rig.world)
        .gateway(name, GatewayState::Connected(Box::new(session)))
        .deadline(DEADLINE)
        .size(200, 40)
        .build()
        .await?;
    console.skip_intro();
    Ok((rig, second, console))
}

#[tokio::test]
async fn failed_gateway_shows_after_refresh() -> TestResult {
    guard(async {
        let (_rig, second, mut console) = with_second_gateway(false).await?;
        let screen = console.screen();
        assert!(!screen.contains(STALE), "{screen}");
        second.inject(Fault::FailNext {
            op: GatewayOp::Find,
            error: refused(),
        });
        refresh(&mut console).await?;
        let screen = console.screen();
        assert!(screen.contains(STALE), "{screen}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn recovered_gateway_shows_healthy() -> TestResult {
    guard(async {
        let (_rig, _second, mut console) = with_second_gateway(true).await?;
        let screen = console.screen();
        assert!(screen.contains(STALE), "{screen}");
        refresh(&mut console).await?;
        let screen = console.screen();
        assert!(!screen.contains(STALE), "{screen}");
        assert_eq!(statuses(&mut console), [InstanceStatus::Running]);
        Ok(())
    })
    .await
}

fn limits(deadline: Duration, reads: usize) -> ConsoleLimits {
    ConsoleLimits {
        connect_deadline: deadline,
        work_reads: NonZeroUsize::new(reads).unwrap_or(NonZeroUsize::MIN),
        pane_inbox_bytes: NonZeroUsize::MIN,
        supervisor_poll: Duration::from_millis(20),
    }
}

async fn seeded(rig: &Rig) -> Result<Config, Box<dyn std::error::Error>> {
    rig.world.seed_all(&rig.config).await?;
    Ok(rig.world.config.load().await?)
}

fn sessions(rig: &Rig, config: &Config, gateway: Arc<dyn Gateway>) -> SessionSet {
    let name = gateway.name().clone();
    let session = session_over(&rig.world, gateway);
    SessionSet::new(
        config,
        DefaultGateway::Followed(Some(name.clone())),
        BTreeMap::from([(name, GatewayState::Connected(Box::new(session)))]),
    )
}

async fn fleet_of(
    rig: &Rig,
    config: &Config,
    limits: ConsoleLimits,
) -> Result<Fleet, Box<dyn std::error::Error>> {
    let set = sessions(rig, config, Arc::<FakeGateway>::clone(&rig.world.gateway));
    read_fleet(&set, config, limits, &CancellationToken::new())
        .await
        .ok_or_else(|| "an uncancelled pass returns a fleet".into())
}

fn work_of(fleet: &Fleet, name: &jackin_core::SandboxName) -> Option<WorkState> {
    fleet
        .sandboxes
        .iter()
        .find(|observed| observed.summary.name == *name)
        .and_then(|observed| observed.work.clone())
}

fn dirty(uncommitted: usize) -> WorkState {
    WorkState::Unexported(Unexported {
        uncommitted,
        branches: Vec::new(),
        detached: None,
        worktrees: Vec::new(),
        unusable: Vec::new(),
    })
}

#[tokio::test]
async fn work_is_read_again_after_the_sandbox_changes_behind_an_unchanged_status() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        handle.exit_codex(0);
        let config = seeded(&rig).await?;
        let name = handle.name();
        let limits = limits(Duration::from_secs(30), 1);

        let first = fleet_of(&rig, &config, limits).await?;
        assert_eq!(work_of(&first, &name), Some(WorkState::Exported));

        std::fs::write(handle.root().join("project/loose.txt"), b"edit")?;
        let second = fleet_of(&rig, &config, limits).await?;
        assert_eq!(work_of(&second, &name), Some(dirty(1)));

        std::fs::remove_file(handle.root().join("project/loose.txt"))?;
        let third = fleet_of(&rig, &config, limits).await?;
        assert_eq!(work_of(&third, &name), Some(WorkState::Exported));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn work_follows_a_recovery_ref_that_changes_on_the_host() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        handle.git(&["checkout", "-q", "-b", "side"]).await?;
        handle.commit_in_project("side.txt", b"s", "side").await?;
        handle.git(&["checkout", "-q", "main"]).await?;
        handle.exit_codex(0);
        let config = seeded(&rig).await?;
        let limits = limits(Duration::from_secs(30), 1);

        let before = fleet_of(&rig, &config, limits).await?;
        assert!(
            matches!(work_of(&before, &name), Some(WorkState::Unexported(_))),
            "{:?}",
            work_of(&before, &name)
        );

        rig.world
            .session
            .export(&rig.config, &name, CancellationToken::new())
            .await?;
        let exported = fleet_of(&rig, &config, limits).await?;
        assert_eq!(work_of(&exported, &name), Some(WorkState::Exported));

        let recovery = format!("refs/heads/jackin/{name}/side");
        rig.repo.git(&["update-ref", "-d", &recovery]).await?;
        let after = fleet_of(&rig, &config, limits).await?;
        assert!(
            matches!(work_of(&after, &name), Some(WorkState::Unexported(_))),
            "{:?}",
            work_of(&after, &name)
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_large_fleet_is_observed_by_at_most_the_configured_number_of_sandboxes_at_once()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        for id in 1..=8 {
            rig.launch(id).await?;
            rig.handle(id).await?.exit_codex(0);
        }
        rig.launch(9).await?;
        let running = rig.sandbox_name(9)?;
        let config = seeded(&rig).await?;
        let probe = Tap::new(&rig);
        probe.pause_reads(Duration::from_millis(25));
        let set = sessions(&rig, &config, Arc::<Tap>::clone(&probe));

        let fleet = read_fleet(
            &set,
            &config,
            limits(Duration::from_secs(60), 2),
            &CancellationToken::new(),
        )
        .await
        .ok_or("an uncancelled pass returns a fleet")?;

        assert_eq!(fleet.sandboxes.len(), 9);
        assert_eq!(work_of(&fleet, &running), None);
        assert!(
            fleet
                .sandboxes
                .iter()
                .filter(|observed| observed.summary.name != running)
                .all(|observed| observed.work == Some(WorkState::Exported)),
            "{:?}",
            fleet.sandboxes.iter().map(|o| &o.work).collect::<Vec<_>>()
        );
        assert!(
            fleet
                .sandboxes
                .iter()
                .all(|observed| observed.supervisor.is_some())
        );
        assert_eq!(probe.peak_observed(), 2);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn slow_reads_and_queued_permits_share_one_deadline_and_keep_every_sandbox_listed()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        for id in 1..=4 {
            rig.launch(id).await?;
            rig.handle(id).await?.exit_codex(0);
        }
        let config = seeded(&rig).await?;
        let probe = Tap::new(&rig);
        probe.stall_reads();
        let set = sessions(&rig, &config, Arc::<Tap>::clone(&probe));
        let deadline = Duration::from_millis(500);

        let started = Instant::now();
        let fleet = read_fleet(
            &set,
            &config,
            limits(deadline, 1),
            &CancellationToken::new(),
        )
        .await
        .ok_or("an uncancelled pass returns a fleet")?;
        let elapsed = started.elapsed();

        assert_eq!(fleet.sandboxes.len(), 4);
        assert!(
            fleet
                .sandboxes
                .iter()
                .all(|observed| observed.work.is_none() && observed.task.is_none())
        );
        assert!(fleet.failed().next().is_none());
        assert!(
            elapsed >= deadline && elapsed < deadline * 4,
            "the pass took {elapsed:?} against a deadline of {deadline:?}"
        );
        Ok(())
    })
    .await
}

async fn stalled_pair_costs_only_itself(stuck: [u8; 2], healthy: [u8; 2]) -> TestResult {
    let rig = Rig::new().await?;
    for id in 1..=4 {
        rig.launch(id).await?;
        rig.handle(id).await?.exit_codex(0);
    }
    let config = seeded(&rig).await?;
    let stuck = [rig.sandbox_name(stuck[0])?, rig.sandbox_name(stuck[1])?];
    let healthy = [rig.sandbox_name(healthy[0])?, rig.sandbox_name(healthy[1])?];
    let probe = Tap::new(&rig);
    probe.stall_only(&stuck);
    let set = sessions(&rig, &config, Arc::<Tap>::clone(&probe));

    let fleet = read_fleet(
        &set,
        &config,
        limits(Duration::from_secs(3), 2),
        &CancellationToken::new(),
    )
    .await
    .ok_or("an uncancelled pass returns a fleet")?;

    assert_eq!(fleet.sandboxes.len(), 4);
    for observed in &fleet.sandboxes {
        if stuck.contains(&observed.summary.name) {
            assert!(observed.supervisor.is_none() && observed.work.is_none());
        } else {
            assert!(healthy.contains(&observed.summary.name));
            assert!(observed.supervisor.is_some());
            assert_eq!(observed.work, Some(WorkState::Exported));
        }
    }
    Ok(())
}

#[tokio::test]
async fn stalled_sandboxes_listed_first_cost_only_their_own_reading() -> TestResult {
    guard(stalled_pair_costs_only_itself([1, 2], [3, 4])).await
}

#[tokio::test]
async fn stalled_sandboxes_listed_last_cost_only_their_own_reading() -> TestResult {
    guard(stalled_pair_costs_only_itself([3, 4], [1, 2])).await
}

async fn cancelled_pass(rig: &Rig, probe: &Arc<Tap>) -> TestResult {
    let config = seeded(rig).await?;
    let set = sessions(rig, &config, Arc::<Tap>::clone(probe));
    let cancel = CancellationToken::new();
    let stopper = cancel.clone();
    let watched = Arc::clone(probe);
    tokio::spawn(async move {
        watched.until_stalled().await;
        stopper.cancel();
    });

    let read = tokio::time::timeout(
        Duration::from_secs(20),
        read_fleet(&set, &config, limits(Duration::from_secs(300), 1), &cancel),
    )
    .await;

    let fleet = read.map_err(|_| "the pass ignored the cancellation")?;
    assert!(fleet.is_none(), "a cancelled pass returns no fleet");
    Ok(())
}

#[tokio::test]
async fn cancelling_the_pass_returns_even_with_every_read_stalled_or_queued() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        for id in 1..=3 {
            rig.launch(id).await?;
            rig.handle(id).await?.exit_codex(0);
        }
        let probe = Tap::new(&rig);
        probe.stall_reads();
        cancelled_pass(&rig, &probe).await
    })
    .await
}

#[tokio::test]
async fn cancelling_the_pass_returns_even_while_the_listing_is_stalled() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let probe = Tap::new(&rig);
        probe.stall_listing();
        cancelled_pass(&rig, &probe).await
    })
    .await
}

#[tokio::test]
async fn a_gateway_that_cannot_be_read_does_not_hide_the_one_that_can() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        rig.handle(1).await?.exit_codex(0);
        seeded(&rig).await?;
        let (down, _) = second_gateway("down-gateway")?;
        configure(&rig, &down).await?;
        let config = rig.world.config.load().await?;
        let up = rig.world.gateway.name().clone();
        let session = session_over(&rig.world, Arc::<FakeGateway>::clone(&rig.world.gateway));
        let set = SessionSet::new(
            &config,
            DefaultGateway::Followed(Some(up.clone())),
            BTreeMap::from([
                (up, GatewayState::Connected(Box::new(session))),
                (down.clone(), GatewayState::Failed(refused())),
            ]),
        );

        let fleet = read_fleet(
            &set,
            &config,
            limits(Duration::from_secs(30), 1),
            &CancellationToken::new(),
        )
        .await
        .ok_or("an uncancelled pass returns a fleet")?;

        assert_eq!(fleet.sandboxes.len(), 1);
        assert_eq!(fleet.sandboxes[0].work, Some(WorkState::Exported));
        assert_eq!(
            fleet.failed().map(|g| g.name.clone()).collect::<Vec<_>>(),
            [down]
        );
        Ok(())
    })
    .await
}
