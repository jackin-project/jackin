#![expect(
    clippy::expect_used,
    reason = "integration test prompt fixtures should fail immediately when expected defaults are absent"
)]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;

use jackin_core::Agent;
use jackin_env::{EnvPrompter, PromptResult, resolve_env};

fn sentinel_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/roles/jackin-sentinel")
}

struct SentinelPrompter {
    calls: RefCell<Vec<String>>,
}

impl SentinelPrompter {
    const fn new() -> Self {
        Self {
            calls: RefCell::new(Vec::new()),
        }
    }
}

impl EnvPrompter for SentinelPrompter {
    fn prompt_text(
        &self,
        title: &str,
        default: Option<&str>,
        skippable: bool,
    ) -> anyhow::Result<PromptResult> {
        self.calls
            .borrow_mut()
            .push(format!("text:{title}:{default:?}:{skippable}"));
        Ok(match title {
            "Sentinel free text:" => PromptResult::Value(
                default
                    .expect("free text prompt should carry its default")
                    .to_owned(),
            ),
            "Required sentinel value:" => PromptResult::Value("required-value".to_owned()),
            "Optional sentinel API key:" => PromptResult::Skipped,
            "Branch for frontend:" => PromptResult::Value(
                default
                    .expect("branch prompt should interpolate its default")
                    .to_owned(),
            ),
            "Combined label for frontend:" => PromptResult::Value(
                default
                    .expect("combined prompt should interpolate its default")
                    .to_owned(),
            ),
            other => anyhow::bail!("unexpected text prompt {other:?}"),
        })
    }

    fn prompt_select(
        &self,
        title: &str,
        options: &[String],
        default: Option<&str>,
        skippable: bool,
    ) -> anyhow::Result<PromptResult> {
        self.calls.borrow_mut().push(format!(
            "select:{title}:{options:?}:{default:?}:{skippable}"
        ));
        Ok(match title {
            "Select sentinel project:" => PromptResult::Value("frontend".to_owned()),
            "Select sentinel mode:" => PromptResult::Value(
                default
                    .expect("select mode prompt should carry its default")
                    .to_owned(),
            ),
            other => anyhow::bail!("unexpected select prompt {other:?}"),
        })
    }
}

#[test]
fn sentinel_role_covers_supported_agents_hooks_and_env_shapes() {
    let manifest = jackin_manifest::load_role_manifest(&sentinel_dir()).unwrap();

    assert_eq!(
        manifest.supported_agents(),
        vec![
            Agent::Claude,
            Agent::Codex,
            Agent::Amp,
            Agent::Kimi,
            Agent::Opencode,
            Agent::Grok
        ]
    );

    let hooks = manifest
        .hooks
        .as_ref()
        .expect("sentinel should declare hooks");
    assert_eq!(hooks.setup_once.as_deref(), Some("hooks/setup-once.sh"));
    assert_eq!(hooks.source.as_deref(), Some("hooks/source.sh"));
    assert_eq!(hooks.preflight.as_deref(), Some("hooks/preflight.sh"));

    assert!(manifest.env["FREE_TEXT"].interactive);
    assert!(manifest.env["SELECT_PROJECT"].interactive);
    assert!(!manifest.env["SELECT_PROJECT"].options.is_empty());
    assert!(manifest.env["OPTIONAL_API_KEY"].skippable);
    assert_eq!(
        manifest.env["BRANCH"].depends_on,
        vec!["env.SELECT_PROJECT"]
    );
    assert_eq!(
        manifest.env["COMBINED_LABEL"].depends_on,
        vec!["env.FREE_TEXT", "env.SELECT_PROJECT"]
    );
}

#[test]
fn sentinel_env_resolution_exercises_defaults_interpolation_and_skip_cascade() {
    let manifest = jackin_manifest::load_role_manifest(&sentinel_dir()).unwrap();
    let prompter = SentinelPrompter::new();

    let resolved = resolve_env(&manifest.env, &prompter).unwrap();
    let vars: BTreeMap<_, _> = resolved.vars.into_iter().collect();

    assert_eq!(vars["STATIC_DEFAULT"], "static-value");
    assert_eq!(vars["LITERAL_TEMPLATE"], "preserve-${other.VALUE}");
    assert_eq!(vars["FREE_TEXT"], "typed-default");
    assert_eq!(vars["FREE_TEXT_REQUIRED"], "required-value");
    assert_eq!(vars["SELECT_PROJECT"], "frontend");
    assert_eq!(vars["SELECT_MODE"], "diagnostic");
    assert_eq!(vars["BRANCH"], "feature/frontend");
    assert_eq!(vars["COMBINED_LABEL"], "frontend-typed-default");
    assert!(!vars.contains_key("OPTIONAL_API_KEY"));
    assert!(!vars.contains_key("OPTIONAL_DERIVED"));

    let calls = prompter.calls.borrow().join("\n");
    assert!(calls.contains("text:Sentinel free text:"));
    assert!(calls.contains("select:Select sentinel project:"));
    assert!(calls.contains("text:Branch for frontend:"));
    assert!(calls.contains("text:Combined label for frontend:"));
    assert!(
        !calls.contains("Value derived from optional key:"),
        "dependent prompt must be skipped after OPTIONAL_API_KEY is skipped:\n{calls}"
    );
}
