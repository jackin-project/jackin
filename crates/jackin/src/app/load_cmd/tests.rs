use super::{apply_dry_run_identity_json, docker_startup_error, take_post_console_config};
use jackin_config::AppConfig;
use jackin_config::{MountConfig, WorkspaceConfig};
use jackin_core::Agent;
use jackin_core::JackinPaths;
use jackin_core::MountIsolation;
use jackin_runtime::runtime::resolve_dry_run_identity;
use tempfile::tempdir;

#[test]
fn docker_startup_error_includes_visible_detail() {
    let error = anyhow::anyhow!(
        "failed to connect to Docker daemon: connect to Docker host unix:///tmp/missing.sock"
    );

    let (title, message) = docker_startup_error(&error);

    assert_eq!(title, "Docker daemon not reachable");
    assert!(message.contains("jackin could not connect to the Docker daemon."));
    assert!(message.contains("failed to connect to Docker daemon"));
    assert!(message.contains("connect to Docker host unix:///tmp/missing.sock"));
    assert!(message.contains("Start Docker or switch to a reachable Docker context"));
}

/// Launch-speed 008g: a no-op console (no settings/workspace save) must not
/// re-read disk. If something else mutates the on-disk file after the console
/// returns its in-memory model, post-console still uses the returned model.
#[test]
fn no_op_console_skips_disk_reload_for_post_console_config() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut on_disk = AppConfig::load_or_init(&paths).unwrap();
    on_disk.env.insert(
        "JACKIN_TEST_NOOP".to_owned(),
        jackin_core::EnvValue::Plain("from-console".to_owned()),
    );
    // Persist the "console-owned" snapshot, then poison disk with a different
    // value that a reload would pick up.
    std::fs::write(
        &paths.config_file,
        toml::to_string(&on_disk).expect("serialize console snapshot"),
    )
    .unwrap();
    let console_owned = AppConfig::load_or_init(&paths).unwrap();
    assert_eq!(
        console_owned
            .env
            .get("JACKIN_TEST_NOOP")
            .map(jackin_core::EnvValue::as_persisted_str),
        Some("from-console")
    );

    let mut poisoned = console_owned.clone();
    poisoned.env.insert(
        "JACKIN_TEST_NOOP".to_owned(),
        jackin_core::EnvValue::Plain("from-disk-after-console".to_owned()),
    );
    std::fs::write(
        &paths.config_file,
        toml::to_string(&poisoned).expect("serialize poisoned disk"),
    )
    .unwrap();

    // Shipped path: use the returned console config, not load_or_init.
    let post = take_post_console_config(console_owned);
    assert_eq!(
        post.env
            .get("JACKIN_TEST_NOOP")
            .map(jackin_core::EnvValue::as_persisted_str),
        Some("from-console"),
        "no-op console path must keep the returned model and ignore later disk writes"
    );
    let reloaded = AppConfig::load_or_init(&paths).unwrap();
    assert_eq!(
        reloaded
            .env
            .get("JACKIN_TEST_NOOP")
            .map(jackin_core::EnvValue::as_persisted_str),
        Some("from-disk-after-console"),
        "control: disk really changed; reload would have returned the poison"
    );
}

/// Launch-speed 008g: after a successful settings/workspace save the console
/// mutates its in-memory `AppConfig`; that value must feed the next launch even
/// if disk is still lagging or was replaced underfoot.
#[test]
fn saved_console_config_feeds_post_console_launch_path() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let workspace_root = temp.path().join("project");
    std::fs::create_dir_all(&workspace_root).unwrap();
    let canonical = workspace_root.canonicalize().unwrap();

    // Disk starts without the saved workspace.
    let disk_before = AppConfig::load_or_init(&paths).unwrap();
    assert!(!disk_before.workspaces.contains_key("saved-ws"));

    // Console save path updates the in-memory model (mirrors *config = saved).
    let mut console_owned = disk_before;
    console_owned.workspaces.insert(
        "saved-ws".to_owned(),
        WorkspaceConfig {
            workdir: "/workspace/project".to_owned(),
            mounts: vec![MountConfig {
                src: canonical.display().to_string(),
                dst: "/workspace/project".to_owned(),
                readonly: false,
                isolation: MountIsolation::Shared,
            }],
            default_agent: Some(Agent::Codex),
            ..Default::default()
        },
    );

    let post = take_post_console_config(console_owned);
    assert!(
        post.workspaces.contains_key("saved-ws"),
        "post-console launch must see the workspace the console saved in memory"
    );
    assert_eq!(
        post.workspaces
            .get("saved-ws")
            .and_then(|ws| ws.default_agent),
        Some(Agent::Codex)
    );

    // Disk still lacks the workspace (save may write asynchronously / tests
    // prove in-memory handoff, not the background writer).
    let still_disk = AppConfig::load_or_init(&paths).unwrap();
    assert!(
        !still_disk.workspaces.contains_key("saved-ws"),
        "control: disk never received the save; only the returned model carries it"
    );
}

/// `--dry-run --format json` promises the *resolved* plan. `image_decision`
/// and `published_image` are resolvable only after the role manifest is read,
/// so their presence in the JSON is the contract these tests hold (D-078).
fn dry_run_workspace() -> crate::workspace::ResolvedWorkspace {
    crate::workspace::ResolvedWorkspace {
        name: "big-monorepo".to_owned(),
        label: "big-monorepo".to_owned(),
        workdir: "/workspace/big-monorepo".to_owned(),
        mounts: vec![MountConfig {
            src: "/host/big-monorepo".to_owned(),
            dst: "/workspace/big-monorepo".to_owned(),
            readonly: false,
            isolation: MountIsolation::Shared,
        }],
        keep_awake_enabled: false,
        default_agent: Some(Agent::Claude),
        git_pull_on_entry: false,
        mount_heal: jackin_config::MountHealReport::default(),
    }
}

fn dry_run_image_plan() -> jackin_runtime::runtime::LaunchImagePlan {
    jackin_runtime::runtime::LaunchImagePlan {
        decision: "build_from_published",
        reason: Some("role_git_sha_changed"),
        image: "jk_the-architect:deadbee".to_owned(),
        base_image: Some("projectjackin/the-architect:latest".to_owned()),
        role_git_sha: Some("deadbee".to_owned()),
        published_image: Some("projectjackin/the-architect:latest".to_owned()),
    }
}

#[test]
fn dry_run_json_carries_the_resolved_image_decision_and_published_image() {
    let selector = jackin_core::RoleSelector::parse("donbeave/the-architect").unwrap();
    let plan = super::dry_run_plan_json(
        &selector,
        &dry_run_workspace(),
        "claude",
        None,
        false,
        &dry_run_image_plan(),
    );
    let data = &plan["data"];
    assert_eq!(plan["schema_version"], "v1");
    assert_eq!(
        data["published_image"],
        "projectjackin/the-architect:latest"
    );
    assert_eq!(data["image_decision"]["decision"], "build_from_published");
    assert_eq!(data["image_decision"]["reason"], "role_git_sha_changed");
    assert_eq!(data["image_decision"]["image"], "jk_the-architect:deadbee");
    assert_eq!(
        data["image_decision"]["base_image"],
        "projectjackin/the-architect:latest"
    );
    assert_eq!(data["image_decision"]["role_git_sha"], "deadbee");
}

#[test]
fn dry_run_json_keeps_every_pre_existing_key() {
    let selector = jackin_core::RoleSelector::parse("donbeave/the-architect").unwrap();
    let plan = super::dry_run_plan_json(
        &selector,
        &dry_run_workspace(),
        "codex",
        Some("feat/my-pr"),
        true,
        &dry_run_image_plan(),
    );
    let data = &plan["data"];
    for key in [
        "workspace",
        "workdir",
        "role",
        "role_branch",
        "agent",
        "rebuild",
        "mounts",
        "image_decision",
        "published_image",
    ] {
        assert!(
            data.get(key).is_some(),
            "the dry-run plan must keep carrying {key}, got {data}"
        );
    }
    assert_eq!(data["agent"], "codex");
    assert_eq!(data["role_branch"], "feat/my-pr");
    assert_eq!(data["rebuild"], true);
    assert_eq!(
        data["mounts"][0]["container_dest"],
        "/workspace/big-monorepo"
    );
}

#[test]
fn a_role_without_a_published_image_reports_it_as_null_not_missing() {
    let selector = jackin_core::RoleSelector::parse("donbeave/the-architect").unwrap();
    let mut image_plan = dry_run_image_plan();
    image_plan.published_image = None;
    image_plan.base_image = None;
    image_plan.decision = "build_from_workspace";
    let plan = super::dry_run_plan_json(
        &selector,
        &dry_run_workspace(),
        "claude",
        None,
        false,
        &image_plan,
    );
    let data = &plan["data"];
    assert!(
        data["published_image"].is_null(),
        "an absent published_image must be an explicit null, not an absent key"
    );
    assert!(data["image_decision"]["base_image"].is_null());
    assert_eq!(data["image_decision"]["decision"], "build_from_workspace");
}

fn dry_run_identity_config() -> (AppConfig, jackin_core::WorkspaceName) {
    use jackin_config::{AccountConfig, AccountCredential, AgentConfiguration, AiProvider};
    let mut config = AppConfig::default();
    for (id, display, provider) in [
        ("a-claude", "Work", AiProvider::Anthropic),
        ("b-claude", "Personal", AiProvider::Anthropic),
        ("c-codex", "Work", AiProvider::OpenAi),
    ] {
        config.accounts.insert(
            id.to_owned(),
            AccountConfig {
                enabled: true,
                name: display.into(),
                provider,
                credential: AccountCredential::ApiKey {
                    value: jackin_core::EnvValue::Plain("test-key".into()),
                    base_url: None,
                    model: None,
                },
            },
        );
    }
    for (id, agent, account) in [
        ("claude-work", Agent::Claude, "a-claude"),
        ("claude-personal", Agent::Claude, "b-claude"),
        ("codex-work", Agent::Codex, "c-codex"),
    ] {
        config.agent_configurations.insert(
            id.to_owned(),
            AgentConfiguration {
                agent,
                account: account.into(),
                model: None,
                base_url: None,
                display_label: None,
                invoked_via_wrapper: None,
            },
        );
    }
    let ws = jackin_core::WorkspaceName::parse("demo").unwrap();
    config.workspaces.insert(
        ws.as_str().to_owned(),
        WorkspaceConfig {
            workdir: "/demo".into(),
            accounts: vec!["a-claude".into(), "b-claude".into(), "c-codex".into()],
            default_launch: Some(vec![
                "claude-work".into(),
                "claude-personal".into(),
                "codex-work".into(),
            ]),
            ..Default::default()
        },
    );
    (config, ws)
}

#[test]
fn dry_run_identity_lists_every_instance_when_no_single_account_resolves() {
    let (config, ws) = dry_run_identity_config();
    let identity =
        resolve_dry_run_identity(&config, Agent::Claude, Some(&ws), "smith", false).unwrap();
    assert_eq!(identity.account_id, None);
    let ids: Vec<&str> = identity
        .instances
        .iter()
        .map(|instance| instance.config_id.as_str())
        .collect();
    assert_eq!(ids, ["claude-work", "claude-personal", "codex-work"]);
    assert_eq!(identity.instances[0].label, "Claude · Work");
    assert_eq!(identity.instances[0].account_id, "a-claude");
    assert_eq!(identity.instances[2].label, "Codex · Work");
}

#[test]
fn dry_run_identity_keeps_single_account_shape_for_unambiguous_launches() {
    let (mut config, ws) = dry_run_identity_config();
    config.workspaces.get_mut(ws.as_str()).unwrap().accounts = vec!["c-codex".into()];
    config
        .workspaces
        .get_mut(ws.as_str())
        .unwrap()
        .default_launch = None;
    let identity =
        resolve_dry_run_identity(&config, Agent::Codex, Some(&ws), "smith", false).unwrap();
    assert_eq!(identity.account_id.as_deref(), Some("c-codex"));
    assert!(identity.instances.is_empty());
}

#[test]
fn dry_run_identity_carries_the_account_model_pin() {
    let (mut config, ws) = dry_run_identity_config();
    config.workspaces.get_mut(ws.as_str()).unwrap().accounts = vec!["c-codex".into()];
    config
        .workspaces
        .get_mut(ws.as_str())
        .unwrap()
        .default_launch = None;
    let jackin_config::AccountCredential::ApiKey { model, .. } =
        &mut config.accounts.get_mut("c-codex").unwrap().credential
    else {
        panic!("fixture account must be an API-key account");
    };
    *model = Some("gpt-5-codex".to_owned());
    let identity =
        resolve_dry_run_identity(&config, Agent::Codex, Some(&ws), "smith", false).unwrap();
    assert_eq!(identity.account_id.as_deref(), Some("c-codex"));
    assert_eq!(identity.model.as_deref(), Some("gpt-5-codex"));
}

#[test]
fn dry_run_json_emits_the_model_pin_at_top_level_and_per_instance() {
    use jackin_config::ResolvedInstance;
    let selector = jackin_core::RoleSelector::parse("donbeave/the-architect").unwrap();
    let mut plan = super::dry_run_plan_json(
        &selector,
        &dry_run_workspace(),
        "claude",
        None,
        false,
        &dry_run_image_plan(),
    );
    let identity = jackin_runtime::runtime::DryRunIdentity {
        account_id: Some("a-claude".to_owned()),
        model: Some("claude-opus-4-6".to_owned()),
        instances: vec![ResolvedInstance {
            config_id: "claude-work".to_owned(),
            agent: Agent::Claude,
            account_id: "a-claude".to_owned(),
            model: Some("claude-opus-4-6".to_owned()),
            base_url: None,
            xdg_roots: None,
            label: "Claude · Work".to_owned(),
            synthesized: true,
        }],
    };
    apply_dry_run_identity_json(&mut plan, &identity);
    let data = &plan["data"];
    assert_eq!(data["account"], "a-claude");
    assert_eq!(data["model"], "claude-opus-4-6");
    assert_eq!(data["instances"][0]["config_id"], "claude-work");
    assert_eq!(data["instances"][0]["agent"], "claude");
    assert_eq!(data["instances"][0]["account"], "a-claude");
    assert_eq!(data["instances"][0]["label"], "Claude · Work");
    assert_eq!(data["instances"][0]["model"], "claude-opus-4-6");
}

#[test]
fn dry_run_json_reports_an_absent_model_pin_as_null_not_missing() {
    let selector = jackin_core::RoleSelector::parse("donbeave/the-architect").unwrap();
    let mut plan = super::dry_run_plan_json(
        &selector,
        &dry_run_workspace(),
        "codex",
        None,
        false,
        &dry_run_image_plan(),
    );
    let identity = jackin_runtime::runtime::DryRunIdentity {
        account_id: Some("c-codex".to_owned()),
        model: None,
        instances: Vec::new(),
    };
    apply_dry_run_identity_json(&mut plan, &identity);
    let data = &plan["data"];
    assert!(data["model"].is_null());
    assert!(data["instances"].as_array().is_some_and(Vec::is_empty));
}
