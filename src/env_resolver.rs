use crate::manifest::EnvVarDecl;
use std::collections::BTreeMap;

pub struct ResolvedEnv {
    pub vars: Vec<(String, String)>,
}

pub enum PromptResult {
    Value(String),
    Skipped,
}

pub trait EnvPrompter {
    fn prompt_text(
        &self,
        title: &str,
        default: Option<&str>,
        skippable: bool,
    ) -> anyhow::Result<PromptResult>;
    fn prompt_select(
        &self,
        title: &str,
        options: &[String],
        default: Option<&str>,
        skippable: bool,
    ) -> anyhow::Result<PromptResult>;
}

/// Replace `${env.VAR_NAME}` placeholders with values from already-resolved vars.
///
/// Uses a single left-to-right scan so that replacement values containing `${...}`
/// are never re-interpreted as placeholders.  Only `${env.*}` references are
/// resolved; other `${...}` forms are preserved as-is.
fn interpolate(template: &str, resolved: &[(String, String)]) -> String {
    let resolved_map: std::collections::HashMap<&str, &str> = resolved
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    let mut result = String::with_capacity(template.len());
    let mut rest = template;

    while let Some(start) = rest.find("${") {
        result.push_str(&rest[..start]);
        let after_open = &rest[start + 2..];
        if let Some(end) = after_open.find('}') {
            let ref_expr = &after_open[..end];
            if let Some(var_name) = ref_expr.strip_prefix("env.") {
                if let Some(&value) = resolved_map.get(var_name) {
                    result.push_str(value);
                } else {
                    // Known namespace but unknown var — preserve as-is
                    result.push_str(&rest[start..=start + 2 + end]);
                }
            } else {
                // Not an env. reference — preserve as-is
                result.push_str(&rest[start..=start + 2 + end]);
            }
            rest = &after_open[end + 1..];
        } else {
            // Unclosed `${` — preserve rest as-is
            result.push_str(&rest[start..]);
            rest = "";
            break;
        }
    }
    result.push_str(rest);
    result
}

pub fn resolve_env(
    declarations: &BTreeMap<String, EnvVarDecl>,
    prompter: &impl EnvPrompter,
) -> anyhow::Result<ResolvedEnv> {
    let order = crate::env_model::topological_env_order(declarations)?;
    let mut vars = Vec::new();
    let mut skipped: std::collections::HashSet<String> = std::collections::HashSet::new();

    for name in &order {
        let decl = &declarations[name];

        // Check if any dependency was skipped — cascade skip
        let dep_skipped = decl.depends_on.iter().any(|dep| {
            dep.strip_prefix("env.")
                .is_some_and(|dep_name| skipped.contains(dep_name))
        });

        if dep_skipped {
            skipped.insert(name.clone());
            continue;
        }

        // Interpolate prompt and default_value using already-resolved vars
        let interpolated_default = decl.default_value.as_deref().map(|d| interpolate(d, &vars));

        if !decl.interactive {
            // Static var — use default
            if let Some(default) = interpolated_default {
                vars.push((name.clone(), default));
            }
            continue;
        }

        // Interactive var — prompt with interpolated fields
        let raw_title = decl.prompt.as_deref().unwrap_or(name.as_str());
        let title = interpolate(raw_title, &vars);

        let result = if decl.options.is_empty() {
            prompter.prompt_text(&title, interpolated_default.as_deref(), decl.skippable)
        } else {
            prompter.prompt_select(
                &title,
                &decl.options,
                interpolated_default.as_deref(),
                decl.skippable,
            )
        }?;

        match result {
            PromptResult::Value(value) => {
                vars.push((name.clone(), value));
            }
            PromptResult::Skipped => {
                if decl.skippable {
                    skipped.insert(name.clone());
                } else {
                    anyhow::bail!("env var {name}: required prompt cannot be skipped");
                }
            }
        }
    }

    Ok(ResolvedEnv { vars })
}

#[cfg(test)]
mod tests {
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
            self.captured_titles.borrow_mut().push(title.to_string());
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
            self.captured_titles.borrow_mut().push(title.to_string());
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
            default_value: Some(default.to_string()),
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
            prompt: Some(prompt.to_string()),
            options: vec![],
            depends_on: vec![],
        }
    }

    fn interactive_select(prompt: &str, options: Vec<&str>) -> EnvVarDecl {
        EnvVarDecl {
            default_value: None,
            interactive: true,
            skippable: false,
            prompt: Some(prompt.to_string()),
            options: options.into_iter().map(String::from).collect(),
            depends_on: vec![],
        }
    }

    #[test]
    fn resolves_static_vars_without_prompting() {
        let mut decls = BTreeMap::new();
        decls.insert("JACKIN_CLAUDE_ENV".to_string(), static_var("docker"));
        let prompter = MockPrompter::new(vec![]);

        let resolved = resolve_env(&decls, &prompter).unwrap();

        assert_eq!(
            resolved.vars,
            vec![("JACKIN_CLAUDE_ENV".to_string(), "docker".to_string())]
        );
    }

    #[test]
    fn resolves_interactive_text_var() {
        let mut decls = BTreeMap::new();
        decls.insert("BRANCH".to_string(), interactive_text("Branch:"));
        let prompter = MockPrompter::new(vec![PromptResult::Value("main".to_string())]);

        let resolved = resolve_env(&decls, &prompter).unwrap();

        assert_eq!(
            resolved.vars,
            vec![("BRANCH".to_string(), "main".to_string())]
        );
    }

    #[test]
    fn resolves_interactive_select_var() {
        let mut decls = BTreeMap::new();
        decls.insert(
            "PROJECT".to_string(),
            interactive_select("Pick:", vec!["a", "b"]),
        );
        let prompter = MockPrompter::new(vec![PromptResult::Value("b".to_string())]);

        let resolved = resolve_env(&decls, &prompter).unwrap();

        assert_eq!(
            resolved.vars,
            vec![("PROJECT".to_string(), "b".to_string())]
        );
    }

    #[test]
    fn skippable_var_can_be_skipped() {
        let mut decls = BTreeMap::new();
        let mut var = interactive_text("API key:");
        var.skippable = true;
        decls.insert("API_KEY".to_string(), var);
        let prompter = MockPrompter::new(vec![PromptResult::Skipped]);

        let resolved = resolve_env(&decls, &prompter).unwrap();

        assert!(resolved.vars.is_empty());
    }

    #[test]
    fn required_var_cannot_be_skipped() {
        let mut decls = BTreeMap::new();
        decls.insert("BRANCH".to_string(), interactive_text("Branch:"));
        let prompter = MockPrompter::new(vec![PromptResult::Skipped]);

        let error = match resolve_env(&decls, &prompter) {
            Ok(_) => panic!("required skipped var should fail"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("BRANCH"));
        assert!(error.to_string().contains("skip"));
    }

    #[test]
    fn prompt_errors_are_propagated() {
        let mut decls = BTreeMap::new();
        decls.insert("BRANCH".to_string(), interactive_text("Branch:"));

        let error = match resolve_env(&decls, &ErrorPrompter) {
            Ok(_) => panic!("prompt I/O failures should bubble up"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("prompt I/O failed"));
    }

    #[test]
    fn skip_cascades_to_dependents() {
        let mut decls = BTreeMap::new();
        let mut project = interactive_select("Pick:", vec!["a", "b"]);
        project.skippable = true;
        decls.insert("PROJECT".to_string(), project);

        let mut branch = interactive_text("Branch:");
        branch.depends_on = vec!["env.PROJECT".to_string()];
        decls.insert("BRANCH".to_string(), branch);

        let prompter = MockPrompter::new(vec![PromptResult::Skipped]);

        let resolved = resolve_env(&decls, &prompter).unwrap();

        assert!(resolved.vars.is_empty());
    }

    #[test]
    fn skip_cascades_through_chain() {
        let mut decls = BTreeMap::new();

        let mut a = interactive_text("A:");
        a.skippable = true;
        decls.insert("A".to_string(), a);

        let mut b = interactive_text("B:");
        b.depends_on = vec!["env.A".to_string()];
        decls.insert("B".to_string(), b);

        let mut c = interactive_text("C:");
        c.depends_on = vec!["env.B".to_string()];
        decls.insert("C".to_string(), c);

        let prompter = MockPrompter::new(vec![PromptResult::Skipped]);

        let resolved = resolve_env(&decls, &prompter).unwrap();

        assert!(resolved.vars.is_empty());
    }

    #[test]
    fn dependency_order_is_respected() {
        let mut decls = BTreeMap::new();

        let mut branch = interactive_text("Branch:");
        branch.depends_on = vec!["env.PROJECT".to_string()];
        decls.insert("BRANCH".to_string(), branch);

        decls.insert(
            "PROJECT".to_string(),
            interactive_select("Pick:", vec!["a", "b"]),
        );

        let prompter = MockPrompter::new(vec![
            PromptResult::Value("a".to_string()),
            PromptResult::Value("main".to_string()),
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
            "PROJECT".to_string(),
            interactive_select("Select a project:", vec!["alpha", "beta"]),
        );

        let mut branch = interactive_text("Branch for ${env.PROJECT}:");
        branch.depends_on = vec!["env.PROJECT".to_string()];
        decls.insert("BRANCH".to_string(), branch);

        let prompter = MockPrompter::new(vec![
            PromptResult::Value("alpha".to_string()),
            PromptResult::Value("main".to_string()),
        ]);

        resolve_env(&decls, &prompter).unwrap();

        let titles = prompter.captured_titles.borrow();
        assert_eq!(titles[1], "Branch for alpha:");
    }

    #[test]
    fn interpolates_default_value_with_resolved_value() {
        let mut decls = BTreeMap::new();
        decls.insert(
            "PROJECT".to_string(),
            interactive_select("Select:", vec!["proj1", "proj2"]),
        );

        let branch = EnvVarDecl {
            default_value: Some("feature/${env.PROJECT}".to_string()),
            interactive: true,
            skippable: false,
            prompt: Some("Branch:".to_string()),
            options: vec![],
            depends_on: vec!["env.PROJECT".to_string()],
        };
        decls.insert("BRANCH".to_string(), branch);

        let prompter = MockPrompter::new(vec![
            PromptResult::Value("proj1".to_string()),
            PromptResult::Value("feature/proj1".to_string()),
        ]);

        resolve_env(&decls, &prompter).unwrap();

        let defaults = prompter.captured_defaults.borrow();
        assert_eq!(defaults[1], Some("feature/proj1".to_string()));
    }

    #[test]
    fn interpolates_static_default_value() {
        let mut decls = BTreeMap::new();
        decls.insert(
            "PROJECT".to_string(),
            interactive_select("Select:", vec!["proj1", "proj2"]),
        );

        let derived = EnvVarDecl {
            default_value: Some("${env.PROJECT}-derived".to_string()),
            interactive: false,
            skippable: false,
            prompt: None,
            options: vec![],
            depends_on: vec!["env.PROJECT".to_string()],
        };
        decls.insert("DERIVED".to_string(), derived);

        let prompter = MockPrompter::new(vec![PromptResult::Value("proj1".to_string())]);

        let resolved = resolve_env(&decls, &prompter).unwrap();

        assert_eq!(
            resolved.vars,
            vec![
                ("PROJECT".to_string(), "proj1".to_string()),
                ("DERIVED".to_string(), "proj1-derived".to_string()),
            ]
        );
    }

    #[test]
    fn interpolates_multiple_refs_in_one_field() {
        let mut decls = BTreeMap::new();
        decls.insert("TEAM".to_string(), static_var("backend"));
        decls.insert(
            "PROJECT".to_string(),
            interactive_select("Select:", vec!["api", "web"]),
        );

        let label = EnvVarDecl {
            default_value: Some("${env.TEAM}/${env.PROJECT}".to_string()),
            interactive: true,
            skippable: false,
            prompt: Some("Label for ${env.TEAM}/${env.PROJECT}:".to_string()),
            options: vec![],
            depends_on: vec!["env.TEAM".to_string(), "env.PROJECT".to_string()],
        };
        decls.insert("LABEL".to_string(), label);

        let prompter = MockPrompter::new(vec![
            PromptResult::Value("api".to_string()),
            PromptResult::Value("backend/api".to_string()),
        ]);

        resolve_env(&decls, &prompter).unwrap();

        let titles = prompter.captured_titles.borrow();
        let defaults = prompter.captured_defaults.borrow();
        // LABEL is the second prompt (PROJECT is first, TEAM is static)
        assert_eq!(titles[1], "Label for backend/api:");
        assert_eq!(defaults[1], Some("backend/api".to_string()));
    }

    #[test]
    fn resolved_values_containing_dollar_brace_are_not_re_interpolated() {
        let mut decls = BTreeMap::new();

        // User types a value that looks like an interpolation placeholder
        decls.insert("A".to_string(), interactive_text("Enter A:"));
        decls.insert("B".to_string(), static_var("secret"));

        let c = EnvVarDecl {
            default_value: Some("prefix-${env.A}-suffix".to_string()),
            interactive: false,
            skippable: false,
            prompt: None,
            options: vec![],
            depends_on: vec!["env.A".to_string()],
        };
        decls.insert("C".to_string(), c);

        // User enters a value that looks like an interpolation ref
        let prompter = MockPrompter::new(vec![PromptResult::Value("${env.B}".to_string())]);

        let resolved = resolve_env(&decls, &prompter).unwrap();

        // C should be "prefix-${env.B}-suffix" (literal), NOT "prefix-secret-suffix"
        let c_value = resolved.vars.iter().find(|(k, _)| k == "C").unwrap();
        assert_eq!(c_value.1, "prefix-${env.B}-suffix");
    }

    #[test]
    fn no_interpolation_without_placeholders() {
        let mut decls = BTreeMap::new();
        decls.insert("BRANCH".to_string(), interactive_text("Branch name:"));

        let prompter = MockPrompter::new(vec![PromptResult::Value("main".to_string())]);

        resolve_env(&decls, &prompter).unwrap();

        let titles = prompter.captured_titles.borrow();
        assert_eq!(titles[0], "Branch name:");
    }
}
