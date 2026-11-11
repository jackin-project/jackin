// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::path::Path;

use jackin_xtask::{
    AgentBehaviorSpec, BehaviorCategory, CompleteBehaviorSpec, CredentialClassification, E2eArgs,
    E2eCredential, E2eTarget, EngineExpectation, ProbeMode, ProbeReport, ProbeRequest, Redactor,
    TargetError, t01_behavior, t09_behavior, t14_behavior, t18_behavior,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::scenario::{Ran, Scenario};
use super::support::hex;

const ZAI: &str = "synthetic-zai-plan-key-0123456789abcdef";

#[derive(serde::Serialize)]
struct CanonicalSpec<'a> {
    tuple: &'a str,
    agent: &'a str,
    provider: &'a str,
    model: &'a str,
    adapter: &'a str,
    behavior: &'a CompleteBehaviorSpec,
    provider_hosts: &'a [String],
}

fn selection(id: &str, credential: Option<&str>) -> E2eArgs {
    let (agent, provider) = match id {
        "T01" => ("codex", "codex"),
        "T09" => ("omp", "zai"),
        "T14" => ("claude", "zai"),
        "T18" => ("codex", "zai"),
        "T10" => ("omp", "kimi"),
        "T11" => ("omp", "minimax"),
        "T12" => ("omp", "mimo"),
        "T16" => ("claude", "mimo"),
        "T20" => ("codex", "mimo"),
        other => panic!("no selection for {other}"),
    };
    E2eArgs {
        agent: Some(agent.to_owned()),
        provider: Some(provider.to_owned()),
        credential: credential.map(str::to_owned),
        ..E2eArgs::default()
    }
}

fn behavior(id: &str) -> AgentBehaviorSpec {
    match id {
        "T01" => t01_behavior(),
        "T09" => t09_behavior(),
        "T14" => t14_behavior(),
        "T18" => t18_behavior(),
        other => panic!("no behavior for {other}"),
    }
}

fn target(id: &str, credential: Option<&str>) -> E2eTarget {
    E2eTarget::resolve(&selection(id, credential), Some(Path::new("/home/test")))
        .expect("supported target")
}

async fn zai_scenario(id: &str) -> Scenario {
    let mut scenario = Scenario::new().await;
    scenario.args = selection(id, Some("env:ZAI_API_KEY"));
    scenario.args.gateway = super::scenario::GATEWAY.to_owned();
    scenario.args.role = Some(scenario.world.role.clone());
    scenario.args.codex_home = Some(scenario.world.codex_home.clone());
    scenario.args.engine_port = std::num::NonZeroU16::new(2376);
    scenario.args.engine_certs = Some(scenario.world.engine_certs.clone());
    scenario
}

fn zai_a1_usage() -> Value {
    json!({"state": "windows", "windows": [
        {"kind": "five_hour", "label": "5h", "used_percent": 1.0, "resets_at": 1},
        {"kind": "weekly", "label": "w", "used_percent": 2.0, "resets_at": 1},
        {"kind": "monthly", "label": "m", "used_percent": 3.0, "resets_at": 1},
    ]})
}

fn assert_t14_prompt(target: &E2eTarget) {
    assert_eq!(
        target.behavior().prompt.argv,
        [
            "/usr/local/bin/claude",
            "-p",
            "--output-format",
            "stream-json",
            "--verbose",
            "--model",
            "glm-5.3",
            "--allowedTools",
            "Bash",
            "Reply with exactly ready using model glm-5.3",
        ]
    );
    assert_eq!(
        target.behavior().prompt.tool.argv,
        [
            "/usr/local/bin/claude",
            "-p",
            "--output-format",
            "stream-json",
            "--verbose",
            "--model",
            "glm-5.3",
            "--allowedTools",
            "Bash",
            "Use the Bash tool once with command ls -A in /sandbox/project using model glm-5.3, then reply with exactly ready",
        ]
    );
}

fn assert_t18_prompt(target: &E2eTarget) {
    assert_eq!(
        target.behavior().prompt.argv,
        [
            "/usr/local/bin/codex",
            "exec",
            "--json",
            "--skip-git-repo-check",
            "--sandbox",
            "danger-full-access",
            "-c",
            "check_for_update_on_startup=false",
            "Reply with exactly ready",
        ]
    );
    assert_eq!(
        target
            .behavior()
            .prompt
            .tool
            .argv
            .last()
            .map(String::as_str),
        Some(
            "Use one shell tool to run ls -A in /sandbox/project with provider zai and model glm-5.3, then reply with exactly ready"
        )
    );
}

fn zai_usage_accounts(wrong: &str) -> Vec<Value> {
    ["a1", "fxr", "fxw"]
        .into_iter()
        .map(|suffix| {
            let outcome = if suffix == "a1" {
                zai_a1_usage()
            } else {
                json!({"state": "unavailable", "reason": {"kind": "expired_login"}})
            };
            json!({
                "account": format!("e2e-abcd-{suffix}"),
                "provider": if suffix == wrong { "codex" } else { "zai" },
                "freshness": {"state": "fresh"},
                "outcome": outcome,
            })
        })
        .collect()
}

async fn expect_process_fault(id: &str, step: &str, fault: &str) {
    let scenario = zai_scenario(id).await;
    let call = if step == "J12" { Some(2) } else { Some(1) };
    if fault == "wrong comm" {
        scenario.patch(step, call, "/facts/processes/agent/0/comm", json!("other"));
    } else {
        let comm = target(id, Some("env:ZAI_API_KEY"))
            .behavior()
            .process
            .comm
            .clone();
        scenario.patch(
            step,
            call,
            "/facts/processes/agent",
            json!([
                {"pid": 4242, "ppid": 100, "sid": 4242, "comm": comm, "start_ticks": 42420, "tty": "pts/0"},
                {"pid": 4243, "ppid": 100, "sid": 4243, "comm": comm, "start_ticks": 42430, "tty": "pts/0"},
            ]),
        );
    }
    let ran = scenario.run_with_env(&[("ZAI_API_KEY", ZAI)]).await;
    let expected = if step == "J11" {
        vec![("J11", "FAIL"), ("J12", "PASS")]
    } else {
        vec![("J12", "FAIL")]
    };
    let case = format!("{id} {step} {fault}");
    ran.expect(&case, &expected);
    let reason = if fault == "wrong comm" {
        "wrong agent comm"
    } else {
        "agent children"
    };
    ran.reason_has(&case, step, reason);
}

fn expect_full_success(id: &str, ran: &Ran) {
    assert_eq!(ran.status.code(), 0, "{id}: {} {}", ran.stdout, ran.stderr);
    let expected: BTreeSet<String> = (0..=17)
        .map(|number| format!("J{number:02}"))
        .chain(["CLEAN".to_owned()])
        .collect();
    let statuses = ran.statuses();
    assert_eq!(
        statuses.keys().collect::<BTreeSet<_>>(),
        expected.iter().collect()
    );
    assert!(
        statuses.values().all(|status| status == "PASS"),
        "{id}: {statuses:?}\n{}",
        ran.stdout
    );
    assert_eq!(statuses.len(), 19, "{id}");
    assert!(ran.stdout.contains("RESULT PASS=19 FAIL=0 SKIP=0"), "{id}");
    assert!(ran.stdout.contains("CLEAN PASS"), "{id}");
}

#[test]
fn supported_target_resolution_matches_catalog_and_selection_defaults() {
    let default = E2eTarget::resolve(&E2eArgs::default(), Some(Path::new("/home/test")))
        .expect("T01 default");
    assert_eq!(default.tuple().id, "T01");
    assert_eq!(default.model().as_str(), "gpt-6.1-sol");
    assert_eq!(default.identity().adapter, "t01-codex-v1");

    let omp = target("T09", Some("env:ZAI_API_KEY"));
    assert_eq!(omp.tuple().id, "T09");
    assert_eq!(omp.identity().adapter, "t09-omp-zai-v1");
    assert_eq!(omp.behavior().version.exact_output, "omp/18.18.3");
    assert_eq!(omp.behavior().process.comm, "omp");
    assert_eq!(
        omp.behavior().provider.probe_url,
        "https://api.z.ai/api/anthropic"
    );
    assert_eq!(
        omp.behavior().credential.classification,
        CredentialClassification::StaticKey
    );
    assert!(matches!(omp.credential(), E2eCredential::Env { .. }));
    assert_eq!(
        E2eTarget::resolve(&selection("T09", None), None)
            .expect("omp default")
            .credential(),
        &E2eCredential::OmpHost
    );

    let claude = target("T14", Some("env:ZAI_API_KEY"));
    assert_eq!(claude.tuple().id, "T14");
    assert_eq!(claude.identity().adapter, "t14-claude-zai-v1");
    assert_eq!(
        claude.behavior().version.exact_output,
        "2.1.311 (Claude Code)"
    );
    assert_eq!(claude.behavior().process.comm, "claude");
    assert_eq!(
        claude.behavior().provider.probe_url,
        "https://api.z.ai/api/anthropic"
    );
    assert_eq!(
        claude.behavior().credential.environment[0].name,
        "ANTHROPIC_AUTH_TOKEN"
    );
    assert_eq!(
        claude.behavior().credential.classification,
        CredentialClassification::StaticKey
    );
    assert_t14_prompt(&claude);

    let provider_only = E2eTarget::resolve(
        &E2eArgs {
            provider: Some("zai".to_owned()),
            credential: Some("env:ZAI_API_KEY".to_owned()),
            ..E2eArgs::default()
        },
        Some(Path::new("/home/test")),
    )
    .expect("provider-only T18");
    assert_eq!(provider_only.tuple().id, "T18");
    assert_eq!(provider_only.identity().adapter, "t18-codex-zai-v1");
    assert_eq!(
        provider_only.behavior().version.exact_output,
        "codex-cli 0.167.0"
    );
    assert_eq!(
        provider_only.behavior().provider.probe_url,
        "https://api.z.ai/api/v1"
    );
    assert_eq!(
        provider_only.behavior().credential.auth_file.required_state,
        "absent"
    );
    assert_eq!(provider_only.behavior().credential.auth_file.mode, None);
    assert_t18_prompt(&provider_only);
    assert_ne!(provider_only.behavior(), default.behavior());

    for id in ["T09", "T14", "T18"] {
        let selected = target(id, Some("env:ZAI_API_KEY"));
        assert_eq!(selected.model().as_str(), "glm-5.3");
        assert_eq!(
            selected.selection_args(),
            [
                "--agent",
                selected.tuple().agent.as_str(),
                "--provider",
                "zai",
                "--model",
                "glm-5.3",
            ]
            .map(str::to_owned)
        );
        assert_eq!(
            selected.tuple().auth_class,
            jackin_core::AuthClass::StaticKey
        );
        assert_eq!(selected.provider_hosts(), ["api.z.ai"]);
        let explicit = E2eTarget::resolve(&selection(id, Some("env:ZAI_API_KEY")), None)
            .expect("explicit Z.AI");
        assert_eq!(explicit.identity(), selected.identity());
        let rejected = E2eTarget::resolve(
            &E2eArgs {
                model: Some("gpt-6.1-sol".to_owned()),
                ..selection(id, Some("env:ZAI_API_KEY"))
            },
            None,
        );
        assert!(matches!(rejected, Err(TargetError::Catalog(_))));
    }

    for id in ["T10", "T11", "T12", "T16", "T20"] {
        let error = E2eTarget::resolve(&selection(id, Some("env:ZAI_API_KEY")), None)
            .expect_err("supported tuple without adapter");
        assert!(
            matches!(error, TargetError::AdapterUnavailable { .. }),
            "{id}: {error}"
        );
    }

    for id in ["T09", "T14", "T18"] {
        for category in [
            BehaviorCategory::Version,
            BehaviorCategory::Process,
            BehaviorCategory::Prompt,
            BehaviorCategory::Terminal,
            BehaviorCategory::Credential,
            BehaviorCategory::Reflection,
            BehaviorCategory::Provider,
            BehaviorCategory::Usage,
        ] {
            let mut raw = behavior(id);
            match category {
                BehaviorCategory::Version => raw.version = None,
                BehaviorCategory::Process => raw.process = None,
                BehaviorCategory::Prompt => raw.prompt = None,
                BehaviorCategory::Terminal => raw.terminal = None,
                BehaviorCategory::Credential => raw.credential = None,
                BehaviorCategory::Reflection => raw.reflection = None,
                BehaviorCategory::Provider => raw.provider = None,
                BehaviorCategory::Usage => raw.usage = None,
            }
            let error = E2eTarget::resolve_with_behavior(
                &selection(id, Some("env:ZAI_API_KEY")),
                None,
                Some(raw),
            )
            .expect_err("incomplete adapter");
            assert!(
                matches!(error, TargetError::IncompleteBehavior { category: found, .. } if found == category),
                "{id} {category}: {error}"
            );
        }
    }
}

#[tokio::test]
async fn invalid_targets_and_incomplete_adapters_have_no_setup_effects() {
    for (id, source) in [("T10", "env:ZAI_API_KEY"), ("T14", "omp-host")] {
        let scenario = Scenario::new().await;
        let error = E2eTarget::resolve(&selection(id, Some(source)), None)
            .expect_err("invalid target or source");
        assert!(matches!(
            error,
            TargetError::AdapterUnavailable { .. } | TargetError::CredentialSource { .. }
        ));
        assert!(scenario.state().commands.is_empty());
        assert_eq!(scenario.state().health_calls, 0);
        assert!(scenario.state().probe_specs.is_empty());
        assert!(!scenario.world.scratch_root.exists());
    }

    for id in ["T09", "T14", "T18"] {
        let scenario = Scenario::new().await;
        let mut incomplete = behavior(id);
        incomplete.reflection = None;
        let error = E2eTarget::resolve_with_behavior(
            &selection(id, Some("env:ZAI_API_KEY")),
            None,
            Some(incomplete),
        )
        .expect_err("incomplete behavior");
        assert!(matches!(
            error,
            TargetError::IncompleteBehavior {
                category: BehaviorCategory::Reflection,
                ..
            }
        ));
        assert!(scenario.state().commands.is_empty());
        assert_eq!(scenario.state().health_calls, 0);
        assert!(!scenario.world.scratch_root.exists());
    }

    let mut scenario = Scenario::new().await;
    scenario.args = selection("T14", Some("env:ZAI_API_KEY"));
    let ran = scenario.run().await;
    assert_eq!(ran.status.code(), 2);
    assert!(ran.stderr.contains("ZAI_API_KEY"));
    assert!(!ran.stderr.contains(ZAI));
    assert_eq!(scenario.state().health_calls, 0);
    assert!(
        scenario
            .state()
            .commands
            .iter()
            .all(|command| !command.contains("account add"))
    );
    assert!(!scenario.world.scratch_root.exists());
}

#[tokio::test]
async fn credential_sources_and_discovery_follow_the_tuple_contract() {
    for id in ["T09", "T14", "T18"] {
        for source in [
            "env:ZAI_API_KEY",
            "env:SYNTHETIC_PLAN_SOURCE",
            "op://vault/item/field",
            "file:/tmp/synthetic-plan-key",
        ] {
            let resolved = target(id, Some(source));
            assert!(!format!("{:?}", resolved.probe_spec()).contains("synthetic-plan-key"));
            if id != "T09" {
                let omp = E2eTarget::resolve(&selection(id, Some("omp-host")), None);
                assert!(matches!(omp, Err(TargetError::CredentialSource { .. })));
            }
        }
    }
    let omp = E2eTarget::resolve(&selection("T09", Some("omp-host")), None).expect("omp host");
    assert_eq!(omp.credential(), &E2eCredential::OmpHost);

    for id in ["T09", "T14", "T18"] {
        let scenario = Scenario::new().await;
        let (mut boundaries, _, _) = scenario.boundaries();
        boundaries
            .env
            .insert("SYNTHETIC_PLAN_SOURCE".to_owned(), ZAI.to_owned());
        let resolved = E2eTarget::resolve(&selection(id, Some("env:SYNTHETIC_PLAN_SOURCE")), None)
            .expect("alternate source");
        let session = jackin_xtask::CredentialSession::open(resolved, &boundaries)
            .await
            .expect("source session");
        let selected = session
            .selected_host_environment(&boundaries.env)
            .expect("selected");
        assert!(selected.contains_key("SYNTHETIC_PLAN_SOURCE"));
        let spec =
            String::from_utf8(session.target.probe_spec_bytes().expect("spec")).expect("text");
        assert!(!spec.contains("SYNTHETIC_PLAN_SOURCE"));
        assert!(!spec.contains(ZAI));
        if id == "T14" {
            assert!(spec.contains("ANTHROPIC_AUTH_TOKEN"));
            assert!(!spec.contains("env:SYNTHETIC_PLAN_SOURCE"));
        }
    }

    let scenario = Scenario::new().await;
    scenario.host_reply_rule(
        "omp --profile zai token zai",
        None,
        0,
        &format!("{ZAI}\n"),
        "",
    );
    let (boundaries, _, _) = scenario.boundaries();
    let discovered = E2eTarget::resolve(&selection("T09", None), None).expect("discovery target");
    let session = jackin_xtask::CredentialSession::open(discovered, &boundaries)
        .await
        .expect("omp-host discovery");
    assert_eq!(session.secrets(), [ZAI]);
    assert!(!session.redactor().scrub(ZAI).contains(ZAI));

    let scenario = Scenario::new().await;
    scenario.host_reply_rule(
        "op read op://vault/item/field",
        None,
        0,
        &format!("{ZAI}\n"),
        "",
    );
    let (boundaries, _, _) = scenario.boundaries();
    let explicit = target("T14", Some("op://vault/item/field"));
    let session = jackin_xtask::CredentialSession::open(explicit, &boundaries)
        .await
        .expect("op source");
    assert_eq!(session.secrets(), [ZAI]);
    assert!(
        !scenario
            .state()
            .commands
            .iter()
            .any(|command| command.contains("whoami"))
    );

    let root = tempfile::tempdir().expect("tempdir");
    let path = root.path().join("key");
    std::fs::write(&path, format!("{ZAI}\n")).expect("key");
    let scenario = Scenario::new().await;
    let (boundaries, _, _) = scenario.boundaries();
    let source = format!("file:{}", path.display());
    let explicit = target("T18", Some(&source));
    let session = jackin_xtask::CredentialSession::open(explicit, &boundaries)
        .await
        .expect("file source");
    assert_eq!(session.secrets(), [ZAI]);
}

#[tokio::test]
async fn onepassword_discovery_requires_whoami_and_one_zai_item() {
    for id in ["T09", "T14", "T18"] {
        let scenario = Scenario::new().await;
        scenario.host_reply_rule("op whoami", None, 0, "tester@example.invalid\n", "");
        scenario.host_reply_rule(
            "op item list --format json",
            None,
            0,
            "[{\"id\":\"item-1\",\"title\":\"Z.AI\",\"vault\":{\"id\":\"vault-1\",\"name\":\"Private\"}}]\n",
            "",
        );
        scenario.host_reply_rule(
            "op read op://vault-1/item-1/credential",
            None,
            0,
            &format!("{ZAI}\n"),
            "",
        );
        let (boundaries, _, _) = scenario.boundaries();
        let discovered = E2eTarget::resolve(&selection(id, None), None).expect("default target");
        let session = jackin_xtask::CredentialSession::open(discovered, &boundaries)
            .await
            .expect("1Password discovery");
        assert_eq!(session.secrets(), [ZAI]);
        let commands = scenario.state().commands.clone();
        let whoami = commands
            .iter()
            .position(|command| command == "op whoami")
            .expect("whoami");
        let listed = commands
            .iter()
            .position(|command| command == "op item list --format json")
            .expect("item list");
        let read = commands
            .iter()
            .position(|command| command == "op read op://vault-1/item-1/credential")
            .expect("item read");
        assert!(whoami < listed && listed < read, "{commands:?}");
        assert_eq!(scenario.state().health_calls, 0);
        assert!(
            !session
                .target
                .probe_spec_bytes()
                .expect("spec")
                .as_slice()
                .windows(b"vault-1".len())
                .any(|window| window == b"vault-1")
        );
    }

    let scenario = Scenario::new().await;
    scenario.host_reply_rule("op whoami", None, 1, "", "not signed in\n");
    let (boundaries, _, _) = scenario.boundaries();
    let discovered = E2eTarget::resolve(&selection("T14", None), None).expect("default target");
    let error = jackin_xtask::CredentialSession::open(discovered, &boundaries)
        .await
        .expect_err("whoami failure");
    assert!(matches!(error, jackin_xtask::CredentialError::Unavailable));
    assert!(
        !scenario
            .state()
            .commands
            .iter()
            .any(|command| command.contains("item list") || command.contains("item-1"))
    );
    assert_eq!(scenario.state().health_calls, 0);

    let scenario = Scenario::new().await;
    scenario.host_reply_rule("op whoami", None, 0, "tester@example.invalid\n", "");
    scenario.host_reply_rule(
        "op item list --format json",
        None,
        0,
        "[{\"id\":\"item-1\",\"title\":\"Z.AI\",\"vault\":\"vault-1\"},{\"id\":\"item-2\",\"title\":\"z.ai\",\"vault\":\"vault-2\"}]\n",
        "",
    );
    let (boundaries, _, _) = scenario.boundaries();
    let discovered = E2eTarget::resolve(&selection("T18", None), None).expect("default target");
    let error = jackin_xtask::CredentialSession::open(discovered, &boundaries)
        .await
        .expect_err("ambiguous discovery");
    assert!(matches!(error, jackin_xtask::CredentialError::Unavailable));
    assert!(
        !scenario
            .state()
            .commands
            .iter()
            .any(|command| command.contains("credential"))
    );
    assert_eq!(scenario.state().health_calls, 0);
}

#[tokio::test]
async fn static_key_sessions_isolate_a1_and_invalid_fixture_accounts() {
    fn prefix_of(value: &str) -> &str {
        match value
            .char_indices()
            .rfind(|(_, c)| !c.is_ascii_alphanumeric())
        {
            Some((index, c)) => &value[..index + c.len_utf8()],
            None => "",
        }
    }
    fn class_of(c: char) -> u8 {
        if c.is_ascii_lowercase() {
            0
        } else if c.is_ascii_uppercase() {
            1
        } else if c.is_ascii_digit() {
            2
        } else {
            3
        }
    }
    for id in ["T09", "T14", "T18"] {
        for key in [ZAI, "other-plan-key-xyz9"] {
            let scenario = zai_scenario(id).await;
            let ran = scenario.run_with_env(&[("ZAI_API_KEY", key)]).await;
            expect_full_success(id, &ran);
            assert!(ran.stdout.contains("a1_host_login=not-applicable"));
            let state = scenario.state();
            assert!(state.fresh_replies.is_empty());
            assert!(state.fresh_reply_lock_refused.is_empty());
            let a1 = "e2e-abcd-a1".to_owned();
            let fxr = "e2e-abcd-fxr".to_owned();
            let fxw = "e2e-abcd-fxw".to_owned();
            assert_eq!(
                state.static_provider_keys.get(&a1).map(String::as_str),
                Some(key)
            );
            assert_ne!(
                state.static_provider_keys.get(&fxr).map(String::as_str),
                Some(key)
            );
            assert_ne!(
                state.static_provider_keys.get(&fxw).map(String::as_str),
                Some(key)
            );
            assert_ne!(
                state.static_provider_keys.get(&fxr),
                state.static_provider_keys.get(&fxw)
            );
            let fxr_value = state.static_provider_keys.get(&fxr).expect("fxr key");
            let fxw_value = state.static_provider_keys.get(&fxw).expect("fxw key");
            assert_eq!(fxr_value.len(), key.len());
            assert_eq!(fxw_value.len(), key.len());
            let key_prefix = prefix_of(key);
            assert!(fxr_value.starts_with(key_prefix));
            assert!(fxw_value.starts_with(key_prefix));
            for (index, c) in key.chars().enumerate() {
                let fxr_char = fxr_value.chars().nth(index).expect("fxr char");
                let fxw_char = fxw_value.chars().nth(index).expect("fxw char");
                assert_eq!(
                    c.is_ascii_alphanumeric(),
                    fxr_char.is_ascii_alphanumeric(),
                    "{id} fxr separator position {index}"
                );
                assert_eq!(
                    c.is_ascii_alphanumeric(),
                    fxw_char.is_ascii_alphanumeric(),
                    "{id} fxw separator position {index}"
                );
                assert_eq!(class_of(c), class_of(fxr_char), "{id} fxr class {index}");
                assert_eq!(class_of(c), class_of(fxw_char), "{id} fxw class {index}");
            }
            assert_eq!(state.accounts, [a1, fxr, fxw]);
            for command in &state.commands {
                assert!(!command.contains("op://"));
                assert!(!command.contains("SYNTHETIC_PLAN_SOURCE"));
                assert!(!command.contains(ZAI));
                assert!(!command.contains(key));
                assert!(!command.contains("--chatgpt-from"));
            }
            assert!(!ran.stdout.contains(ZAI));
            assert!(!ran.stderr.contains(ZAI));
            assert!(!ran.stdout.contains(key));
            assert!(!ran.stderr.contains(key));
        }
    }
}

#[tokio::test]
async fn schema_v3_private_specs_are_exact_distinct_and_secret_free() {
    let targets: Vec<E2eTarget> = ["T01", "T09", "T14", "T18"]
        .into_iter()
        .map(|id| target(id, (id != "T01").then_some("env:SYNTHETIC_PLAN_SOURCE")))
        .collect();
    let identities: BTreeSet<String> = targets
        .iter()
        .map(|target| target.identity().spec_sha256.clone())
        .collect();
    assert_eq!(identities.len(), 4);
    for target in &targets {
        let first = target.probe_spec_bytes().expect("spec");
        assert_eq!(first, target.probe_spec_bytes().expect("stable spec"));
        let spec: Value = serde_json::from_slice(&first).expect("spec JSON");
        let canonical = CanonicalSpec {
            tuple: &target.identity().tuple,
            agent: &target.identity().agent,
            provider: &target.identity().provider,
            model: &target.identity().model,
            adapter: &target.identity().adapter,
            behavior: target.behavior(),
            provider_hosts: target.provider_hosts(),
        };
        assert_eq!(
            hex(&Sha256::digest(
                serde_json::to_vec(&canonical).expect("canonical")
            )),
            target.identity().spec_sha256
        );
        let text = String::from_utf8(first).expect("UTF-8");
        for excluded in [
            ZAI,
            "SYNTHETIC_PLAN_SOURCE",
            "op://vault",
            "/tmp/secret",
            "e2e-abcd",
        ] {
            assert!(
                !text.contains(excluded),
                "{} contains {excluded}",
                target.tuple().id
            );
        }
        assert!(spec["behavior"]["prompt"]["argv"].is_array());
        assert!(spec["behavior"]["prompt"]["tool"]["argv"].is_array());
        assert!(spec["behavior"]["prompt"]["tool"]["observation"].is_object());
        assert!(spec["behavior"]["credential"]["auth_file"]["fields"].is_array());

        let mut changed = behavior(target.tuple().id);
        changed.prompt.as_mut().expect("prompt").tool.kind = "wrong-tool".to_owned();
        let changed = E2eTarget::resolve_with_behavior(
            &selection(
                target.tuple().id,
                (target.tuple().id != "T01").then_some("env:SYNTHETIC_PLAN_SOURCE"),
            ),
            Some(Path::new("/home/test")),
            Some(changed),
        )
        .expect("changed tool");
        assert_ne!(
            changed.identity().spec_sha256,
            target.identity().spec_sha256
        );
    }

    let scenario = Scenario::new().await;
    for left in &targets {
        for right in &targets {
            if left.tuple().id == right.tuple().id {
                continue;
            }
            scenario.state().target = Some(left.clone());
            let report = scenario.probe_json(&["J04"], false);
            let request = ProbeRequest {
                target: right.identity().clone(),
                only: vec!["J04".to_owned()],
                docker: false,
                mode: ProbeMode::Sandbox,
                engine: EngineExpectation::Absent,
            };
            drop(
                ProbeReport::parse(report.to_string().as_bytes(), &request, &Redactor::none())
                    .unwrap_err(),
            );
        }
    }
}

#[tokio::test]
async fn each_supported_target_completes_all_19_lines_without_skips() {
    let control = Scenario::new().await;
    expect_full_success("T01", &control.run().await);
    for id in ["T09", "T14", "T18"] {
        let scenario = zai_scenario(id).await;
        let ran = scenario.run_with_env(&[("ZAI_API_KEY", ZAI)]).await;
        expect_full_success(id, &ran);
        let target = target(id, Some("env:ZAI_API_KEY"));
        let state = scenario.state();
        assert!(!state.probe_specs.is_empty());
        for bytes in &state.probe_specs {
            let spec: Value = serde_json::from_slice(bytes).expect("spec");
            assert_eq!(
                spec["target"],
                serde_json::to_value(target.identity()).expect("identity")
            );
            assert_eq!(spec["target"]["spec_sha256"], target.identity().spec_sha256);
        }
        for command in state
            .commands
            .iter()
            .filter(|command| command.contains(" build ") || command.contains(" load "))
        {
            for argument in target.selection_args() {
                assert!(command.contains(&argument), "{id}: {command}");
            }
            assert!(!command.contains("--account"));
            assert!(!command.contains("--account-for"));
        }
    }
}

#[tokio::test]
async fn journey_process_terminal_and_restart_identity_uses_the_selected_adapter() {
    for id in ["T09", "T14", "T18"] {
        for same_pid in [false, true] {
            let scenario = zai_scenario(id).await;
            if same_pid {
                scenario.knobs().restart_same_pid = true;
            } else {
                scenario.knobs().pty_fault = Some(super::scenario::PtyFault {
                    from: 1,
                    header: Some("wrong terminal readiness".to_owned()),
                    exit: None,
                    never_exits: false,
                });
            }
            let ran = scenario.run_with_env(&[("ZAI_API_KEY", ZAI)]).await;
            if same_pid {
                ran.expect(id, &[("J07", "FAIL"), ("J11", "SKIP"), ("J12", "SKIP")]);
            } else {
                ran.expect(
                    id,
                    &[
                        ("J07", "FAIL"),
                        ("J08", "FAIL"),
                        ("J11", "SKIP"),
                        ("J12", "SKIP"),
                    ],
                );
            }
        }
    }
}

#[tokio::test]
async fn j11_and_j12_reject_wrong_comm_and_duplicate_children_for_each_target() {
    for id in ["T09", "T14", "T18"] {
        for step in ["J11", "J12"] {
            for fault in ["wrong comm", "duplicate child"] {
                expect_process_fault(id, step, fault).await;
            }
        }
    }
}

#[tokio::test]
async fn credentialed_exec_proves_identity_answer_and_real_tool_use() {
    for id in ["T09", "T14", "T18"] {
        for (name, pointer, value) in [
            (
                "answer without tool",
                "/facts/agent_exec/tool/succeeded",
                json!(false),
            ),
            (
                "wrong provider",
                "/facts/agent_exec/actual_provider",
                json!("codex"),
            ),
            (
                "wrong model",
                "/facts/agent_exec/actual_model",
                json!("other"),
            ),
            (
                "wrong billing class",
                "/facts/agent_exec/credential",
                json!("api_key"),
            ),
            ("wrong tool", "/facts/agent_exec/tool/kind", json!("prose")),
            (
                "wrong directory",
                "/facts/agent_exec/tool/workdir",
                json!("/tmp"),
            ),
            (
                "fabricated observation",
                "/facts/agent_exec/tool/observation",
                json!([]),
            ),
        ] {
            let scenario = zai_scenario(id).await;
            scenario.patch("J07", None, pointer, value);
            let ran = scenario.run_with_env(&[("ZAI_API_KEY", ZAI)]).await;
            ran.expect(
                &format!("{id} {name}"),
                &[("J07", "FAIL"), ("J11", "SKIP"), ("J12", "SKIP")],
            );
        }
    }
}

#[tokio::test]
async fn j08_enforces_exact_target_credential_delivery_and_reflection() {
    for id in ["T09", "T14", "T18"] {
        let other = zai_scenario(id).await;
        other.patch("J08", None, "/facts/environment/0/kind", json!("other"));
        let ran = other.run_with_env(&[("ZAI_API_KEY", ZAI)]).await;
        ran.expect(&format!("{id} other credential"), &[("J08", "FAIL")]);
        ran.reason_has(id, "J08", "kind other");

        let scenario = zai_scenario(id).await;
        if id == "T14" {
            scenario.patch(
                "J08",
                None,
                "/facts/environment/0",
                json!({"name": "ZAI_API_KEY", "kind": "brokered", "source": "agent"}),
            );
        } else if id == "T18" {
            scenario.patch("J08", None, "/facts/auth_file/state", json!("present"));
            scenario.patch("J08", None, "/facts/auth_file/mode", json!("600"));
        } else {
            scenario.patch("J08", None, "/facts/environment/0/kind", json!("other"));
        }
        scenario.knobs().reflect_not_descendant = true;
        let ran = scenario.run_with_env(&[("ZAI_API_KEY", ZAI)]).await;
        ran.expect(id, &[("J08", "FAIL")]);
    }
}

#[tokio::test]
async fn j08_rejects_unrecognized_brokered_values() {
    for id in ["T09", "T14", "T18"] {
        let variable = if id == "T14" {
            "ANTHROPIC_AUTH_TOKEN"
        } else {
            "ZAI_API_KEY"
        };
        let scenario = zai_scenario(id).await;
        scenario.knobs().brokered_env_value = Some("arbitrary-other-value".to_owned());
        let ran = scenario.run_with_env(&[("ZAI_API_KEY", ZAI)]).await;
        let case = format!("{id} arbitrary brokered value");
        ran.expect(&case, &[("J08", "FAIL")]);
        ran.reason_has(&case, "J08", variable);
    }

    for (label, value) in [
        ("wrong name", "openshell:resolve:env:WRONG_NAME"),
        ("truncated", "openshell:resolve:env:"),
        ("raw key", ZAI),
    ] {
        let scenario = zai_scenario("T09").await;
        scenario.knobs().brokered_env_value = Some(value.to_owned());
        let ran = scenario.run_with_env(&[("ZAI_API_KEY", ZAI)]).await;
        let case = format!("T09 {label} brokered value");
        ran.expect(&case, &[("J08", "FAIL")]);
        ran.reason_has(&case, "J08", "ZAI_API_KEY");
    }

    let scenario = zai_scenario("T09").await;
    let ran = scenario.run_with_env(&[("ZAI_API_KEY", ZAI)]).await;
    expect_full_success("T09", &ran);
}

#[tokio::test]
async fn j09_provider_closure_and_response_classification_are_exact() {
    for id in ["T09", "T14", "T18"] {
        for url in ["https://outside.invalid/v1", "https://user@api.z.ai/v1"] {
            let mut raw = behavior(id);
            raw.provider.as_mut().expect("provider").probe_url = url.to_owned();
            let error = E2eTarget::resolve_with_behavior(
                &selection(id, Some("env:ZAI_API_KEY")),
                None,
                Some(raw),
            )
            .expect_err("outside or malformed URL");
            assert!(matches!(error, TargetError::OutsideProfile { .. }));
        }
        let scenario = zai_scenario(id).await;
        scenario.patch("J09", None, "/facts/provider_host/http_code", json!(401));
        scenario.patch(
            "J09",
            None,
            "/facts/provider_host/body",
            json!("invalid_api_key"),
        );
        let ran = scenario.run_with_env(&[("ZAI_API_KEY", ZAI)]).await;
        ran.expect(id, &[("J09", "FAIL")]);
    }
}

#[tokio::test]
async fn j17_enforces_zai_plan_usage_and_account_precedence() {
    for id in ["T09", "T14", "T18"] {
        for (name, window) in [
            (
                "weekly",
                json!({"kind": "weekly", "label": "w", "used_percent": 1.0, "resets_at": 1}),
            ),
            (
                "five_hour",
                json!({"kind": "five_hour", "label": "5h", "used_percent": 1.0, "resets_at": 1}),
            ),
            (
                "credits",
                json!({"kind": "credits", "label": "c", "remaining": {"kind": "amount", "value": "1", "unit": "credits"}, "resets_at": null}),
            ),
        ] {
            let scenario = zai_scenario(id).await;
            scenario.knobs().usage = Some(vec![super::scenario::UsageScript {
                a1: json!({"state": "windows", "windows": [window]}),
                fixtures: json!({"state": "unavailable", "reason": {"kind": "expired_login"}}),
                fxw: None,
                drop_account: None,
            }]);
            let ran = scenario.run_with_env(&[("ZAI_API_KEY", ZAI)]).await;
            ran.expect(&format!("{id} {name}"), &[("J17", "FAIL")]);
        }

        for wrong in ["fxr", "fxw"] {
            let scenario = zai_scenario(id).await;
            let expired = json!({"state": "unavailable", "reason": {"kind": "expired_login"}});
            let network = json!({"state": "unavailable", "reason": {"kind": "network"}});
            scenario.knobs().usage = Some(vec![super::scenario::UsageScript {
                a1: zai_a1_usage(),
                fixtures: if wrong == "fxr" {
                    network.clone()
                } else {
                    expired.clone()
                },
                fxw: Some(if wrong == "fxw" { network } else { expired }),
                drop_account: None,
            }]);
            let ran = scenario.run_with_env(&[("ZAI_API_KEY", ZAI)]).await;
            let case = format!("{id} {wrong} wrong reason");
            ran.expect(&case, &[("J17", "FAIL")]);
            ran.reason_has(
                &case,
                "J17",
                &format!(
                    "the fixture account e2e-abcd-{wrong} returned network, expected expired_login"
                ),
            );
            assert_eq!(scenario.state().usage_calls, 1, "{case}");
        }

        for wrong in ["fxr", "fxw"] {
            let scenario = zai_scenario(id).await;
            let accounts = zai_usage_accounts(wrong);
            let output = format!(
                "{}\n",
                json!({"schema": 1, "generated_at": 0, "accounts": accounts})
            );
            scenario.host_reply_rule("usage --json", None, 0, &output, "");
            let ran = scenario.run_with_env(&[("ZAI_API_KEY", ZAI)]).await;
            let case = format!("{id} {wrong} wrong provider");
            ran.expect(&case, &[("J17", "FAIL")]);
            ran.reason_has(
                &case,
                "J17",
                &format!("usage record e2e-abcd-{wrong} names provider codex, expected zai"),
            );
        }

        let scenario = zai_scenario(id).await;
        scenario.knobs().usage = Some(vec![super::scenario::UsageScript {
            a1: zai_a1_usage(),
            fixtures: json!({"state": "unavailable", "reason": {"kind": "expired_login"}}),
            fxw: None,
            drop_account: None,
        }]);
        expect_full_success(id, &scenario.run_with_env(&[("ZAI_API_KEY", ZAI)]).await);
    }
}
