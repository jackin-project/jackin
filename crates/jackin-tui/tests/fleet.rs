// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;

use jackin_console::domain::instance::InstanceStatus;
use jackin_core::{ConfigEdit, GatewayName, OpenShellWorkspace, SandboxName};
use jackin_openshell::{Gateway, GatewayError, GatewayOp};
use jackin_session::WorkState;
use jackin_testkit::{
    ConsoleRig, FakeGateway, Fault, Rig, TestResult, Trigger, guard, session_over,
};
use jackin_tui::{
    ConsoleLimits, DefaultGateway, Fleet, GatewayState, SessionSet, read_fleet as inspect_fleet,
};
use tokio_util::sync::CancellationToken;

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
        .map(|instance| instance.status)
        .collect();
    found.sort_by_key(|status| format!("{status:?}"));
    found
}

fn fleet_sessions(rig: &Rig) -> SessionSet {
    let name = rig.world.gateway.name().clone();
    let port: Arc<dyn Gateway> = Arc::<FakeGateway>::clone(&rig.world.gateway);
    let session = session_over(&rig.world, port);
    SessionSet::new(
        &rig.config,
        DefaultGateway::Pinned(name.clone()),
        BTreeMap::from([(name, GatewayState::Connected(Box::new(session)))]),
    )
}

async fn fleet_read(rig: &Rig, sessions: &SessionSet, work_reads: NonZeroUsize) -> Fleet {
    let limits = ConsoleLimits {
        connect_deadline: DEADLINE,
        work_reads,
        pane_inbox_bytes: NonZeroUsize::MIN,
        supervisor_poll: Duration::from_millis(20),
    };
    inspect_fleet(sessions, &rig.config, limits, &CancellationToken::new()).await
}

fn work_for(fleet: &Fleet, sandbox: &SandboxName) -> Option<WorkState> {
    fleet
        .sandboxes
        .iter()
        .find(|observed| observed.summary.name == *sandbox)
        .and_then(|observed| observed.work.clone())
}

struct ReleaseProgramHold<'a> {
    gateway: &'a FakeGateway,
    trigger: Trigger,
}

impl Drop for ReleaseProgramHold<'_> {
    fn drop(&mut self) {
        self.gateway.release(&self.trigger);
    }
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
        .edit(
            vec![ConfigEdit::AddGateway {
                name: name.clone(),
                entry,
            }],
            &rig.world.cancel,
        )
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

#[tokio::test]
async fn host_recovery_refs_change_fleet_work_classification() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let sandbox = rig.handle(1).await?;
        sandbox
            .commit_in_project("src/recovery.rs", b"fn recovery() {}\n", "recovery")
            .await?;
        let branch = sandbox.git(&["branch", "--show-current"]).await?;
        sandbox.exit_codex(0);
        let name = sandbox.name();
        let recovery = format!("refs/heads/jackin/{name}/{}", branch.trim());
        let sessions = fleet_sessions(&rig);
        let initial = fleet_read(&rig, &sessions, NonZeroUsize::new(2).unwrap()).await;
        assert!(matches!(work_for(&initial, &name), Some(WorkState::Unexported(state)) if state.uncommitted == 0 && !state.branches.is_empty()));

        rig.world
            .session
            .export(&rig.config, &name, CancellationToken::new())
            .await?;
        let refs = rig
            .repo
            .git(&["for-each-ref", "--format=%(refname)", "refs/heads/jackin"])
            .await?;
        assert!(refs.lines().any(|reference| reference == recovery));
        let exported = fleet_read(&rig, &sessions, NonZeroUsize::new(2).unwrap()).await;
        assert_eq!(work_for(&exported, &name), Some(WorkState::Exported));
        let unchanged = fleet_read(&rig, &sessions, NonZeroUsize::new(2).unwrap()).await;
        assert_eq!(work_for(&unchanged, &name), Some(WorkState::Exported));

        rig.repo.git(&["update-ref", "-d", &recovery]).await?;
        let missing_ref = fleet_read(&rig, &sessions, NonZeroUsize::new(2).unwrap()).await;
        assert!(matches!(work_for(&missing_ref, &name), Some(WorkState::Unexported(state)) if state.uncommitted == 0 && !state.branches.is_empty()));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn sandbox_changes_after_exit_change_fleet_work_classification() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let sandbox = rig.handle(1).await?;
        sandbox.exit_codex(0);
        let name = sandbox.name();
        let sessions = fleet_sessions(&rig);
        let initial = fleet_read(&rig, &sessions, NonZeroUsize::new(2).unwrap()).await;
        assert_eq!(work_for(&initial, &name), Some(WorkState::Exported));

        let dirty = sandbox.root().join("project/after-exit.txt");
        tokio::fs::write(&dirty, b"written after exit\n").await?;
        let changed = fleet_read(&rig, &sessions, NonZeroUsize::new(2).unwrap()).await;
        assert!(matches!(work_for(&changed, &name), Some(WorkState::Unexported(state)) if state.uncommitted == 1 && state.unpushed() == 0));

        tokio::fs::remove_file(dirty).await?;
        let restored = fleet_read(&rig, &sessions, NonZeroUsize::new(2).unwrap()).await;
        assert_eq!(work_for(&restored, &name), Some(WorkState::Exported));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_unknown_work_state_retries_and_does_not_hide_a_read_failure() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let sandbox = rig.handle(1).await?;
        sandbox
            .commit_in_project("src/retry.rs", b"fn retry() {}\n", "retry")
            .await?;
        sandbox.exit_codex(0);
        let name = sandbox.name();
        let sessions = fleet_sessions(&rig);
        let known = fleet_read(&rig, &sessions, NonZeroUsize::new(2).unwrap()).await;
        assert!(matches!(work_for(&known, &name), Some(WorkState::Unexported(state)) if !state.branches.is_empty()));

        rig.world.gateway.inject(Fault::FailNext {
            op: GatewayOp::Program,
            error: refused(),
        });
        let unknown = fleet_read(&rig, &sessions, NonZeroUsize::new(2).unwrap()).await;
        assert!(matches!(work_for(&unknown, &name), Some(WorkState::Unknown(_))));

        let recovered = fleet_read(&rig, &sessions, NonZeroUsize::new(2).unwrap()).await;
        let recovered_state = work_for(&recovered, &name);
        assert!(matches!(&recovered_state, Some(WorkState::Unexported(state)) if !state.branches.is_empty()));
        let unchanged = fleet_read(&rig, &sessions, NonZeroUsize::new(2).unwrap()).await;
        assert_eq!(work_for(&unchanged, &name), recovered_state);
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

#[tokio::test]
async fn the_sandbox_reads_of_one_gateway_stay_within_the_read_limit() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        for launch in 1..=6 {
            rig.launch(launch).await?;
            let sandbox = rig.handle(launch).await?;
            if launch <= 3 {
                let path = format!("src/work-{launch}.rs");
                let content = format!("fn work_{launch}() {{}}\n");
                sandbox
                    .commit_in_project(
                        &path,
                        content.as_bytes(),
                        "work",
                    )
                    .await?;
            }
            sandbox.exit_codex(0);
        }
        let sessions = fleet_sessions(&rig);
        let initial = fleet_read(&rig, &sessions, NonZeroUsize::new(2).unwrap()).await;
        assert_eq!(initial.sandboxes.len(), 6);
        assert_eq!(
            initial.sandboxes.iter()
                .filter(|observed| matches!(observed.work, Some(WorkState::Unexported(ref state)) if state.unpushed() > 0))
                .count(),
            3,
            "{initial:?}"
        );
        assert_eq!(
            initial.sandboxes.iter()
                .filter(|observed| observed.work == Some(WorkState::Exported))
                .count(),
            3,
            "{initial:?}"
        );

        let gateway = Arc::clone(&rig.world.gateway);
        gateway.reset_program_peak();
        let trigger = Trigger::Op(GatewayOp::Program);
        gateway.inject(Fault::HoldAt {
            trigger: trigger.clone(),
        });
        let _release = ReleaseProgramHold {
            gateway: &gateway,
            trigger: trigger.clone(),
        };
        let task_config = rig.config.clone();
        let task_sessions = fleet_sessions(&rig);
        let task_cancel = CancellationToken::new();
        let read = tokio::spawn(async move {
            let limits = ConsoleLimits {
                connect_deadline: DEADLINE,
                work_reads: NonZeroUsize::new(2).unwrap(),
                pane_inbox_bytes: NonZeroUsize::MIN,
                supervisor_poll: Duration::from_millis(20),
            };
            inspect_fleet(&task_sessions, &task_config, limits, &task_cancel).await
        });
        let held = tokio::time::timeout(DEADLINE, gateway.until_held(&trigger))
            .await
            .is_ok_and(|reached| reached);
        let overlapped = tokio::time::timeout(DEADLINE, gateway.until_peak_programs(2))
            .await
            .is_ok();
        let released = gateway.release(&trigger);
        let refreshed = read.await?;
        assert!(held, "a work inspection did not reach the gate");
        assert!(released, "the held work inspection was not released");
        assert!(overlapped, "work reads did not overlap");
        let peak = gateway.peak_programs();
        assert!(peak >= 2, "work reads did not overlap: {peak}");
        assert!(peak <= 2, "{peak} work reads exceeded the configured limit");
        assert_eq!(refreshed.sandboxes.len(), initial.sandboxes.len());
        for launch in 1..=6 {
            let name = rig.sandbox_name(launch)?;
            let work = work_for(&refreshed, &name);
            if launch <= 3 {
                assert!(matches!(work, Some(WorkState::Unexported(ref state)) if state.unpushed() > 0));
            } else {
                assert_eq!(work, Some(WorkState::Exported));
            }
        }
        Ok(())
    })
    .await
}
