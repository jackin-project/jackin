use crate::manifest::AgentManifest;
use crate::paths::JackinPaths;
use crate::selector::ClassSelector;
use std::path::{Component, Path, PathBuf};

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

#[derive(Debug, Clone)]
pub struct ValidatedAgentRepo {
    pub manifest: AgentManifest,
    pub dockerfile_path: PathBuf,
}

pub fn validate_agent_repo(repo_dir: &Path) -> anyhow::Result<ValidatedAgentRepo> {
    let manifest = repo_dir.join("jackin.agent.toml");

    if !manifest.is_file() {
        anyhow::bail!("invalid agent repo: missing {}", manifest.display());
    }

    let manifest = AgentManifest::load(repo_dir)?;
    let dockerfile_path = resolve_manifest_dockerfile_path(repo_dir, &manifest)?;

    Ok(ValidatedAgentRepo {
        manifest,
        dockerfile_path,
    })
}

fn resolve_manifest_dockerfile_path(repo_dir: &Path, manifest: &AgentManifest) -> anyhow::Result<PathBuf> {
    let dockerfile = Path::new(&manifest.dockerfile);

    if dockerfile.is_absolute() {
        anyhow::bail!("invalid agent repo: dockerfile path must be relative");
    }

    for component in dockerfile.components() {
        if matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_)) {
            anyhow::bail!("invalid agent repo: dockerfile path must stay inside the repo");
        }
    }

    let resolved = repo_dir.join(dockerfile);
    if !resolved.is_file() {
        anyhow::bail!("invalid agent repo: missing {}", resolved.display());
    }

    Ok(resolved)
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

    #[test]
    fn rejects_missing_manifest_dockerfile() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            "dockerfile = \"docker/agent.Dockerfile\"\n\n[claude]\nplugins = []\n",
        )
        .unwrap();

        let error = validate_agent_repo(temp.path()).unwrap_err();

        assert!(error.to_string().contains("docker/agent.Dockerfile"));
    }

    #[test]
    fn rejects_manifest_dockerfile_outside_repo() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            "dockerfile = \"../Dockerfile\"\n\n[claude]\nplugins = []\n",
        )
        .unwrap();

        let error = validate_agent_repo(temp.path()).unwrap_err();

        assert!(error.to_string().contains("must stay inside the repo"));
    }

    #[test]
    fn accepts_manifest_dockerfile_in_subdirectory() {
        let temp = tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("docker")).unwrap();
        std::fs::write(
            temp.path().join("docker/agent.Dockerfile"),
            "FROM debian:trixie\n",
        )
        .unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            "dockerfile = \"docker/agent.Dockerfile\"\n\n[claude]\nplugins = []\n",
        )
        .unwrap();

        let validated = validate_agent_repo(temp.path()).unwrap();

        assert_eq!(validated.dockerfile_path, temp.path().join("docker/agent.Dockerfile"));
    }
}
