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
    fn prompt_text(&self, title: &str, default: Option<&str>, skippable: bool) -> PromptResult;
    fn prompt_select(
        &self,
        title: &str,
        options: &[String],
        default: Option<&str>,
        skippable: bool,
    ) -> PromptResult;
}

pub fn resolve_env(
    declarations: &BTreeMap<String, EnvVarDecl>,
    prompter: &impl EnvPrompter,
) -> anyhow::Result<ResolvedEnv> {
    let order = topological_sort(declarations)?;
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

        if !decl.interactive {
            // Static var — use default
            if let Some(ref default) = decl.default_value {
                vars.push((name.clone(), default.clone()));
            }
            continue;
        }

        // Interactive var — prompt
        let title = decl.prompt.as_deref().unwrap_or(name.as_str());

        let result = if decl.options.is_empty() {
            prompter.prompt_text(title, decl.default_value.as_deref(), decl.skippable)
        } else {
            prompter.prompt_select(
                title,
                &decl.options,
                decl.default_value.as_deref(),
                decl.skippable,
            )
        };

        match result {
            PromptResult::Value(value) => {
                vars.push((name.clone(), value));
            }
            PromptResult::Skipped => {
                skipped.insert(name.clone());
            }
        }
    }

    Ok(ResolvedEnv { vars })
}

fn topological_sort(declarations: &BTreeMap<String, EnvVarDecl>) -> anyhow::Result<Vec<String>> {
    use std::collections::{HashMap, VecDeque};

    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();

    for name in declarations.keys() {
        in_degree.entry(name.as_str()).or_insert(0);
        adjacency.entry(name.as_str()).or_default();
    }

    for (name, decl) in declarations {
        for dep in &decl.depends_on {
            if let Some(dep_name) = dep.strip_prefix("env.") {
                adjacency.entry(dep_name).or_default().push(name.as_str());
                *in_degree.entry(name.as_str()).or_insert(0) += 1;
            }
        }
    }

    let mut queue: VecDeque<&str> = in_degree
        .iter()
        .filter(|&(_, &deg)| deg == 0)
        .map(|(&name, _)| name)
        .collect();

    let mut result = Vec::new();

    while let Some(node) = queue.pop_front() {
        result.push(node.to_string());
        if let Some(neighbors) = adjacency.get(node) {
            for &neighbor in neighbors {
                if let Some(deg) = in_degree.get_mut(neighbor) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(neighbor);
                    }
                }
            }
        }
    }

    if result.len() != declarations.len() {
        anyhow::bail!("env var dependency cycle detected");
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockPrompter {
        responses: std::cell::RefCell<Vec<PromptResult>>,
    }

    impl MockPrompter {
        fn new(responses: Vec<PromptResult>) -> Self {
            Self {
                responses: std::cell::RefCell::new(responses),
            }
        }
    }

    impl EnvPrompter for MockPrompter {
        fn prompt_text(
            &self,
            _title: &str,
            _default: Option<&str>,
            _skippable: bool,
        ) -> PromptResult {
            self.responses.borrow_mut().remove(0)
        }

        fn prompt_select(
            &self,
            _title: &str,
            _options: &[String],
            _default: Option<&str>,
            _skippable: bool,
        ) -> PromptResult {
            self.responses.borrow_mut().remove(0)
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
        decls.insert("CLAUDE_ENV".to_string(), static_var("docker"));
        let prompter = MockPrompter::new(vec![]);

        let resolved = resolve_env(&decls, &prompter).unwrap();

        assert_eq!(
            resolved.vars,
            vec![("CLAUDE_ENV".to_string(), "docker".to_string())]
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
}
