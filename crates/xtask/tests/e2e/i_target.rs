// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::future::poll_fn;
use std::path::Path;
use std::process::{Command, Output};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::Poll;

use jackin_xtask::{
    AgentBehaviorSpec, AnswerMatcher, BehaviorCategory, E2eArgs, E2eCredential, E2eTarget,
    TargetError, UsageBehavior, UsageWindow, e2e_command, t01_behavior,
};
use serde_json::{Value, json};

use super::scenario::world::{ACCESS, ID_TOKEN, REFRESH};
use super::scenario::{AgeReply, Ran, Scenario};
use super::support::capture;

fn args() -> E2eArgs {
    E2eArgs {
        codex_home: Some(Path::new("/home/test/.codex").to_path_buf()),
        ..E2eArgs::default()
    }
}

fn resolve_with(change: impl FnOnce(&mut AgentBehaviorSpec, &mut E2eArgs)) -> E2eTarget {
    let mut behavior = t01_behavior();
    let mut args = args();
    change(&mut behavior, &mut args);
    E2eTarget::resolve_with_behavior(&args, Some(Path::new("/home/test")), Some(behavior))
        .expect("valid target")
}

fn assert_t01(target: &E2eTarget, model: &str, home: &Path) {
    assert_eq!(target.tuple().id, "T01");
    assert_eq!(target.tuple().agent, jackin_core::Agent::Codex);
    assert_eq!(target.tuple().provider, jackin_core::Provider::Codex);
    assert_eq!(target.model().as_str(), model);
    assert_eq!(
        target.credential(),
        &E2eCredential::CodexLogin {
            home: home.to_path_buf()
        }
    );
    assert_eq!(target.identity().adapter, "t01-codex-v1");
    let behavior = target.behavior();
    assert_eq!(behavior.version.binary, "/usr/local/bin/codex");
    assert_eq!(behavior.process.comm, "codex");
    assert!(matches!(
        behavior.prompt.answer,
        AnswerMatcher::WordCaseInsensitive { .. }
    ));
    assert_eq!(behavior.terminal.quit_interrupts, 2);
    assert_eq!(behavior.credential.auth_file.required_state, "present");
    assert_eq!(behavior.reflection.descendant_comm, "codex");
    assert_eq!(behavior.provider.accepted_status, Some(401));
    assert!(matches!(behavior.usage, UsageBehavior::Windows { .. }));
}

#[test]
fn t01_resolution_defaults_partial_selection_and_complete_behavior() {
    let default = E2eTarget::resolve(&args(), Some(Path::new("/home/test"))).expect("default");
    assert_t01(&default, "gpt-6.1-sol", Path::new("/home/test/.codex"));
    for (agent, provider, model) in [
        (Some("codex"), None, None),
        (None, Some("codex"), None),
        (None, None, Some("gpt-5.1-codex")),
        (Some("codex"), Some("codex"), None),
    ] {
        let mut selected = args();
        selected.agent = agent.map(str::to_owned);
        selected.provider = provider.map(str::to_owned);
        selected.model = model.map(str::to_owned);
        let target = E2eTarget::resolve(&selected, Some(Path::new("/home/test"))).expect("T01");
        assert_t01(
            &target,
            model.unwrap_or("gpt-6.1-sol"),
            Path::new("/home/test/.codex"),
        );
        assert_eq!(
            target.selection_args(),
            default
                .selection_args()
                .into_iter()
                .enumerate()
                .map(|(index, value)| {
                    if index == 5 && model.is_some() {
                        "gpt-5.1-codex".to_owned()
                    } else {
                        value
                    }
                })
                .collect::<Vec<_>>()
        );
    }
    let home_default = E2eTarget::resolve(
        &E2eArgs {
            codex_home: None,
            ..args()
        },
        Some(Path::new("/home/test")),
    )
    .expect("HOME default");
    assert_t01(&home_default, "gpt-6.1-sol", Path::new("/home/test/.codex"));
    let override_target = E2eTarget::resolve(
        &E2eArgs {
            codex_home: Some(Path::new("/other/login").to_path_buf()),
            ..args()
        },
        Some(Path::new("/home/test")),
    )
    .expect("override");
    assert_t01(&override_target, "gpt-6.1-sol", Path::new("/other/login"));
}

#[test]
fn target_resolution_rejects_every_unavailable_or_incomplete_contract() {
    let cases: Vec<(&str, E2eArgs, &str)> = vec![
        (
            "unknown pair",
            E2eArgs {
                agent: Some("grok".to_owned()),
                provider: Some("kimi".to_owned()),
                ..args()
            },
            "no catalog tuple",
        ),
        (
            "non-supported pair",
            E2eArgs {
                agent: Some("claude".to_owned()),
                provider: Some("claude".to_owned()),
                ..args()
            },
            "tuple T02 is unavailable",
        ),
        (
            "supported without adapter",
            E2eArgs {
                agent: Some("omp".to_owned()),
                provider: Some("kimi".to_owned()),
                ..args()
            },
            "tuple T10 has no complete E2E adapter",
        ),
        (
            "invalid T01 model",
            E2eArgs {
                model: Some("not-a-model".to_owned()),
                ..args()
            },
            "not allowed",
        ),
    ];
    for (name, selection, expected) in cases {
        let error = E2eTarget::resolve(&selection, Some(Path::new("/home/test")))
            .expect_err(name)
            .to_string();
        assert!(error.contains(expected), "{name}: {error}");
    }
    let categories = [
        BehaviorCategory::Version,
        BehaviorCategory::Process,
        BehaviorCategory::Prompt,
        BehaviorCategory::Terminal,
        BehaviorCategory::Credential,
        BehaviorCategory::Reflection,
        BehaviorCategory::Provider,
        BehaviorCategory::Usage,
    ];
    for category in categories {
        let mut behavior = t01_behavior();
        match category {
            BehaviorCategory::Version => behavior.version = None,
            BehaviorCategory::Process => behavior.process = None,
            BehaviorCategory::Prompt => behavior.prompt = None,
            BehaviorCategory::Terminal => behavior.terminal = None,
            BehaviorCategory::Credential => behavior.credential = None,
            BehaviorCategory::Reflection => behavior.reflection = None,
            BehaviorCategory::Provider => behavior.provider = None,
            BehaviorCategory::Usage => behavior.usage = None,
        }
        let error = E2eTarget::resolve_with_behavior(
            &args(),
            Some(Path::new("/home/test")),
            Some(behavior),
        )
        .expect_err("incomplete behavior");
        assert!(
            matches!(error, TargetError::IncompleteBehavior { category: found, .. } if found == category),
            "{error}"
        );
    }
    for source in [
        "env:ZAI_API_KEY",
        "file:/tmp/key",
        "op://vault/key",
        "omp-host",
    ] {
        let selection = E2eArgs {
            credential: Some(source.to_owned()),
            ..args()
        };
        assert!(matches!(
            E2eTarget::resolve(&selection, Some(Path::new("/home/test"))),
            Err(TargetError::CredentialSource { .. })
        ));
    }
    for (provider, reflection) in [(true, false), (false, true)] {
        let error = resolve_error(|behavior, _| {
            if provider {
                behavior.provider.as_mut().expect("provider").probe_url =
                    "https://outside.invalid/v1".to_owned();
            }
            if reflection {
                behavior.reflection.as_mut().expect("reflection").url =
                    "https://outside.invalid/reflect".to_owned();
            }
        });
        assert!(
            matches!(error, TargetError::OutsideProfile { .. }),
            "{error}"
        );
    }
}

fn resolve_error(change: impl FnOnce(&mut AgentBehaviorSpec, &mut E2eArgs)) -> TargetError {
    let mut behavior = t01_behavior();
    let mut args = args();
    change(&mut behavior, &mut args);
    E2eTarget::resolve_with_behavior(&args, Some(Path::new("/home/test")), Some(behavior))
        .expect_err("invalid behavior")
}

#[tokio::test]
async fn selection_errors_precede_every_secret_gateway_scratch_and_image_effect() {
    for (name, agent, provider, model, expected) in [
        (
            "unknown",
            Some("grok"),
            Some("kimi"),
            None,
            "no catalog tuple",
        ),
        (
            "blocked",
            Some("claude"),
            Some("claude"),
            None,
            "unavailable",
        ),
        (
            "adapter",
            Some("omp"),
            Some("kimi"),
            None,
            "no complete E2E adapter",
        ),
        ("model", None, None, Some("not-a-model"), "not allowed"),
    ] {
        let mut scenario = Scenario::new().await;
        scenario.args.agent = agent.map(str::to_owned);
        scenario.args.provider = provider.map(str::to_owned);
        scenario.args.model = model.map(str::to_owned);
        std::fs::remove_file(scenario.world.codex_home.join("auth.json")).expect("remove login");
        let ran = scenario.run().await;
        assert_eq!(ran.status.code(), 2, "{name}: {}", ran.stderr);
        assert!(ran.stderr.contains(expected), "{name}: {}", ran.stderr);
        assert!(!ran.stderr.contains("A1 has no usable source"), "{name}");
        assert!(ran.stdout.is_empty(), "{name}: nothing ran");
        let state = scenario.state();
        assert_eq!(state.builds, 0, "{name}");
        assert!(state.commands.is_empty(), "{name}");
        assert!(state.accounts.is_empty(), "{name}");
        assert!(state.fresh_replies.is_empty(), "{name}");
        assert!(state.probe_specs.is_empty(), "{name}");
        assert_eq!(state.health_calls, 0, "{name}");
        drop(state);
        assert!(!scenario.world.scratch_root.exists(), "{name}");
        assert_eq!(
            std::fs::read_dir(&scenario.world.lock_dir)
                .expect("lock directory")
                .count(),
            0,
            "{name}: no lock entry was created"
        );
    }
}

#[test]
fn canonical_digest_matches_independent_fixture_and_excludes_runtime_values() {
    let target = E2eTarget::t01().expect("T01 target");
    assert_eq!(
        target.identity().spec_sha256,
        "ccaea79cdf09b5949ea94266c33692bec52090e1782364c53e13a8f731d1806a"
    );
    let other_home = E2eTarget::resolve(
        &E2eArgs {
            codex_home: Some(Path::new("/another/home").to_path_buf()),
            ..args()
        },
        Some(Path::new("/different/host")),
    )
    .expect("other home");
    assert_eq!(target.identity(), other_home.identity());
    let first = target.probe_spec_bytes().expect("spec");
    let second = target.probe_spec_bytes().expect("idempotent spec");
    assert_eq!(first, second);
    let other_first = other_home.probe_spec_bytes().expect("other home spec");
    assert_eq!(other_first, other_home.probe_spec_bytes().expect("again"));
    assert_eq!(first, other_first);
    for (name, bytes) in [("default", &first), ("other home", &other_first)] {
        let text = String::from_utf8(bytes.clone()).expect("UTF-8");
        for excluded in [
            ACCESS,
            REFRESH,
            ID_TOKEN,
            "op://",
            "op://vault/api-key",
            "env:",
            "omp-host",
            "CodexLogin",
            "account-name",
            "actual-usage-value",
            "/tmp/.codex",
            "/another/home",
            "/different/host",
        ] {
            assert!(!text.contains(excluded), "{name} spec contains {excluded}");
        }
        let value: Value = serde_json::from_str(&text).expect("spec JSON");
        let keys = |value: &Value| -> Vec<String> {
            let mut keys: Vec<String> =
                value.as_object().expect("object").keys().cloned().collect();
            keys.sort();
            keys
        };
        assert_eq!(keys(&value), ["behavior", "provider_hosts", "target"]);
        assert_eq!(
            keys(&value["behavior"]),
            [
                "credential",
                "process",
                "prompt",
                "provider",
                "reflection",
                "terminal",
                "usage",
                "version"
            ]
        );
        assert_eq!(
            keys(&value["target"]),
            [
                "adapter",
                "agent",
                "model",
                "provider",
                "spec_sha256",
                "tuple"
            ]
        );
        assert_eq!(
            value["provider_hosts"],
            json!([
                "api.openai.com",
                "auth.openai.com",
                "chatgpt.com",
                "ab.chatgpt.com"
            ]),
            "{name}"
        );
        assert_eq!(value["provider_hosts"], json!(target.provider_hosts()));
        assert!(value["behavior"]["prompt"]["argv"].is_array());
        assert_eq!(
            value["behavior"]["prompt"]["answer"],
            json!({"word_case_insensitive": {"word": "ready"}})
        );
        assert_eq!(
            value["target"],
            serde_json::to_value(target.identity()).expect("identity")
        );
        assert_eq!(
            value["behavior"],
            serde_json::to_value(target.behavior()).expect("behavior")
        );
    }
}

#[test]
fn every_behavior_and_identity_input_changes_the_digest() {
    let base = resolve_with(|_, _| {});
    type Mutate = fn(&mut AgentBehaviorSpec, &mut E2eArgs);
    let cases: Vec<(&str, Mutate)> = vec![
        ("model", |_, args| {
            args.model = Some("gpt-5.1-codex".to_owned());
        }),
        ("binary", |behavior, _| {
            behavior.version.as_mut().expect("version").binary = "/usr/local/bin/other".to_owned();
        }),
        ("version", |behavior, _| {
            behavior.version.as_mut().expect("version").exact_output = "other".to_owned();
        }),
        ("comm", |behavior, _| {
            behavior.process.as_mut().expect("process").comm = "other".to_owned();
        }),
        ("minimum age", |behavior, _| {
            behavior
                .process
                .as_mut()
                .expect("process")
                .minimum_age_seconds += 1;
        }),
        ("argv", |behavior, _| {
            behavior
                .prompt
                .as_mut()
                .expect("prompt")
                .argv
                .push("--other".to_owned());
        }),
        ("answer", |behavior, _| {
            behavior.prompt.as_mut().expect("prompt").answer = AnswerMatcher::WordCaseInsensitive {
                word: "other".to_owned(),
            };
        }),
        ("terminal", |behavior, _| {
            behavior.terminal.as_mut().expect("terminal").ready_text = "other".to_owned();
        }),
        ("quit", |behavior, _| {
            behavior
                .terminal
                .as_mut()
                .expect("terminal")
                .quit_interrupts = 1;
        }),
        ("credential env", |behavior, _| {
            behavior
                .credential
                .as_mut()
                .expect("credential")
                .environment
                .push(jackin_xtask::EnvironmentBehavior {
                    name: "OTHER_API_KEY".to_owned(),
                    kind: jackin_xtask::EnvironmentKind::Placeholder,
                });
        }),
        ("auth file", |behavior, _| {
            behavior
                .credential
                .as_mut()
                .expect("credential")
                .auth_file
                .mode = Some("400".to_owned());
        }),
        ("provider status", |behavior, _| {
            behavior
                .provider
                .as_mut()
                .expect("provider")
                .accepted_status = Some(403);
        }),
        ("provider body", |behavior, _| {
            behavior.provider.as_mut().expect("provider").accepted_body =
                Some(jackin_xtask::ProviderBodyMatcher::JsonErrorCode {
                    code: "other".to_owned(),
                });
        }),
        ("provider URL", |behavior, _| {
            behavior.provider.as_mut().expect("provider").probe_url =
                "https://chatgpt.com/backend-api/other".to_owned();
        }),
        ("reflection", |behavior, _| {
            behavior.reflection.as_mut().expect("reflection").url =
                "https://chatgpt.com/backend-api/other-reflect".to_owned();
        }),
        ("usage", |behavior, _| {
            behavior.usage = Some(UsageBehavior::Windows {
                required: vec![UsageWindow::Weekly],
                optional: vec![UsageWindow::Credits],
            });
        }),
    ];
    for (name, change) in cases {
        let changed = resolve_with(change);
        assert_ne!(
            base.identity().spec_sha256,
            changed.identity().spec_sha256,
            "{name}"
        );
    }
    let hosts = E2eTarget::resolve_with_behavior_and_hosts(
        &args(),
        Some(Path::new("/home/test")),
        Some(t01_behavior()),
        Some(vec![
            "api.openai.com".to_owned(),
            "auth.openai.com".to_owned(),
            "chatgpt.com".to_owned(),
        ]),
    )
    .expect("different parsed hosts");
    assert_ne!(base.identity().spec_sha256, hosts.identity().spec_sha256);
}

#[tokio::test]
async fn j12_normal_and_shell_probes_share_the_private_invocation() {
    let scenario = Scenario::new().await;
    let ran = scenario.run().await;
    ran.expect("J12 invocation control", &[]);
    let state = scenario.state();
    let normal: Vec<&Vec<String>> = state
        .probe_argvs
        .iter()
        .filter(|args| args.contains(&"J12".to_owned()))
        .collect();
    assert_eq!(normal.len(), 2, "{normal:?}");
    for args in normal {
        assert_eq!(
            args,
            &[
                "--spec".to_owned(),
                "/sandbox/.jackin/e2e/probe-spec.json".to_owned(),
                "--docker".to_owned(),
                "off".to_owned(),
                "--only".to_owned(),
                "J12".to_owned(),
            ]
        );
    }
    let shell: Vec<&String> = state
        .shell_commands
        .iter()
        .filter(|command| command.contains("J12"))
        .collect();
    assert_eq!(shell.len(), 1);
    assert_eq!(
        shell[0],
        "'/usr/local/bin/jackin-sentinel-probe' '--spec' '/sandbox/.jackin/e2e/probe-spec.json' '--docker' 'off' '--only' 'J12'"
    );
    assert!(!state.probe_specs.is_empty());
    assert!(
        state
            .probe_specs
            .iter()
            .all(|spec| spec == &state.probe_specs[0])
    );
}

#[tokio::test]
async fn j12_rejects_shell_target_digest_and_engine_mismatches() {
    for (name, pointer, value, expected) in [
        ("target", "/target/model", json!("other"), "target model is"),
        ("tuple", "/target/tuple", json!("T99"), "target tuple is"),
        ("agent", "/target/agent", json!("omp"), "target agent is"),
        (
            "provider",
            "/target/provider",
            json!("zai"),
            "target provider is",
        ),
        (
            "adapter",
            "/target/adapter",
            json!("other-adapter"),
            "target adapter is",
        ),
        (
            "digest",
            "/target/spec_sha256",
            json!("0".repeat(64)),
            "spec_sha256",
        ),
        (
            "engine",
            "/engine",
            json!("host.openshell.internal:2376"),
            "engine",
        ),
    ] {
        let scenario = Scenario::new().await;
        scenario.patch_report("J12", Some(2), pointer, value);
        let ran = scenario.run().await;
        ran.expect(name, &[("J12", "FAIL")]);
        ran.reason_has(name, "J12", expected);
    }
}

#[tokio::test]
async fn j15_docker_shell_probe_uses_the_private_invocation_with_docker_and_engine() {
    let scenario = Scenario::new().await;
    let ran = scenario.run().await;
    ran.expect("J15 invocation control", &[]);
    let state = scenario.state();
    let shell: Vec<&String> = state
        .shell_commands
        .iter()
        .filter(|command| command.contains("J15"))
        .collect();
    assert_eq!(shell.len(), 1, "{shell:?}");
    assert_eq!(
        shell[0],
        "'/usr/local/bin/jackin-sentinel-probe' '--spec' '/sandbox/.jackin/e2e/probe-spec.json' '--docker' 'on' '--engine' 'host.openshell.internal:2376' '--only' 'J04,J15'"
    );
    assert!(!state.probe_specs.is_empty());
    assert!(
        state
            .probe_specs
            .iter()
            .all(|spec| spec == &state.probe_specs[0])
    );
}

#[tokio::test]
async fn j15_rejects_shell_target_digest_engine_mode_and_docker_mismatches() {
    for (name, pointer, value, expected) in [
        ("target", "/target/model", json!("other"), "target model is"),
        ("tuple", "/target/tuple", json!("T99"), "target tuple is"),
        ("agent", "/target/agent", json!("omp"), "target agent is"),
        (
            "provider",
            "/target/provider",
            json!("zai"),
            "target provider is",
        ),
        (
            "adapter",
            "/target/adapter",
            json!("other-adapter"),
            "target adapter is",
        ),
        (
            "digest",
            "/target/spec_sha256",
            json!("0".repeat(64)),
            "spec_sha256",
        ),
        (
            "engine",
            "/engine",
            json!("host.openshell.internal:9999"),
            "engine is",
        ),
        ("mode", "/mode", json!("host"), "mode is host"),
        ("docker", "/docker", json!("off"), "docker is off"),
    ] {
        let scenario = Scenario::new().await;
        scenario.patch_report("J15", None, pointer, value);
        let ran = scenario.run().await;
        ran.expect(name, &[("J15", "FAIL"), ("CLEAN", "FAIL")]);
        ran.reason_has(name, "J15", expected);
    }
}

#[tokio::test]
async fn j04_deadline_rejects_every_unqualified_or_young_child() {
    for (name, reply) in [
        ("no child", AgeReply::NoChild),
        ("wrong comm", AgeReply::WrongComm),
        ("wrong parent", AgeReply::WrongParent),
        ("child remains young", AgeReply::Young),
    ] {
        let mut scenario = Scenario::new().await;
        scenario.budgets.exec = std::time::Duration::from_millis(30);
        scenario.budgets.poll = std::time::Duration::from_millis(1);
        scenario.knobs().age_reply = Some(reply);
        let ran = scenario.run().await;
        ran.expect(
            name,
            &[
                ("J04", "FAIL"),
                ("J05", "FAIL"),
                ("J06", "FAIL"),
                ("J07", "FAIL"),
                ("J08", "FAIL"),
                ("J09", "FAIL"),
                ("J10", "FAIL"),
                ("J11", "SKIP"),
                ("J12", "SKIP"),
            ],
        );
        ran.reason_has(name, "J04", "before the J04 deadline");
    }
}

#[tokio::test]
async fn j00_and_j03_reject_build_and_load_identity_faults() {
    let build = Scenario::new().await;
    build.knobs().target_args_fault = Some("build".to_owned());
    let ran = build.run().await;
    ran.expect(
        "build identity",
        &[
            ("J00", "FAIL"),
            ("J03", "SKIP"),
            ("J04", "SKIP"),
            ("J05", "SKIP"),
            ("J06", "SKIP"),
            ("J07", "SKIP"),
            ("J08", "SKIP"),
            ("J09", "SKIP"),
            ("J10", "SKIP"),
            ("J11", "SKIP"),
            ("J12", "SKIP"),
            ("J13", "SKIP"),
            ("J14", "SKIP"),
            ("J15", "SKIP"),
            ("J16", "SKIP"),
            ("J17", "SKIP"),
        ],
    );
    ran.reason_has("build identity", "J00", "first build failed");

    let load = Scenario::new().await;
    load.knobs().target_args_fault = Some("load".to_owned());
    let ran = load.run().await;
    ran.expect(
        "load identity",
        &[
            ("J00", "SKIP"),
            ("J03", "FAIL"),
            ("J04", "SKIP"),
            ("J05", "SKIP"),
            ("J06", "SKIP"),
            ("J07", "SKIP"),
            ("J08", "SKIP"),
            ("J09", "SKIP"),
            ("J10", "SKIP"),
            ("J11", "SKIP"),
            ("J12", "SKIP"),
            ("J13", "SKIP"),
            ("J14", "SKIP"),
            ("J15", "FAIL"),
            ("J16", "SKIP"),
            ("J17", "FAIL"),
        ],
    );
}

const SIBLINGS: [(&str, &str); 2] = [
    ("ZAI_API_KEY", "sibling-zai-secret-0123456789"),
    ("KIMI_API_KEY", "sibling-kimi-secret-0123456789"),
];

fn files_under(root: &Path) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                found.push((
                    path.display().to_string(),
                    String::from_utf8_lossy(&std::fs::read(&path).unwrap_or_default()).into_owned(),
                ));
            }
        }
    }
    found
}

fn assert_siblings_absent(case: &str, scenario: &Scenario, ran: &Ran) {
    let mut haystacks = vec![
        ("stdout".to_owned(), ran.stdout.clone()),
        ("stderr".to_owned(), ran.stderr.clone()),
    ];
    haystacks.extend(files_under(&scenario.world.root));
    {
        let state = scenario.state();
        for spec in &state.probe_specs {
            haystacks.push((
                "probe spec".to_owned(),
                String::from_utf8_lossy(spec).into_owned(),
            ));
        }
        haystacks.push(("host commands".to_owned(), state.commands.join("\n")));
        for env in &state.fresh_reply_envs {
            for (name, value) in SIBLINGS {
                assert!(!env.contains_key(name), "{case}: child env carries {name}");
                assert!(
                    !env.values().any(|found| found == value),
                    "{case}: child env"
                );
            }
        }
    }
    for (place, text) in &haystacks {
        for (name, value) in SIBLINGS {
            assert!(!text.contains(name), "{case}: {place} names {name}");
            assert!(
                !text.contains(value),
                "{case}: {place} carries {name}'s value"
            );
        }
    }
}

#[tokio::test]
async fn fresh_host_replies_are_distinct_exact_and_required() {
    let plain = Scenario::new().await;
    let ran = plain.run_with_env(&SIBLINGS).await;
    assert_eq!(ran.status.code(), 0, "{} {}", ran.stdout, ran.stderr);
    {
        let state = plain.state();
        assert_eq!(
            state.fresh_replies,
            vec![
                "e2e-fresh-before-abcd".to_owned(),
                "e2e-fresh-after-abcd".to_owned(),
            ]
        );
        assert_ne!(state.fresh_replies[0], state.fresh_replies[1]);
        for env in &state.fresh_reply_envs {
            assert_eq!(env.len(), 3);
            assert!(env.contains_key("HOME"));
            assert!(env.contains_key("PATH"));
            assert!(env.contains_key("CODEX_HOME"));
        }
        assert!(!state.probe_specs.is_empty());
    }
    assert_siblings_absent("successful run", &plain, &ran);

    for phase in ["before", "after"] {
        let other = if phase == "before" { "after" } else { "before" };
        let expected = format!("e2e-fresh-{phase}-abcd");
        let cases = [
            (
                "wrong text with exit 0",
                format!("{ACCESS}\n"),
                0,
                String::new(),
            ),
            ("empty text with exit 0", String::new(), 0, String::new()),
            (
                "the expected text behind a prefix with exit 0",
                format!("X{expected}\n"),
                0,
                String::new(),
            ),
            (
                "the expected text before a suffix with exit 0",
                format!("{expected}X\n"),
                0,
                String::new(),
            ),
            (
                "the expected text inside other lines with exit 0",
                format!("noise\n{expected}\nnoise\n"),
                0,
                String::new(),
            ),
            (
                "the other phase's reply with exit 0",
                format!("e2e-fresh-{other}-abcd\n"),
                0,
                String::new(),
            ),
            (
                "exact text with a nonzero exit",
                format!("e2e-fresh-{phase}-abcd\n"),
                1,
                ACCESS.to_owned(),
            ),
        ];
        for (case, stdout, exit, stderr) in cases {
            let case = format!("{phase}: {case}");
            let scenario = Scenario::new().await;
            scenario.host_reply_rule(&format!("fresh-{phase}"), None, exit, &stdout, &stderr);
            let ran = scenario.run_with_env(&SIBLINGS).await;
            if phase == "before" {
                assert_eq!(
                    ran.status.code(),
                    2,
                    "{case}: {} {}",
                    ran.stdout,
                    ran.stderr
                );
                assert!(ran.stdout.is_empty(), "{case}");
                assert!(
                    ran.stderr.contains("fresh host Codex reply before failed"),
                    "{case}: {}",
                    ran.stderr
                );
                assert_eq!(scenario.state().builds, 0, "{case}");
            } else {
                assert_eq!(
                    ran.status.code(),
                    1,
                    "{case}: {} {}",
                    ran.stdout,
                    ran.stderr
                );
                assert!(
                    ran.stdout.contains("HOST_REPLY FAIL"),
                    "{case}: {}",
                    ran.stdout
                );
                assert!(ran.stdout.contains("a1_host_login=unchanged"), "{case}");
            }
            for secret in [ACCESS, REFRESH, ID_TOKEN] {
                assert!(!ran.stdout.contains(secret), "{case}: stdout leaks");
                assert!(!ran.stderr.contains(secret), "{case}: stderr leaks");
            }
            assert_siblings_absent(&case, &scenario, &ran);
        }
    }
}

#[tokio::test]
async fn the_refresh_lock_is_held_through_both_fresh_replies_and_stops_a_contended_run() {
    let scenario = Scenario::new().await;
    let ran = scenario.run().await;
    assert_eq!(ran.status.code(), 0, "{} {}", ran.stdout, ran.stderr);
    assert_eq!(scenario.state().fresh_reply_lock_refused, vec![true, true]);
    assert!(
        ran.stdout.contains("a1_host_login=unchanged"),
        "{}",
        ran.stdout
    );
    assert!(
        jackin_xtask::A1Lock::acquire(&scenario.world.lock_dir).is_ok(),
        "the lock is released once the run ends"
    );

    let contended = Scenario::new().await;
    std::fs::write(contended.world.codex_home.join("auth.json"), "not a login").expect("corrupt");
    let held = jackin_xtask::A1Lock::acquire(&contended.world.lock_dir).expect("another holder");
    let ran = contended.run().await;
    assert_eq!(ran.status.code(), 2, "{} {}", ran.stdout, ran.stderr);
    assert!(ran.stderr.contains("holds the A1 lock"), "{}", ran.stderr);
    assert!(!ran.stderr.contains("no usable source"), "{}", ran.stderr);
    assert!(ran.stdout.is_empty(), "{}", ran.stdout);
    {
        let state = contended.state();
        assert!(state.fresh_replies.is_empty());
        assert!(state.commands.is_empty(), "{:?}", state.commands);
        assert_eq!(state.builds, 0);
        assert_eq!(state.loads, 0);
        assert_eq!(state.health_calls, 0);
        assert!(state.probe_specs.is_empty());
        assert!(state.accounts.is_empty());
        drop(state);
        assert!(!contended.world.scratch_root.exists());
    }
    drop(held);
}

struct ProbeRun {
    code: u8,
    polls: usize,
    stdout: String,
    stderr: String,
    scratch: bool,
}

async fn command_with_poll_probe(selection: &E2eArgs) -> ProbeRun {
    let mut scenario = Scenario::new().await;
    scenario.args.agent.clone_from(&selection.agent);
    scenario.args.provider.clone_from(&selection.provider);
    scenario.args.model.clone_from(&selection.model);
    scenario.args.credential.clone_from(&selection.credential);
    scenario.state().tag.clone_from(&scenario.tag);
    let args = scenario.args.clone();
    let (boundaries, out, err) = scenario.boundaries();
    let mut boundaries = Some(boundaries);
    let polls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&polls);
    let future = poll_fn(move |_| {
        counted.fetch_add(1, Ordering::SeqCst);
        Poll::Ready(Ok::<_, jackin_xtask::ProductionError>(
            boundaries.take().expect("one production boundary"),
        ))
    });
    let code = e2e_command(&args, future).await;
    ProbeRun {
        code,
        polls: polls.load(Ordering::SeqCst),
        stdout: out.text(),
        stderr: err.text(),
        scratch: scenario.world.scratch_root.exists(),
    }
}

fn public_output(selection: &E2eArgs) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_jackin-xtask"));
    command
        .arg("e2e")
        .args([
            "--gateway",
            "jackin-local",
            "--role",
            "/nonexistent-role",
            "--jackin",
            "/bin/echo",
            "--codex-home",
            "/nonexistent-codex",
        ])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/nonexistent-home");
    for (flag, value) in [
        ("--agent", &selection.agent),
        ("--provider", &selection.provider),
        ("--model", &selection.model),
        ("--credential", &selection.credential),
    ] {
        if let Some(value) = value {
            command.arg(flag).arg(value);
        }
    }
    capture(&mut command)
}

#[tokio::test]
async fn invalid_targets_never_poll_production_and_valid_t01_polls_once() {
    for (name, selection, expected) in [
        (
            "unknown pair",
            E2eArgs {
                agent: Some("grok".to_owned()),
                provider: Some("kimi".to_owned()),
                ..args()
            },
            "no catalog tuple",
        ),
        (
            "unsupported pair",
            E2eArgs {
                agent: Some("claude".to_owned()),
                provider: Some("claude".to_owned()),
                ..args()
            },
            "tuple T02 is unavailable",
        ),
        (
            "supported pair without adapter",
            E2eArgs {
                agent: Some("omp".to_owned()),
                provider: Some("kimi".to_owned()),
                ..args()
            },
            "tuple T10 has no complete E2E adapter",
        ),
        (
            "invalid model",
            E2eArgs {
                model: Some("not-a-model".to_owned()),
                ..args()
            },
            "not allowed",
        ),
        (
            "malformed credential",
            E2eArgs {
                credential: Some("op://".to_owned()),
                ..args()
            },
            "--credential expects env:NAME",
        ),
    ] {
        let error = E2eTarget::resolve(&selection, Some(Path::new("/home/test"))).expect_err(name);
        match name {
            "unknown pair" | "invalid model" => {
                assert!(matches!(error, TargetError::Catalog(_)), "{name}");
            }
            "unsupported pair" => {
                assert!(
                    matches!(error, TargetError::TupleUnavailable { .. }),
                    "{name}"
                );
            }
            "supported pair without adapter" => {
                assert!(
                    matches!(error, TargetError::AdapterUnavailable { .. }),
                    "{name}"
                );
            }
            "malformed credential" => {
                assert!(matches!(error, TargetError::CredentialDescriptor), "{name}");
            }
            _ => unreachable!(),
        }
        assert!(error.to_string().contains(expected), "{name}: {error}");
        let ran = command_with_poll_probe(&selection).await;
        assert_eq!(ran.code, 2, "{name}");
        assert_eq!(ran.polls, 0, "{name}");
        assert!(ran.stdout.is_empty(), "{name}");
        assert!(ran.stderr.is_empty(), "{name}");
        assert!(!ran.scratch, "{name}");
        let output = public_output(&selection);
        assert_eq!(output.status.code(), Some(2), "{name}");
        assert!(output.stdout.is_empty(), "{name}");
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            format!("xtask: {error}\n"),
            "{name}"
        );
    }

    let ran = command_with_poll_probe(&args()).await;
    assert_eq!(ran.code, 0, "{}", ran.stdout);
    assert_eq!(ran.polls, 1);
    assert!(ran.stdout.contains("RESULT PASS=19 FAIL=0 SKIP=0"));
    assert!(ran.stdout.contains("CLEAN PASS"));
}

#[tokio::test]
async fn credential_descriptors_follow_cli_grammar_without_echoing_rejected_input() {
    const FORMS: &str = "--credential expects env:NAME, an op:// reference, file:PATH, or omp-host";
    for source in [
        "env:ZAI_API_KEY",
        "file:/tmp/synthetic-key",
        "op://vault/item",
        "omp-host",
    ] {
        assert!(E2eCredential::parse(source).is_ok(), "{source}");
    }
    let longest = format!("env:{}", "A".repeat(128));
    assert!(E2eCredential::parse(&longest).is_ok(), "128-character name");
    let overlong = format!("env:{}", "A".repeat(129));
    {
        let source = overlong.as_str();
        let error = E2eCredential::parse(source).expect_err("overlong name");
        assert!(matches!(error, TargetError::CredentialDescriptor));
        assert_eq!(error.to_string(), FORMS);
        assert_eq!(format!("{error:?}"), "CredentialDescriptor");
        let output = public_output(&E2eArgs {
            credential: Some(source.to_owned()),
            ..args()
        });
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(stderr, format!("xtask: {FORMS}\n"));
        assert!(!stderr.contains(&"A".repeat(129)));
    }
    for source in [
        "op://",
        "op://vault",
        "op:///item",
        "op://vault//field",
        "op://vault//synthetic-canary-empty-field",
        "synthetic-raw-credential-canary-7f3a9c",
        "env:not a name",
        "env:",
        "file:",
        "omp-host-synthetic-canary-7f3a9c",
        "omp-hostile",
        "omp-host ",
        "OMP-HOST",
    ] {
        let error = E2eCredential::parse(source).expect_err(source);
        assert!(
            matches!(error, TargetError::CredentialDescriptor),
            "{source}"
        );
        let display = error.to_string();
        let debug = format!("{error:?}");
        assert_eq!(display, FORMS, "{source}");
        assert_eq!(debug, "CredentialDescriptor", "{source}");
        let output = public_output(&E2eArgs {
            credential: Some(source.to_owned()),
            ..args()
        });
        assert_eq!(output.status.code(), Some(2), "{source}");
        assert!(output.stdout.is_empty(), "{source}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(stderr, format!("xtask: {FORMS}\n"), "{source}");
        if !["op://", "env:", "file:"].contains(&source) {
            assert!(!display.contains(source), "{source}");
            assert!(!debug.contains(source), "{source}");
            assert!(!stderr.contains(source), "{source}");
        }
    }
}

#[test]
fn t01_credential_source_errors_never_echo_the_parsed_descriptor() {
    const CANARY: &str = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9";
    for source in [
        format!("env:{CANARY}_KEY"),
        format!("file:/tmp/{CANARY}.payload.signature"),
        format!("op://vault/{CANARY}/field"),
        "omp-host".to_owned(),
    ] {
        assert!(E2eCredential::parse(&source).is_ok(), "{source}");
        let selection = E2eArgs {
            credential: Some(source.clone()),
            ..args()
        };
        let error = E2eTarget::resolve(&selection, Some(Path::new("/home/test")))
            .expect_err("T01 accepts no explicit credential");
        assert!(
            matches!(error, TargetError::CredentialSource { .. }),
            "{error}"
        );
        let display = error.to_string();
        let debug = format!("{error:?}");
        for text in [&display, &debug] {
            assert!(!text.contains(CANARY), "{text}");
            assert!(!text.contains(source.as_str()) || source == "omp-host");
        }
        let output = public_output(&selection);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(stderr, format!("xtask: {display}\n"));
        assert!(!stderr.contains(CANARY), "{stderr}");
    }
}

fn fake_cargo_output(selection: &E2eArgs, dir: &Path) -> Output {
    use std::os::unix::fs::PermissionsExt;

    let cargo = dir.join("cargo");
    std::fs::write(
        &cargo,
        "#!/bin/sh\nprintf called > \"$0.called\"\nprintf 'fake cargo refuses\\n' >&2\nexit 1\n",
    )
    .expect("fake cargo");
    std::fs::set_permissions(&cargo, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let mut command = Command::new(env!("CARGO_BIN_EXE_jackin-xtask"));
    command
        .arg("e2e")
        .args([
            "--gateway",
            "jackin-local",
            "--role",
            "/nonexistent-role",
            "--codex-home",
            "/nonexistent-codex",
        ])
        .env_clear()
        .env("PATH", format!("{}:/usr/bin:/bin", dir.display()))
        .env("HOME", "/nonexistent-home");
    for (flag, value) in [
        ("--agent", &selection.agent),
        ("--provider", &selection.provider),
        ("--model", &selection.model),
        ("--credential", &selection.credential),
    ] {
        if let Some(value) = value {
            command.arg(flag).arg(value);
        }
    }
    capture(&mut command)
}

#[test]
fn the_binary_rejects_an_invalid_target_before_it_starts_cargo_or_reaches_a_gateway() {
    let control_dir = tempfile::tempdir().expect("tempdir");
    let control = fake_cargo_output(&args(), control_dir.path());
    assert_eq!(control.status.code(), Some(2));
    assert!(
        control_dir.path().join("cargo.called").exists(),
        "a valid target does start the fake cargo"
    );
    assert!(String::from_utf8_lossy(&control.stderr).contains("cargo build of jackin exited"));

    for (name, selection, expected) in [
        (
            "unknown pair",
            E2eArgs {
                agent: Some("grok".to_owned()),
                provider: Some("kimi".to_owned()),
                ..args()
            },
            "no catalog tuple",
        ),
        (
            "unsupported pair",
            E2eArgs {
                agent: Some("claude".to_owned()),
                provider: Some("claude".to_owned()),
                ..args()
            },
            "tuple T02 is unavailable",
        ),
        (
            "supported pair without adapter",
            E2eArgs {
                agent: Some("omp".to_owned()),
                provider: Some("kimi".to_owned()),
                ..args()
            },
            "tuple T10 has no complete E2E adapter",
        ),
        (
            "invalid model",
            E2eArgs {
                model: Some("not-a-model".to_owned()),
                ..args()
            },
            "not allowed",
        ),
        (
            "malformed credential",
            E2eArgs {
                credential: Some("op://".to_owned()),
                ..args()
            },
            "--credential expects env:NAME",
        ),
    ] {
        let dir = tempfile::tempdir().expect("tempdir");
        let output = fake_cargo_output(&selection, dir.path());
        assert_eq!(output.status.code(), Some(2), "{name}");
        assert!(output.stdout.is_empty(), "{name}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.starts_with("xtask: "), "{name}: {stderr}");
        assert!(stderr.contains(expected), "{name}: {stderr}");
        assert!(!stderr.contains("cargo"), "{name}: {stderr}");
        assert!(
            !dir.path().join("cargo.called").exists(),
            "{name}: cargo was started before the target was validated"
        );
    }
}
