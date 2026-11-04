// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::sync::Arc;

use jackin_core::{
    Clock, DockerLayer, ENGINE_CERT_MAX_BYTES, EngineCert, EngineCertProblem, FileMode, FileSystem,
    HostPath, ProcessRunner, SandboxPath, sandbox_layout,
};
use jackin_image::{BuildStage, ImageEngine};
use jackin_openshell::Gateway;
use jackin_session::{HostPorts, LaunchEvent, LaunchStep, LaunchWarning, Session, SessionDeps};
use jackin_testkit::{
    BuildEffect, ENGINE_PORT, EngineMoment, FakeClock, FakeEngine, FakeGateway, Fault, HostEdit,
    KEY_MARKER, TestRunner, Trigger, default_limits, engine_pem,
};
use tokio_util::sync::CancellationToken;

use super::cap_fs::RecordsCaps;
use super::docker_support::{
    Engine, docker_files, docker_rig, engine_error, env_values, nothing_was_created, padded,
    rule_keys, split_failure, warnings,
};
use super::support::{TestResult, failure, guard, ready, unreachable};

fn parent(path: &SandboxPath) -> &str {
    path.as_str().rsplit_once('/').map_or("", |(dir, _)| dir)
}

#[tokio::test]
async fn disabled_leaves_no_client_rule_file_or_variable_even_with_a_gateway_engine() -> TestResult
{
    guard(async {
        let rig = docker_rig(false, false, Engine::Configured).await?;
        let outcome = rig.launch_observed(1).await?;
        assert_eq!(warnings(&outcome.events), 0);
        ready(outcome)?;
        let handle = rig.handle(1).await?;
        assert!(docker_files(&handle).is_empty());
        let env = env_values(&handle, 1)?;
        assert!(
            env.keys().all(|name| !name.starts_with("DOCKER_")),
            "{env:?}"
        );
        assert!(
            !rule_keys(&rig, &handle)
                .await?
                .contains(&"docker_engine".to_owned())
        );
        assert_eq!(rig.plan(1)?.image_request().docker, DockerLayer::Absent);
        let mut bare = rig.config.clone();
        let text = bare.render();
        let cut: Vec<&str> = text
            .lines()
            .filter(|line| {
                !line.contains("docker_engine")
                    && !line.starts_with("port = ")
                    && !line.starts_with("client_certs")
            })
            .collect();
        bare = jackin_core::Config::parse(&cut.join("\n"))?;
        let with = rig.plan(1)?;
        let without = jackin_session::plan_launch(&bare, &rig.loaded, rig.request(1)?)?;
        let construct = jackin_core::ImageId::parse(&format!("sha256:{}", "1".repeat(64)))?;
        let tag = |plan: &jackin_session::LaunchPlan| {
            jackin_image::ImagePlan::new(plan.image_request()).map(|p| p.final_tag(&construct))
        };
        assert_eq!(tag(&with)?, tag(&without)?);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn each_source_writes_the_certificates_modes_variables_and_rule_and_a_rerun_changes_nothing()
-> TestResult {
    guard(async {
        for (role, workspace) in [(true, false), (false, true), (true, true)] {
            let rig = docker_rig(role, workspace, Engine::WithCerts).await?;
            ready(rig.launch_observed(1).await?)?;
            let handle = rig.handle(1).await?;
            for cert in EngineCert::ALL {
                let path = cert.sandbox_path();
                assert_eq!(handle.file(&path), Some(engine_pem(cert).into_bytes()));
                assert_eq!(handle.file_mode(&path), Some(cert.mode()));
            }
            assert_eq!(
                handle.file_mode(&EngineCert::Key.sandbox_path()),
                Some(FileMode(0o600))
            );
            let env = env_values(&handle, 1)?;
            assert_eq!(
                env["DOCKER_HOST"],
                format!("tcp://host.openshell.internal:{ENGINE_PORT}")
            );
            assert_eq!(env["DOCKER_TLS_VERIFY"], "1");
            for path in docker_files(&handle).keys() {
                assert_eq!(parent(path), env["DOCKER_CERT_PATH"], "{path:?}");
            }
            assert_eq!(rig.plan(1)?.image_request().docker, DockerLayer::Client);
            assert!(
                rule_keys(&rig, &handle)
                    .await?
                    .contains(&"docker_engine".to_owned())
            );
            let modes = |h: &jackin_testkit::SandboxHandle| {
                h.files()
                    .keys()
                    .map(|path| (path.clone(), h.file_mode(path)))
                    .collect::<Vec<_>>()
            };
            let (files, before) = (handle.files(), modes(&handle));
            rig.launch(1).await?;
            let again = rig.handle(1).await?;
            assert_eq!((again.files(), modes(&again)), (files, before));
            assert_eq!(rig.gateway().sandboxes().len(), 1);
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn unusable_certificate_content_is_refused_before_anything_is_created() -> TestResult {
    guard(async {
        let cert = engine_pem(EngineCert::Cert);
        let key = engine_pem(EngineCert::Key);
        let cases: Vec<(EngineCert, Option<Vec<u8>>, EngineCertProblem)> = vec![
            (EngineCert::Key, None, EngineCertProblem::Missing),
            (EngineCert::Key, Some(Vec::new()), EngineCertProblem::Empty),
            (
                EngineCert::Cert,
                Some(padded("CERTIFICATE", 65537)),
                EngineCertProblem::TooLarge,
            ),
            (
                EngineCert::Cert,
                Some(KEY_MARKER.as_bytes().to_vec()),
                EngineCertProblem::NotPem,
            ),
            (
                EngineCert::Key,
                Some(cert.clone().into_bytes()),
                EngineCertProblem::WrongKind,
            ),
            (
                EngineCert::Cert,
                Some(key.clone().into_bytes()),
                EngineCertProblem::WrongKind,
            ),
            (
                EngineCert::Cert,
                Some(format!("{cert}{key}").into_bytes()),
                EngineCertProblem::WrongKind,
            ),
        ];
        for (slot, bytes, expected) in cases {
            let rig = docker_rig(true, false, Engine::WithCerts).await?;
            let path = rig.engine_certs()?.as_path().join(slot.file_name());
            match bytes {
                Some(bytes) => std::fs::write(&path, bytes)?,
                None => std::fs::remove_file(&path)?,
            }
            let outcome = rig.launch_observed(1).await?;
            assert!(outcome.events.is_empty(), "{expected:?}");
            let failed = failure(outcome)?;
            assert_eq!(failed.step, LaunchStep::Credentials);
            assert_eq!(failed.sandbox, None);
            let (file, problem, directory) = engine_error(&failed)?;
            assert_eq!((file, problem), (slot, expected));
            let shown = format!("{} {:?}", failed.error, failed.error);
            assert!(shown.contains(slot.file_name()) && shown.contains(&directory));
            assert!(!shown.contains(KEY_MARKER), "{shown}");
            nothing_was_created(&rig).await?;
        }
        let rig = docker_rig(true, false, Engine::WithCerts).await?;
        let path = rig.engine_certs()?.as_path().join("cert.pem");
        std::fs::write(&path, padded("CERTIFICATE", 65536))?;
        ready(rig.launch_observed(1).await?)?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn directory_symlink_and_symlinked_parent_shapes_are_refused_or_accepted_as_documented()
-> TestResult {
    guard(async {
        for symlink in [false, true] {
            let rig = docker_rig(true, false, Engine::WithCerts).await?;
            let key = rig.engine_certs()?.as_path().join("key.pem");
            let valid = rig.home.as_path().join("other-key.pem");
            std::fs::write(&valid, engine_pem(EngineCert::Key))?;
            std::fs::remove_file(&key)?;
            if symlink {
                std::os::unix::fs::symlink(&valid, &key)?;
            } else {
                std::fs::create_dir(&key)?;
            }
            let outcome = rig.launch_observed(1).await?;
            assert!(outcome.events.is_empty());
            let failed = failure(outcome)?;
            assert_eq!(
                (failed.step, failed.sandbox.clone()),
                (LaunchStep::Credentials, None)
            );
            let (file, problem, _) = engine_error(&failed)?;
            let expected = if symlink {
                EngineCertProblem::Symlink
            } else {
                EngineCertProblem::NotARegularFile
            };
            assert_eq!((file, problem), (EngineCert::Key, expected));
            nothing_was_created(&rig).await?;
        }
        let rig = docker_rig(true, false, Engine::WithCerts).await?;
        let link = rig.home.as_path().join("linked-certs");
        std::os::unix::fs::symlink(rig.engine_certs()?.as_path(), &link)?;
        let text = rig.config.render().replace("engine-certs", "linked-certs");
        let config = jackin_core::Config::parse(&text)?;
        let plan = jackin_session::plan_launch(&config, &rig.loaded, rig.request(1)?)?;
        let outcome = rig
            .run_planned(&rig.world.session, plan, CancellationToken::new())
            .await;
        ready(outcome)?;
        assert_eq!(docker_files(&rig.handle(1).await?).len(), 3);
        Ok(())
    })
    .await
}

fn certificate(body: &str) -> String {
    format!("-----BEGIN CERTIFICATE-----\n{body}\n-----END CERTIFICATE-----\n")
}

#[derive(Clone, Copy)]
enum Swap {
    KeyLink,
    KeyGarbage,
    NewCert,
}

#[tokio::test]
async fn a_certificate_changed_during_the_build_is_refused_at_the_write_and_the_retry_continues()
-> TestResult {
    guard(async {
        let rotated_cert = certificate("TkVXQ0VSVE5FV0NFUlRORVdDRVJU");
        let rows = [
            (
                Swap::KeyLink,
                Some((EngineCert::Key, EngineCertProblem::Symlink)),
            ),
            (
                Swap::KeyGarbage,
                Some((EngineCert::Key, EngineCertProblem::NotPem)),
            ),
            (Swap::NewCert, None),
        ];
        for (swap, expected) in rows {
            let rig = docker_rig(true, false, Engine::WithCerts).await?;
            let directory = rig.engine_certs()?;
            let other = rig.home.as_path().join("other-key.pem");
            std::fs::write(&other, engine_pem(EngineCert::Key))?;
            let (edited, edit) = match swap {
                Swap::KeyLink => (
                    EngineCert::Key,
                    HostEdit::Symlink {
                        path: HostPath::parse(&directory.as_path().join("key.pem"))?,
                        target: HostPath::parse(&other)?,
                    },
                ),
                Swap::KeyGarbage => (
                    EngineCert::Key,
                    HostEdit::Write {
                        path: HostPath::parse(&directory.as_path().join("key.pem"))?,
                        bytes: b"not pem".to_vec(),
                    },
                ),
                Swap::NewCert => (
                    EngineCert::Cert,
                    HostEdit::Write {
                        path: HostPath::parse(&directory.as_path().join("cert.pem"))?,
                        bytes: rotated_cert.clone().into_bytes(),
                    },
                ),
            };
            rig.world.engine.at(EngineMoment::BuildFinished {
                stage: BuildStage::Derived,
                effect: BuildEffect::EditHost(edit),
            });
            let outcome = rig.launch_observed(1).await?;
            let Some(wanted) = expected else {
                ready(outcome)?;
                let handle = rig.handle(1).await?;
                assert_eq!(
                    docker_files(&handle).get(&edited.sandbox_path()),
                    Some(&rotated_cert.clone().into_bytes())
                );
                assert_eq!(docker_files(&handle).len(), 3);
                continue;
            };
            let failed = failure(outcome)?;
            assert_eq!(failed.step, LaunchStep::Credentials);
            let (file, problem, _) = engine_error(&failed)?;
            assert_eq!((file, problem), wanted);
            assert!(failed.sandbox.is_some());
            assert!(docker_files(&rig.handle(1).await?).is_empty());
            let path = directory.as_path().join(edited.file_name());
            std::fs::remove_file(&path)?;
            std::fs::write(&path, engine_pem(edited))?;
            ready(rig.launch_observed(1).await?)?;
            assert_eq!(rig.gateway().sandboxes().len(), 1);
            assert_eq!(docker_files(&rig.handle(1).await?).len(), 3);
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_crash_between_the_certificates_and_the_env_file_never_leaves_docker_host_alone()
-> TestResult {
    guard(async {
        let env = sandbox_layout::env_file(&jackin_testkit::launch_id(1));
        for (trigger, certificates) in [
            (Trigger::Write(EngineCert::Key.sandbox_path()), 2),
            (Trigger::Write(env.clone()), 3),
        ] {
            let rig = docker_rig(true, false, Engine::WithCerts).await?;
            rig.gateway().inject(Fault::FailAt {
                trigger,
                error: unreachable(),
            });
            let failed = failure(rig.launch_observed(1).await?)?;
            assert_eq!(failed.step, LaunchStep::Credentials);
            let handle = rig.handle(1).await?;
            assert_eq!(docker_files(&handle).len(), certificates);
            assert_eq!(handle.file(&env), None);
            let holds_host = handle
                .files()
                .values()
                .any(|bytes| String::from_utf8_lossy(bytes).contains("DOCKER_HOST"));
            assert!(!holds_host);
            ready(rig.launch_observed(1).await?)?;
            let handle = rig.handle(1).await?;
            assert_eq!(docker_files(&handle).len(), 3);
            assert!(env_values(&handle, 1)?.contains_key("DOCKER_HOST"));
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_key_content_reaches_only_the_key_file() -> TestResult {
    guard(async {
        let rig = docker_rig(true, false, Engine::WithCerts).await?;
        let outcome = rig.launch_observed(1).await?;
        let events = format!("{:?}", outcome.events);
        ready(outcome)?;
        let handle = rig.handle(1).await?;
        let holders: Vec<SandboxPath> = handle
            .files()
            .into_iter()
            .filter(|(_, bytes)| String::from_utf8_lossy(bytes).contains(KEY_MARKER))
            .map(|(path, _)| path)
            .collect();
        assert_eq!(holders, [EngineCert::Key.sandbox_path()]);
        let mut surfaces = vec![
            events,
            format!("{:?}", handle.labels()),
            format!("{:?}", handle.environment()),
            format!("{:?}", rig.plan(1)?),
        ];
        let programs = [
            sandbox_layout::PRINTENV.clone(),
            sandbox_layout::hook_path(jackin_core::HookKind::Setup),
            sandbox_layout::hook_path(jackin_core::HookKind::Preflight),
        ];
        for program in &programs {
            surfaces.push(format!("{:?}", handle.runs(program)));
        }
        for surface in surfaces {
            assert!(!surface.contains(KEY_MARKER), "{surface}");
        }
        let swapped = docker_rig(true, false, Engine::WithCerts).await?;
        let cert = swapped.engine_certs()?.as_path().join("cert.pem");
        std::fs::write(&cert, engine_pem(EngineCert::Key))?;
        let (events, failed) = split_failure(swapped.launch_observed(1).await?)?;
        let shown = format!("{events:?} {failed:?} {}", failed.error);
        assert!(!shown.contains(KEY_MARKER), "{shown}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_warning_is_the_first_event_once_per_launch_and_never_for_a_refusal_or_a_disabled_launch()
-> TestResult {
    guard(async {
        let rig = docker_rig(false, true, Engine::WithCerts).await?;
        let first = rig.launch_observed(1).await?;
        assert_eq!(
            first.events.first(),
            Some(&LaunchEvent::Warning(LaunchWarning::DockerCapability))
        );
        assert_eq!(
            first.events.get(1),
            Some(&LaunchEvent::StepStarted(LaunchStep::Image))
        );
        assert_eq!(warnings(&first.events), 1);
        let started = first
            .events
            .iter()
            .filter(|event| matches!(event, LaunchEvent::StepStarted(_)))
            .count();
        assert_eq!(started, 9);
        ready(first)?;
        let again = rig.launch_observed(1).await?;
        assert_eq!(warnings(&again.events), 1);
        assert_eq!(
            again.events.first(),
            Some(&LaunchEvent::Warning(LaunchWarning::DockerCapability))
        );
        let off = docker_rig(false, false, Engine::WithCerts).await?;
        assert_eq!(warnings(&off.launch_observed(1).await?.events), 0);
        let cancelled = CancellationToken::new();
        cancelled.cancel();
        let early = docker_rig(false, true, Engine::WithCerts).await?;
        let outcome = early.run(early.request(1)?, cancelled).await?;
        assert_eq!(warnings(&outcome.events), 0);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_missing_certificate_directory_is_a_typed_error_that_names_the_directory() -> TestResult {
    guard(async {
        let rig = docker_rig(true, false, Engine::WithCerts).await?;
        let directory = rig.engine_certs()?;
        std::fs::remove_dir_all(directory.as_path())?;
        let outcome = rig.launch_observed(1).await?;
        assert!(outcome.events.is_empty());
        let failed = failure(outcome)?;
        assert_eq!(
            (failed.step, failed.sandbox.clone()),
            (LaunchStep::Credentials, None)
        );
        let (file, problem, named) = engine_error(&failed)?;
        assert_eq!(
            (file, problem),
            (EngineCert::Ca, EngineCertProblem::Missing)
        );
        assert_eq!(named, directory.as_path().display().to_string());
        nothing_was_created(&rig).await?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn every_certificate_read_asks_the_filesystem_for_no_more_than_the_size_cap() -> TestResult {
    guard(async {
        let rig = docker_rig(false, true, Engine::WithCerts).await?;
        let recorder = Arc::new(RecordsCaps::new(Arc::clone(&rig.world.fs)));
        let fs: Arc<dyn FileSystem> = Arc::<RecordsCaps>::clone(&recorder);
        let world = &rig.world;
        let gateway: Arc<dyn Gateway> = Arc::<FakeGateway>::clone(&world.gateway);
        let engine: Arc<dyn ImageEngine> = Arc::<FakeEngine>::clone(&world.engine);
        let clock: Arc<dyn Clock> = Arc::<FakeClock>::clone(&world.clock);
        let runner: Arc<dyn ProcessRunner> = Arc::<TestRunner>::clone(&world.runner);
        let session = Session::new(SessionDeps {
            gateway,
            engine,
            host: HostPorts { clock, fs, runner },
            cache_dir: world.cache_dir().clone(),
            config: world.config.clone(),
            limits: default_limits(),
        });
        let outcome = rig
            .run_on(&session, rig.request(1)?, CancellationToken::new())
            .await?;
        ready(outcome)?;

        let names: BTreeSet<&str> = EngineCert::ALL
            .iter()
            .map(|cert| cert.file_name())
            .collect();
        let reads: Vec<(String, usize)> = recorder
            .seen()
            .into_iter()
            .filter(|(file, _)| names.contains(file.as_str()))
            .collect();
        let read_names: BTreeSet<&str> = reads.iter().map(|(file, _)| file.as_str()).collect();
        assert_eq!(read_names, names, "{reads:?}");
        for (file, max) in &reads {
            assert!(*max <= ENGINE_CERT_MAX_BYTES + 1, "{file}: {max}");
        }
        Ok(())
    })
    .await
}
