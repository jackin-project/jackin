// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `env_resolver`.
use super::*;

struct MockPrompter {
    responses: std::cell::RefCell<Vec<PromptResult>>,
    captured_titles: std::cell::RefCell<Vec<String>>,
    captured_defaults: std::cell::RefCell<Vec<Option<String>>>,
}

impl MockPrompter {
    fn new(responses: Vec<PromptResult>) -> Self {
        Self {
            responses: std::cell::RefCell::new(responses),
            captured_titles: std::cell::RefCell::new(vec![]),
            captured_defaults: std::cell::RefCell::new(vec![]),
        }
    }
}

impl EnvPrompter for MockPrompter {
    fn prompt_text(
        &self,
        title: &str,
        default: Option<&str>,
        _skippable: bool,
    ) -> anyhow::Result<PromptResult> {
        self.captured_titles.borrow_mut().push(title.to_owned());
        self.captured_defaults
            .borrow_mut()
            .push(default.map(String::from));
        Ok(self.responses.borrow_mut().remove(0))
    }

    fn prompt_select(
        &self,
        title: &str,
        _options: &[String],
        default: Option<&str>,
        _skippable: bool,
    ) -> anyhow::Result<PromptResult> {
        self.captured_titles.borrow_mut().push(title.to_owned());
        self.captured_defaults
            .borrow_mut()
            .push(default.map(String::from));
        Ok(self.responses.borrow_mut().remove(0))
    }
}

struct ErrorPrompter;

impl EnvPrompter for ErrorPrompter {
    fn prompt_text(
        &self,
        _title: &str,
        _default: Option<&str>,
        _skippable: bool,
    ) -> anyhow::Result<PromptResult> {
        anyhow::bail!("prompt I/O failed")
    }

    fn prompt_select(
        &self,
        _title: &str,
        _options: &[String],
        _default: Option<&str>,
        _skippable: bool,
    ) -> anyhow::Result<PromptResult> {
        anyhow::bail!("prompt I/O failed")
    }
}

fn static_var(default: &str) -> EnvVarDecl {
    EnvVarDecl {
        default_value: Some(default.to_owned()),
        interactive: false,
        skippable: false,
        prompt: None,
        options: vec![],
        depends_on: vec![],
    }
}

fn interactive_text(prompt: &str) -> EnvVarDecl {
    EnvVarDecl {
        default_value: None,
        interactive: true,
        skippable: false,
        prompt: Some(prompt.to_owned()),
        options: vec![],
        depends_on: vec![],
    }
}

fn interactive_select(prompt: &str, options: Vec<&str>) -> EnvVarDecl {
    EnvVarDecl {
        default_value: None,
        interactive: true,
        skippable: false,
        prompt: Some(prompt.to_owned()),
        options: options.into_iter().map(String::from).collect(),
        depends_on: vec![],
    }
}

#[test]
fn resolves_static_vars_without_prompting() {
    let mut decls = BTreeMap::new();
    decls.insert("JACKIN".to_owned(), static_var("docker"));
    let prompter = MockPrompter::new(vec![]);

    let resolved = resolve_env(&decls, &prompter).unwrap();

    assert_eq!(
        resolved.vars,
        vec![("JACKIN".to_owned(), "docker".to_owned())]
    );
}

#[test]
fn resolves_interactive_text_var() {
    let mut decls = BTreeMap::new();
    decls.insert("BRANCH".to_owned(), interactive_text("Branch:"));
    let prompter = MockPrompter::new(vec![PromptResult::Value("main".to_owned())]);

    let resolved = resolve_env(&decls, &prompter).unwrap();

    assert_eq!(
        resolved.vars,
        vec![("BRANCH".to_owned(), "main".to_owned())]
    );
}

#[test]
fn resolves_interactive_select_var() {
    let mut decls = BTreeMap::new();
    decls.insert(
        "PROJECT".to_owned(),
        interactive_select("Pick:", vec!["a", "b"]),
    );
    let prompter = MockPrompter::new(vec![PromptResult::Value("b".to_owned())]);

    let resolved = resolve_env(&decls, &prompter).unwrap();

    assert_eq!(resolved.vars, vec![("PROJECT".to_owned(), "b".to_owned())]);
}

#[test]
fn skippable_var_can_be_skipped() {
    let mut decls = BTreeMap::new();
    let mut var = interactive_text("API key:");
    var.skippable = true;
    decls.insert("API_KEY".to_owned(), var);
    let prompter = MockPrompter::new(vec![PromptResult::Skipped]);

    let resolved = resolve_env(&decls, &prompter).unwrap();

    assert!(resolved.vars.is_empty());
}

#[test]
fn required_var_cannot_be_skipped() {
    let mut decls = BTreeMap::new();
    decls.insert("BRANCH".to_owned(), interactive_text("Branch:"));
    let prompter = MockPrompter::new(vec![PromptResult::Skipped]);

    let Err(error) = resolve_env(&decls, &prompter) else {
        panic!("required skipped var should fail");
    };

    assert!(error.to_string().contains("BRANCH"));
    assert!(error.to_string().contains("skip"));
}

#[test]
fn prompt_errors_are_propagated() {
    let mut decls = BTreeMap::new();
    decls.insert("BRANCH".to_owned(), interactive_text("Branch:"));

    let Err(error) = resolve_env(&decls, &ErrorPrompter) else {
        panic!("prompt I/O failures should bubble up");
    };

    assert!(error.to_string().contains("prompt I/O failed"));
}

#[test]
fn skip_cascades_to_dependents() {
    let mut decls = BTreeMap::new();
    let mut project = interactive_select("Pick:", vec!["a", "b"]);
    project.skippable = true;
    decls.insert("PROJECT".to_owned(), project);

    let mut branch = interactive_text("Branch:");
    branch.depends_on = vec!["env.PROJECT".to_owned()];
    decls.insert("BRANCH".to_owned(), branch);

    let prompter = MockPrompter::new(vec![PromptResult::Skipped]);

    let resolved = resolve_env(&decls, &prompter).unwrap();

    assert!(resolved.vars.is_empty());
}

#[test]
fn skip_cascades_through_chain() {
    let mut decls = BTreeMap::new();

    let mut a = interactive_text("A:");
    a.skippable = true;
    decls.insert("A".to_owned(), a);

    let mut b = interactive_text("B:");
    b.depends_on = vec!["env.A".to_owned()];
    decls.insert("B".to_owned(), b);

    let mut c = interactive_text("C:");
    c.depends_on = vec!["env.B".to_owned()];
    decls.insert("C".to_owned(), c);

    let prompter = MockPrompter::new(vec![PromptResult::Skipped]);

    let resolved = resolve_env(&decls, &prompter).unwrap();

    assert!(resolved.vars.is_empty());
}

#[test]
fn dependency_order_is_respected() {
    let mut decls = BTreeMap::new();

    let mut branch = interactive_text("Branch:");
    branch.depends_on = vec!["env.PROJECT".to_owned()];
    decls.insert("BRANCH".to_owned(), branch);

    decls.insert(
        "PROJECT".to_owned(),
        interactive_select("Pick:", vec!["a", "b"]),
    );

    let prompter = MockPrompter::new(vec![
        PromptResult::Value("a".to_owned()),
        PromptResult::Value("main".to_owned()),
    ]);

    let resolved = resolve_env(&decls, &prompter).unwrap();

    assert_eq!(resolved.vars[0].0, "PROJECT");
    assert_eq!(resolved.vars[1].0, "BRANCH");
}

#[test]
fn empty_declarations_returns_empty() {
    let decls = BTreeMap::new();
    let prompter = MockPrompter::new(vec![]);

    let resolved = resolve_env(&decls, &prompter).unwrap();

    assert!(resolved.vars.is_empty());
}

#[test]
fn interpolates_prompt_with_resolved_value() {
    let mut decls = BTreeMap::new();
    decls.insert(
        "PROJECT".to_owned(),
        interactive_select("Select a project:", vec!["alpha", "beta"]),
    );

    let mut branch = interactive_text("Branch for ${env.PROJECT}:");
    branch.depends_on = vec!["env.PROJECT".to_owned()];
    decls.insert("BRANCH".to_owned(), branch);

    let prompter = MockPrompter::new(vec![
        PromptResult::Value("alpha".to_owned()),
        PromptResult::Value("main".to_owned()),
    ]);

    resolve_env(&decls, &prompter).unwrap();

    let titles = prompter.captured_titles.borrow();
    assert_eq!(titles[1], "Branch for alpha:");
}

#[test]
fn interpolates_default_value_with_resolved_value() {
    let mut decls = BTreeMap::new();
    decls.insert(
        "PROJECT".to_owned(),
        interactive_select("Select:", vec!["proj1", "proj2"]),
    );

    let branch = EnvVarDecl {
        default_value: Some("feature/${env.PROJECT}".to_owned()),
        interactive: true,
        skippable: false,
        prompt: Some("Branch:".to_owned()),
        options: vec![],
        depends_on: vec!["env.PROJECT".to_owned()],
    };
    decls.insert("BRANCH".to_owned(), branch);

    let prompter = MockPrompter::new(vec![
        PromptResult::Value("proj1".to_owned()),
        PromptResult::Value("feature/proj1".to_owned()),
    ]);

    resolve_env(&decls, &prompter).unwrap();

    let defaults = prompter.captured_defaults.borrow();
    assert_eq!(defaults[1], Some("feature/proj1".to_owned()));
}

#[test]
fn operator_overrides_preseed_interactive_manifest_env() {
    let mut decls = BTreeMap::new();
    decls.insert(
        "PROJECT".to_owned(),
        interactive_select("Select:", vec!["api", "web"]),
    );

    let branch = EnvVarDecl {
        default_value: Some("feature/${env.PROJECT}".to_owned()),
        interactive: true,
        skippable: false,
        prompt: Some("Branch for ${env.PROJECT}:".to_owned()),
        options: vec![],
        depends_on: vec!["env.PROJECT".to_owned()],
    };
    decls.insert("BRANCH".to_owned(), branch);

    let prompter = MockPrompter::new(vec![PromptResult::Value("feature/web".to_owned())]);
    let overrides = BTreeMap::from([("PROJECT".to_owned(), "web".to_owned())]);
    let resolved = resolve_env_with_overrides(&decls, &prompter, &overrides).unwrap();

    assert_eq!(resolved.vars[0], ("PROJECT".to_owned(), "web".to_owned()));
    assert_eq!(
        resolved.vars[1],
        ("BRANCH".to_owned(), "feature/web".to_owned())
    );
    let titles = prompter.captured_titles.borrow();
    assert_eq!(titles.as_slice(), ["Branch for web:"]);
}

#[test]
fn operator_override_wins_over_skipped_dependency_cascade() {
    let mut decls = BTreeMap::new();
    let mut project = interactive_select("Select:", vec!["api", "web"]);
    project.skippable = true;
    decls.insert("PROJECT".to_owned(), project);

    let mut branch = interactive_text("Branch:");
    branch.depends_on = vec!["env.PROJECT".to_owned()];
    decls.insert("BRANCH".to_owned(), branch);

    // PROJECT is skipped, which would normally cascade-skip BRANCH — but
    // BRANCH carries an operator override. The override check runs before
    // the dep-skipped check, so the override survives the cascade.
    let prompter = MockPrompter::new(vec![PromptResult::Skipped]);
    let overrides = BTreeMap::from([("BRANCH".to_owned(), "hotfix".to_owned())]);
    let resolved = resolve_env_with_overrides(&decls, &prompter, &overrides).unwrap();

    assert_eq!(
        resolved.vars,
        vec![("BRANCH".to_owned(), "hotfix".to_owned())]
    );
}

#[test]
fn interpolates_static_default_value() {
    let mut decls = BTreeMap::new();
    decls.insert(
        "PROJECT".to_owned(),
        interactive_select("Select:", vec!["proj1", "proj2"]),
    );

    let derived = EnvVarDecl {
        default_value: Some("${env.PROJECT}-derived".to_owned()),
        interactive: false,
        skippable: false,
        prompt: None,
        options: vec![],
        depends_on: vec!["env.PROJECT".to_owned()],
    };
    decls.insert("DERIVED".to_owned(), derived);

    let prompter = MockPrompter::new(vec![PromptResult::Value("proj1".to_owned())]);

    let resolved = resolve_env(&decls, &prompter).unwrap();

    assert_eq!(
        resolved.vars,
        vec![
            ("PROJECT".to_owned(), "proj1".to_owned()),
            ("DERIVED".to_owned(), "proj1-derived".to_owned()),
        ]
    );
}

#[test]
fn interpolates_multiple_refs_in_one_field() {
    let mut decls = BTreeMap::new();
    decls.insert("TEAM".to_owned(), static_var("backend"));
    decls.insert(
        "PROJECT".to_owned(),
        interactive_select("Select:", vec!["api", "web"]),
    );

    let label = EnvVarDecl {
        default_value: Some("${env.TEAM}/${env.PROJECT}".to_owned()),
        interactive: true,
        skippable: false,
        prompt: Some("Label for ${env.TEAM}/${env.PROJECT}:".to_owned()),
        options: vec![],
        depends_on: vec!["env.TEAM".to_owned(), "env.PROJECT".to_owned()],
    };
    decls.insert("LABEL".to_owned(), label);

    let prompter = MockPrompter::new(vec![
        PromptResult::Value("api".to_owned()),
        PromptResult::Value("backend/api".to_owned()),
    ]);

    resolve_env(&decls, &prompter).unwrap();

    let titles = prompter.captured_titles.borrow();
    let defaults = prompter.captured_defaults.borrow();
    // LABEL is the second prompt (PROJECT is first, TEAM is static)
    assert_eq!(titles[1], "Label for backend/api:");
    assert_eq!(defaults[1], Some("backend/api".to_owned()));
}

#[test]
fn resolved_values_containing_dollar_brace_are_not_re_interpolated() {
    let mut decls = BTreeMap::new();

    // User types a value that looks like an interpolation placeholder
    decls.insert("A".to_owned(), interactive_text("Enter A:"));
    decls.insert("B".to_owned(), static_var("secret"));

    let c = EnvVarDecl {
        default_value: Some("prefix-${env.A}-suffix".to_owned()),
        interactive: false,
        skippable: false,
        prompt: None,
        options: vec![],
        depends_on: vec!["env.A".to_owned()],
    };
    decls.insert("C".to_owned(), c);

    // User enters a value that looks like an interpolation ref
    let prompter = MockPrompter::new(vec![PromptResult::Value("${env.B}".to_owned())]);

    let resolved = resolve_env(&decls, &prompter).unwrap();

    // C should be "prefix-${env.B}-suffix" (literal), NOT "prefix-secret-suffix"
    let c_value = resolved.vars.iter().find(|(k, _)| k == "C").unwrap();
    assert_eq!(c_value.1, "prefix-${env.B}-suffix");
}

#[test]
fn no_interpolation_without_placeholders() {
    let mut decls = BTreeMap::new();
    decls.insert("BRANCH".to_owned(), interactive_text("Branch name:"));

    let prompter = MockPrompter::new(vec![PromptResult::Value("main".to_owned())]);

    resolve_env(&decls, &prompter).unwrap();

    let titles = prompter.captured_titles.borrow();
    assert_eq!(titles[0], "Branch name:");
}

#[test]
fn required_prompt_skip_is_typed_source() {
    let mut decls = BTreeMap::new();
    decls.insert("NEED".to_owned(), interactive_text("Need:"));
    let prompter = MockPrompter::new(vec![PromptResult::Skipped]);
    let err = resolve_env(&decls, &prompter).expect_err("required prompt cannot skip");
    assert_eq!(
        err.to_string(),
        "env var NEED: required prompt cannot be skipped"
    );
    let typed = err
        .downcast_ref::<ResolveEnvError>()
        .expect("ResolveEnvError source");
    match typed {
        ResolveEnvError::PromptRequired { name } => assert_eq!(name, "NEED"),
        ResolveEnvError::Cycle(_) => panic!("expected PromptRequired, got Cycle"),
    }
}

#[test]
fn resolve_env_error_prompt_required_message_parity() {
    let err = ResolveEnvError::PromptRequired { name: "FOO".into() };
    assert_eq!(
        err.to_string(),
        "env var FOO: required prompt cannot be skipped"
    );
}
