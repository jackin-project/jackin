// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use jackin_xtask::{
    AgentBehaviorSpec, AnswerMatcher, BehaviorCategory, E2eArgs, E2eCredential, E2eTarget,
    TargetError, UsageBehavior, UsageWindow, t01_behavior,
};
use serde_json::{Value, json};

use super::scenario::world::{ACCESS, ID_TOKEN, REFRESH};
use super::scenario::{AgeReply, Scenario};

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
    assert_eq!(behavior.provider.accepted_status, 401);
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
                agent: Some("codex".to_owned()),
                provider: Some("zai".to_owned()),
                ..args()
            },
            "tuple T18 has no complete E2E adapter",
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
            Some("codex"),
            Some("zai"),
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
        drop(state);
        assert!(!scenario.world.scratch_root.exists(), "{name}");
    }
}

#[test]
fn canonical_digest_matches_independent_fixture_and_excludes_runtime_values() {
    let target = E2eTarget::t01().expect("T01 target");
    assert_eq!(
        target.identity().spec_sha256,
        "8bb9d5f794def53a5f3bca6bb0490616d909301f946ce2b870e6d7d848be305f"
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
    let text = String::from_utf8(first).expect("UTF-8");
    for excluded in [
        ACCESS,
        REFRESH,
        ID_TOKEN,
        "op://vault/api-key",
        "account-name",
        "actual-usage-value",
        "/another/home",
        "/different/host",
    ] {
        assert!(!text.contains(excluded), "spec contains {excluded}");
    }
    let value: Value = serde_json::from_str(&text).expect("spec JSON");
    assert!(value["behavior"]["prompt"]["argv"].is_array());
    assert_eq!(
        value["behavior"]["prompt"]["answer"],
        json!({"word_case_insensitive": {"word": "ready"}})
    );
    assert_eq!(
        value["target"],
        serde_json::to_value(target.identity()).expect("identity")
    );
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
                .push("OTHER_API_KEY".to_owned());
        }),
        ("auth file", |behavior, _| {
            behavior
                .credential
                .as_mut()
                .expect("credential")
                .auth_file
                .mode = "400".to_owned();
        }),
        ("provider status", |behavior, _| {
            behavior
                .provider
                .as_mut()
                .expect("provider")
                .accepted_status = 403;
        }),
        ("provider body", |behavior, _| {
            behavior.provider.as_mut().expect("provider").accepted_body =
                jackin_xtask::ProviderBodyMatcher::JsonErrorCode {
                    code: "other".to_owned(),
                };
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
        ("target", "/target/model", json!("other"), "model"),
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

#[tokio::test]
async fn fresh_host_replies_are_distinct_exact_and_required() {
    let plain = Scenario::new().await;
    plain.state().tag.clone_from(&plain.tag);
    let mut boundaries = plain.boundaries();
    boundaries.0.env.insert(
        "ZAI_API_KEY".to_owned(),
        "sibling-zai-secret-0123456789".to_owned(),
    );
    boundaries.0.env.insert(
        "KIMI_API_KEY".to_owned(),
        "sibling-kimi-secret-0123456789".to_owned(),
    );
    let status = jackin_xtask::run_e2e(&plain.args, boundaries.0).await;
    assert_eq!(status.code(), 0);
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
            assert!(!env.contains_key("ZAI_API_KEY"));
            assert!(!env.contains_key("KIMI_API_KEY"));
        }
        let serialized = state.probe_specs.concat();
        let text = String::from_utf8(serialized).expect("spec text");
        assert!(!text.contains("sibling-zai-secret"));
        assert!(!text.contains("sibling-kimi-secret"));
    }

    let before = Scenario::new().await;
    before.host_rule("fresh-before", None, 1, ACCESS);
    let ran = before.run().await;
    assert_eq!(ran.status.code(), 2);
    assert!(ran.stdout.is_empty());
    assert!(ran.stderr.contains("fresh host Codex reply before failed"));
    assert!(!ran.stderr.contains(ACCESS));

    let after = Scenario::new().await;
    after.host_rule("fresh-after", None, 1, ACCESS);
    let ran = after.run().await;
    assert_eq!(ran.status.code(), 1, "{}", ran.stdout);
    assert!(ran.stdout.contains("HOST_REPLY FAIL"));
    assert!(ran.stdout.contains("a1_host_login=unchanged"));
    assert!(!ran.stdout.contains(ACCESS));
    assert!(!ran.stderr.contains(ACCESS));
}
