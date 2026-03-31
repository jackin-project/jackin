use crate::manifest::AgentManifest;
use crate::paths::JackinPaths;
use crate::selector::ClassSelector;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct CachedRepo {
    pub key: String,
    pub repo_dir: PathBuf,
}

impl CachedRepo {
    pub fn new(paths: &JackinPaths, selector: &ClassSelector) -> Self {
        let repo_dir = match &selector.namespace {
            Some(namespace) => paths.agents_dir.join(namespace).join(&selector.name),
            None => paths.agents_dir.join(&selector.name),
        };

        Self {
            key: selector.key(),
            repo_dir,
        }
    }
}

pub fn validate_agent_repo(repo_dir: &Path) -> anyhow::Result<AgentManifest> {
    let manifest = repo_dir.join("jackin.agent.toml");
    let dockerfile = repo_dir.join("Dockerfile");

    if !manifest.exists() {
        anyhow::bail!("invalid agent repo: missing {}", manifest.display());
    }
    if !dockerfile.exists() {
        anyhow::bail!("invalid agent repo: missing {}", dockerfile.display());
    }

    AgentManifest::load(repo_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::JackinPaths;
    use crate::selector::ClassSelector;
    use tempfile::tempdir;

    #[test]
    fn computes_cached_repo_path_for_namespaced_selector() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let selector = ClassSelector::new(Some("chainargos"), "smith");

        let repo = CachedRepo::new(&paths, &selector);

        assert_eq!(
            repo.repo_dir,
            paths.agents_dir.join("chainargos").join("smith")
        );
    }

    #[test]
    fn rejects_repo_without_required_files() {
        let temp = tempdir().unwrap();
        std::fs::create_dir_all(temp.path()).unwrap();

        let error = validate_agent_repo(temp.path()).unwrap_err();

        assert!(error.to_string().contains("jackin.agent.toml"));
    }
}
