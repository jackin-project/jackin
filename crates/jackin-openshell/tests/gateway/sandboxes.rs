// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use jackin_core::{
    AccountName, Agent, ChatgptAccountId, ImageId, LabelSelector, LaunchId, Phase, ProfileId,
    ProviderName, SandboxLabels, Secret,
};
use jackin_openshell::{
    ClosedPolicy, Gateway, PolicyDocument, PowerAction, ProviderMaterial, ProviderProfile,
    ProviderSpec, ProviderState, Provisioning, SandboxQuery,
};
use openshell_sdk::raw::proto::{Provider, SandboxPhase, SandboxRestartPolicy};

use super::harness::{WORKSPACE, connect, env, guard};
use super::support::{
    IMAGE_DIGEST, LAUNCH, POLICY, SANDBOX, add_sandbox, create_spec, planned, sandbox_name,
    set_conditions,
};

fn managed() -> [(&'static str, &'static str); 1] {
    [("jackin.dev/managed", "true")]
}

#[tokio::test]
async fn create_then_find_by_launch_label_round_trips_the_server_state() {
    let env = env().await;
    let gateway = connect(&env, WORKSPACE).await;
    let spec = create_spec(SANDBOX, LAUNCH);
    let view = guard(gateway.create_sandbox(&spec)).await.unwrap();
    assert_eq!(view.phase, Phase::Provisioning);
    {
        let state = env.server.state();
        assert_eq!(state.sandboxes.len(), 1);
        let sandbox = &state.sandboxes[&(WORKSPACE.to_owned(), SANDBOX.to_owned())];
        let stored = sandbox.spec.as_ref().unwrap();
        assert_eq!(
            stored.template.as_ref().unwrap().image,
            "jackin.invalid/backend:0123456789abcdef"
        );
        let labels: BTreeMap<_, _> = sandbox
            .metadata
            .as_ref()
            .unwrap()
            .labels
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        let expected: BTreeMap<_, _> = [
            ("jackin.dev/managed", "true"),
            ("jackin.dev/workspace", "My.Project"),
            ("jackin.dev/role", "backend"),
            ("jackin.dev/agent", "codex"),
            ("jackin.dev/account", "work"),
            ("jackin.dev/image", &IMAGE_DIGEST[..32]),
            ("jackin.dev/launch", LAUNCH),
            ("jackin.dev/docker", "off"),
        ]
        .into_iter()
        .collect();
        assert_eq!(labels, expected);
        assert_eq!(
            stored.environment,
            [("JACKIN_LAUNCH_ID".to_owned(), LAUNCH.to_owned())]
                .into_iter()
                .collect()
        );
        assert_eq!(stored.providers, vec!["jackin-work-codex".to_owned()]);
        assert_eq!(
            stored.command,
            vec![
                "/usr/local/bin/jackin-start".to_owned(),
                "codex".to_owned(),
                "--sandbox".to_owned()
            ]
        );
        assert!(stored.tty);
        assert_eq!(stored.restart_policy, SandboxRestartPolicy::Never as i32);
    }
    let found = guard(
        gateway.find_sandboxes(&SandboxQuery::Labels(LabelSelector::launch(
            &LaunchId::parse(LAUNCH).unwrap(),
        ))),
    )
    .await
    .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, sandbox_name(SANDBOX));
    assert_eq!(
        &SandboxLabels::from_labels(&found[0].labels).unwrap(),
        spec.labels()
    );
}

#[tokio::test]
async fn find_follows_every_page_and_managed_hides_foreign_sandboxes() {
    let env = env().await;
    let gateway = connect(&env, WORKSPACE).await;
    {
        let mut state = env.server.state();
        for index in 1..=5 {
            add_sandbox(
                &mut state,
                WORKSPACE,
                &format!("jackin-s{index}"),
                SandboxPhase::Ready,
                None,
                &managed(),
            );
        }
        add_sandbox(
            &mut state,
            WORKSPACE,
            "jackin-foreign",
            SandboxPhase::Ready,
            None,
            &[("team", "runtime")],
        );
        add_sandbox(
            &mut state,
            WORKSPACE,
            "jackin-bare",
            SandboxPhase::Ready,
            None,
            &[],
        );
    }
    let found = guard(gateway.find_sandboxes(&SandboxQuery::Labels(LabelSelector::managed())))
        .await
        .unwrap();
    let names: BTreeSet<String> = found.iter().map(|view| view.name.to_string()).collect();
    assert_eq!(
        names,
        (1..=5)
            .map(|index| format!("jackin-s{index}"))
            .collect::<BTreeSet<_>>()
    );
    let foreign =
        guard(gateway.find_sandboxes(&SandboxQuery::Name(sandbox_name("jackin-foreign"))))
            .await
            .unwrap();
    assert_eq!(foreign.len(), 1);
    assert!(!foreign[0].labels.is_managed());
    let missing = guard(gateway.find_sandboxes(&SandboxQuery::Name(sandbox_name("jackin-nope"))))
        .await
        .unwrap();
    assert!(missing.is_empty());
}

#[tokio::test]
async fn label_selector_text_is_accepted_by_the_gateway_grammar() {
    let env = env().await;
    let gateway = connect(&env, WORKSPACE).await;
    let corpus = [
        LAUNCH.to_owned(),
        "7leading-digit".to_owned(),
        "a-b_c.d".to_owned(),
        "MixedCase9".to_owned(),
        "a".repeat(63),
    ];
    for launch in &corpus {
        add_sandbox(
            &mut env.server.state(),
            WORKSPACE,
            "jackin-hit",
            SandboxPhase::Ready,
            None,
            &[
                ("jackin.dev/managed", "true"),
                ("jackin.dev/launch", launch),
            ],
        );
        add_sandbox(
            &mut env.server.state(),
            WORKSPACE,
            "jackin-miss",
            SandboxPhase::Ready,
            None,
            &[
                ("jackin.dev/managed", "true"),
                ("jackin.dev/launch", "other"),
            ],
        );
        let selector = LabelSelector::launch(&LaunchId::parse(launch).unwrap());
        let found = guard(gateway.find_sandboxes(&SandboxQuery::Labels(selector)))
            .await
            .unwrap();
        let names: Vec<String> = found.iter().map(|view| view.name.to_string()).collect();
        assert_eq!(names, vec!["jackin-hit".to_owned()], "{launch}");
    }
}

#[tokio::test]
async fn phase_and_exit_code_map_to_the_documented_jackin_phase() {
    let env = env().await;
    let gateway = connect(&env, WORKSPACE).await;
    let cases: [(i32, Option<i32>, Phase); 16] = [
        (0, None, Phase::Unknown),
        (1, None, Phase::Provisioning),
        (2, None, Phase::Ready),
        (3, Some(3), Phase::Error { exit_code: Some(3) }),
        (3, None, Phase::Error { exit_code: None }),
        (4, None, Phase::Deleting),
        (5, None, Phase::Unknown),
        (6, None, Phase::Stopping),
        (7, None, Phase::Stopped),
        (8, None, Phase::Starting),
        (9, Some(0), Phase::Completed { exit_code: 0 }),
        (9, Some(4), Phase::Completed { exit_code: 4 }),
        (11, None, Phase::Suspending),
        (12, None, Phase::Suspended),
        (10, None, Phase::Unknown),
        (99, None, Phase::Unknown),
    ];
    for (index, (raw, exit, expected)) in cases.into_iter().enumerate() {
        let name = format!("jackin-p{index}");
        {
            let mut state = env.server.state();
            add_sandbox(&mut state, WORKSPACE, &name, SandboxPhase::Ready, exit, &[]);
            let status = state
                .sandboxes
                .get_mut(&(WORKSPACE.to_owned(), name.clone()))
                .unwrap()
                .status
                .as_mut()
                .unwrap();
            status.phase = raw;
        }
        let found = guard(gateway.find_sandboxes(&SandboxQuery::Name(sandbox_name(&name))))
            .await
            .unwrap();
        assert_eq!(found[0].phase, expected, "{raw}");
    }
}

#[tokio::test]
async fn provisioning_failure_condition_is_reported_with_a_sanitized_message() {
    let env = env().await;
    let gateway = connect(&env, WORKSPACE).await;
    let plain = "wait for image preparation: Operation not permitted (os error 1)";
    let noisy = "\u{1b}[31mred\u{1b}[0m\u{7}";
    for (name, message, labelled) in [("jackin-f1", plain, true), ("jackin-f2", noisy, false)] {
        let labels: &[(&str, &str)] = if labelled { &managed() } else { &[] };
        let mut state = env.server.state();
        add_sandbox(
            &mut state,
            WORKSPACE,
            name,
            SandboxPhase::Error,
            None,
            labels,
        );
        set_conditions(
            &mut state,
            WORKSPACE,
            name,
            &[
                ("WorkloadIdentity", "True", "DriverResolved", "ok"),
                (
                    "ConfigurationReady",
                    "False",
                    "ConfigurationPending",
                    "waiting",
                ),
                ("Ready", "False", "ProvisioningFailed", message),
            ],
        );
    }
    let failed = |view: &jackin_openshell::SandboxView| match &view.provisioning {
        Provisioning::Failed { message } => message.as_str().to_owned(),
        Provisioning::Normal => panic!("expected a failed provisioning"),
    };
    let by_labels = guard(gateway.find_sandboxes(&SandboxQuery::Labels(LabelSelector::managed())))
        .await
        .unwrap();
    assert!(failed(&by_labels[0]).contains("Operation not permitted"));
    let by_name = guard(gateway.find_sandboxes(&SandboxQuery::Name(sandbox_name("jackin-f2"))))
        .await
        .unwrap();
    let message = failed(&by_name[0]);
    assert!(!message.contains('\u{1b}') && !message.contains('\u{7}'));
    assert!(message.contains("red"));
    let detail = guard(gateway.sandbox_detail(&sandbox_name("jackin-f1")))
        .await
        .unwrap();
    assert!(failed(&detail.view).contains("Operation not permitted"));
    {
        let mut state = env.server.state();
        add_sandbox(
            &mut state,
            WORKSPACE,
            "jackin-f3",
            SandboxPhase::Ready,
            None,
            &[],
        );
        set_conditions(
            &mut state,
            WORKSPACE,
            "jackin-f3",
            &[("Ready", "False", "ProvisioningFailed", plain)],
        );
        add_sandbox(
            &mut state,
            WORKSPACE,
            "jackin-f4",
            SandboxPhase::Provisioning,
            None,
            &[],
        );
        set_conditions(
            &mut state,
            WORKSPACE,
            "jackin-f4",
            &[("Ready", "False", "DependenciesNotReady", "waiting")],
        );
    }
    let stopped = guard(gateway.set_power(&sandbox_name("jackin-f3"), PowerAction::Stop))
        .await
        .unwrap();
    assert!(failed(&stopped).contains("Operation not permitted"));
    let normal = guard(gateway.find_sandboxes(&SandboxQuery::Name(sandbox_name("jackin-f4"))))
        .await
        .unwrap();
    assert!(matches!(normal[0].provisioning, Provisioning::Normal));
    let none = guard(gateway.find_sandboxes(&SandboxQuery::Name(sandbox_name("jackin-f1"))))
        .await
        .unwrap();
    assert!(matches!(none[0].provisioning, Provisioning::Failed { .. }));
}

#[tokio::test]
async fn delete_removes_only_the_configured_workspaces_sandbox() {
    let env = env().await;
    let gateway = connect(&env, WORKSPACE).await;
    {
        let mut state = env.server.state();
        add_sandbox(
            &mut state,
            WORKSPACE,
            SANDBOX,
            SandboxPhase::Ready,
            None,
            &[],
        );
        add_sandbox(
            &mut state,
            "default",
            SANDBOX,
            SandboxPhase::Ready,
            None,
            &[],
        );
    }
    guard(gateway.delete_sandbox(&sandbox_name(SANDBOX)))
        .await
        .unwrap();
    {
        let state = env.server.state();
        assert!(
            !state
                .sandboxes
                .contains_key(&(WORKSPACE.to_owned(), SANDBOX.to_owned()))
        );
        assert!(
            state
                .sandboxes
                .contains_key(&("default".to_owned(), SANDBOX.to_owned()))
        );
    }
    guard(gateway.delete_sandbox(&sandbox_name(SANDBOX)))
        .await
        .unwrap();
}

#[tokio::test]
async fn power_actions_move_the_server_phase_and_return_the_view() {
    let env = env().await;
    let gateway = connect(&env, WORKSPACE).await;
    env.server.state().auto_advance = true;
    add_sandbox(
        &mut env.server.state(),
        WORKSPACE,
        SANDBOX,
        SandboxPhase::Ready,
        None,
        &[],
    );
    let name = sandbox_name(SANDBOX);
    let stopped = guard(gateway.set_power(&name, PowerAction::Stop))
        .await
        .unwrap();
    assert_eq!(stopped.phase, Phase::Stopped);
    assert_eq!(
        env.server.state().sandboxes[&(WORKSPACE.to_owned(), SANDBOX.to_owned())]
            .status
            .as_ref()
            .unwrap()
            .phase,
        SandboxPhase::Stopped as i32
    );
    let starting = guard(gateway.set_power(&name, PowerAction::Start))
        .await
        .unwrap();
    assert_eq!(starting.phase, Phase::Starting);
    let found = guard(gateway.find_sandboxes(&SandboxQuery::Name(name)))
        .await
        .unwrap();
    assert_eq!(found[0].phase, Phase::Ready);
}

#[tokio::test]
async fn policy_round_trips_through_create_and_detail() {
    let env = env().await;
    let gateway = connect(&env, WORKSPACE).await;
    let spec = create_spec(SANDBOX, LAUNCH);
    guard(gateway.create_sandbox(&spec)).await.unwrap();
    let sent = openshell_policy::parse_sandbox_policy(POLICY).unwrap();
    {
        let mut state = env.server.state();
        let key = (WORKSPACE.to_owned(), SANDBOX.to_owned());
        assert_eq!(
            state.sandboxes[&key].spec.as_ref().unwrap().policy.as_ref(),
            Some(&sent)
        );
        let provider = Provider {
            metadata: Some(openshell_sdk::raw::proto::ObjectMeta {
                name: "jackin-work-codex".to_owned(),
                ..Default::default()
            }),
            ..Provider::default()
        };
        state.providers.insert(
            (WORKSPACE.to_owned(), "jackin-work-codex".to_owned()),
            provider,
        );
        let rule = openshell_sdk::raw::proto::NetworkPolicyRule {
            name: "_provider_jackin_work_codex".to_owned(),
            endpoints: vec![openshell_sdk::raw::proto::NetworkEndpoint {
                host: "api.openai.com".to_owned(),
                port: 443,
                ports: vec![443],
                protocol: "rest".to_owned(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let info = state.policy_info.entry(key).or_default();
        info.version = 4;
        "h4".clone_into(&mut info.hash);
        info.extra_rules
            .push(("_provider_jackin_work_codex".to_owned(), rule));
    }
    let detail = guard(gateway.sandbox_detail(&sandbox_name(SANDBOX)))
        .await
        .unwrap();
    assert_eq!(
        detail.providers,
        vec![ProviderName::parse("jackin-work-codex").unwrap()]
    );
    assert_eq!(detail.policy.version, 4);
    assert_eq!(detail.policy.hash, "h4");
    assert!(detail.policy.loaded);
    let keys: BTreeSet<String> = detail
        .policy
        .document
        .network_rules()
        .iter()
        .map(|rule| rule.key.to_string())
        .collect();
    assert_eq!(
        keys,
        ["docker_engine", "_provider_jackin_work_codex"]
            .into_iter()
            .map(str::to_owned)
            .collect::<BTreeSet<_>>()
    );
    let stored = PolicyDocument::parse_authored(POLICY).unwrap();
    assert_eq!(stored.network_rules().len(), 1);
    add_sandbox(
        &mut env.server.state(),
        WORKSPACE,
        "jackin-other",
        SandboxPhase::Ready,
        None,
        &[],
    );
    env.server
        .state()
        .policy_info
        .entry((WORKSPACE.to_owned(), "jackin-other".to_owned()))
        .or_default()
        .loaded = false;
    let other = guard(gateway.sandbox_detail(&sandbox_name("jackin-other")))
        .await
        .unwrap();
    assert!(!other.policy.loaded);
}

#[tokio::test]
async fn maximum_length_names_and_labels_the_types_allow_are_accepted_by_the_gateway_rules() {
    let env = env().await;
    let workspace = "a".repeat(19);
    let gateway = connect(&env, &workspace).await;
    let long = "a".repeat(63);
    let image = ImageId::parse(&format!("sha256:{IMAGE_DIGEST}")).unwrap();
    let labels = planned(&long, &long, &"a".repeat(32), LAUNCH).complete(&image);
    let base = create_spec(SANDBOX, LAUNCH);
    let name = sandbox_name("jackin-abcdefg-wxyz");
    assert_eq!(name.as_str().len(), 19);
    let spec = jackin_openshell::SandboxCreate::new(
        name.clone(),
        base.image().clone(),
        labels.clone(),
        Vec::new(),
        ClosedPolicy::parse(POLICY).unwrap(),
        base.main().clone(),
    );
    guard(gateway.create_sandbox(&spec)).await.unwrap();
    assert!(
        env.server
            .state()
            .sandboxes
            .contains_key(&(workspace.clone(), name.to_string()))
    );
    let found = guard(gateway.find_sandboxes(&SandboxQuery::Labels(LabelSelector::managed())))
        .await
        .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(
        &SandboxLabels::from_labels(&found[0].labels).unwrap(),
        &labels
    );
    guard(gateway.delete_sandbox(&name)).await.unwrap();
    assert!(env.server.state().sandboxes.is_empty());
    let account = AccountName::parse(&"a".repeat(32)).unwrap();
    let provider = ProviderName::for_account(&account, Agent::Codex);
    assert_eq!(provider.as_str().len(), 45);
    let codex = &Agent::Codex.spec().openshell;
    guard(gateway.ensure_profile(&ProviderProfile::parse(&codex.chatgpt_profile).unwrap()))
        .await
        .unwrap();
    guard(gateway.put_provider(&ProviderSpec {
        name: provider.clone(),
        profile: ProfileId::parse("codex").unwrap(),
        keys: codex.keys,
        material: ProviderMaterial::ChatgptLogin {
            access_token: Secret::new("access-token-value".to_owned()),
            refresh_token: Secret::new("refresh-token-value".to_owned()),
            client_id: "app_client".to_owned(),
            chatgpt_account_id:
                ChatgptAccountId::parse("0a1b2c3d-4e5f-6071-8293-a4b5c6d7e8f9").unwrap(),
        },
    }))
    .await
    .unwrap();
    {
        let state = env.server.state();
        let stored = &state.providers[&(workspace.clone(), provider.to_string())];
        assert_eq!(stored.metadata.as_ref().unwrap().name, provider.as_str());
    }
    let with_provider = jackin_openshell::SandboxCreate::new(
        name.clone(),
        base.image().clone(),
        labels,
        vec![provider.clone()],
        ClosedPolicy::parse(POLICY).unwrap(),
        base.main().clone(),
    );
    guard(gateway.create_sandbox(&with_provider)).await.unwrap();
    assert_eq!(
        env.server.state().sandboxes[&(workspace.clone(), name.to_string())]
            .spec
            .as_ref()
            .unwrap()
            .providers,
        vec![provider.to_string()]
    );
    assert_eq!(
        guard(gateway.provider_state(&provider)).await.unwrap(),
        ProviderState::Present
    );
    guard(gateway.delete_sandbox(&name)).await.unwrap();
    guard(gateway.delete_provider(&provider)).await.unwrap();
    assert!(env.server.state().providers.is_empty());
    assert_eq!(
        guard(gateway.provider_state(&provider)).await.unwrap(),
        ProviderState::Absent
    );
}
