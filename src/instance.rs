use crate::paths::JackinPaths;
use crate::selector::ClassSelector;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AgentState {
    pub root: PathBuf,
    pub claude_dir: PathBuf,
    pub claude_json: PathBuf,
}

impl AgentState {
    pub fn prepare(paths: &JackinPaths, container_name: &str) -> anyhow::Result<Self> {
        let root = paths.data_dir.join(container_name);
        let claude_dir = root.join(".claude");
        let claude_json = root.join(".claude.json");

        std::fs::create_dir_all(&claude_dir)?;
        if !claude_json.exists() {
            std::fs::write(&claude_json, "{}")?;
        }

        Ok(Self {
            root,
            claude_dir,
            claude_json,
        })
    }
}

pub fn primary_container_name(selector: &ClassSelector) -> String {
    match &selector.namespace {
        Some(namespace) => format!("agent-{namespace}-{}", selector.name),
        None => format!("agent-{}", selector.name),
    }
}

pub fn next_container_name(selector: &ClassSelector, existing: &[String]) -> String {
    let primary = primary_container_name(selector);
    if !existing.iter().any(|name| name == &primary) {
        return primary;
    }

    let mut clone_index = 1;
    loop {
        let candidate = format!("{primary}-clone-{clone_index}");
        if !existing.iter().any(|name| name == &candidate) {
            return candidate;
        }
        clone_index += 1;
    }
}

pub fn class_family_matches(selector: &ClassSelector, container_name: &str) -> bool {
    let primary = primary_container_name(selector);
    container_name == primary || container_name.starts_with(&format!("{primary}-clone-"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::JackinPaths;
    use crate::selector::ClassSelector;
    use tempfile::tempdir;

    #[test]
    fn picks_next_clone_name() {
        let selector = ClassSelector::new(None, "smith");
        let existing = vec![
            "agent-smith".to_string(),
            "agent-smith-clone-1".to_string(),
        ];

        let name = next_container_name(&selector, &existing);

        assert_eq!(name, "agent-smith-clone-2");
    }

    #[test]
    fn prepares_persisted_claude_state() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());

        let state = AgentState::prepare(&paths, "agent-smith").unwrap();

        assert!(state.claude_dir.is_dir());
        assert_eq!(std::fs::read_to_string(&state.claude_json).unwrap(), "{}");
    }
}
