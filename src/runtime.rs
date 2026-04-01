use crate::config::AppConfig;
use crate::docker::CommandRunner;
use crate::instance::{next_container_name, AgentState};
use crate::manifest::AgentManifest;
use crate::paths::JackinPaths;
use crate::repo::{validate_agent_repo, CachedRepo};
use crate::selector::ClassSelector;

pub fn load_agent(
    paths: &JackinPaths,
    config: &mut AppConfig,
    selector: &ClassSelector,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let source = config.resolve_or_register(selector, paths)?;

    let cached_repo = CachedRepo::new(paths, selector);
    std::fs::create_dir_all(cached_repo.repo_dir.parent().unwrap())?;

    if cached_repo.repo_dir.exists() {
        runner.run(
            "git",
            &[
                "-C".into(),
                cached_repo.repo_dir.display().to_string(),
                "pull".into(),
                "--ff-only".into(),
            ],
            None,
        )?;
    } else {
        runner.run(
            "git",
            &[
                "clone".into(),
                source.git.clone(),
                cached_repo.repo_dir.display().to_string(),
            ],
            None,
        )?;
    }

    let validated_repo = validate_agent_repo(&cached_repo.repo_dir)?;
    let manifest = &validated_repo.manifest;

    let existing = list_running_agent_names(runner)?;
    let container_name = next_container_name(selector, &existing);
    let state = AgentState::prepare(paths, &container_name, manifest)?;

    let image = image_name(selector);
    let network = format!("jackin-{container_name}-net");
    let dind = format!("{container_name}-dind");

    let mut cleanup = LoadCleanup::new(container_name.clone(), dind.clone(), network.clone());
    let load_result = (|| -> anyhow::Result<()> {
        runner.run(
            "docker",
            &["network".into(), "create".into(), network.clone()],
            None,
        )?;

        runner.run(
            "docker",
            &[
                "run".into(),
                "-d".into(),
                "--name".into(),
                dind.clone(),
                "--network".into(),
                network.clone(),
                "--privileged".into(),
                "docker:dind".into(),
            ],
            None,
        )?;

        wait_for_dind(&dind, runner)?;

        runner.run(
            "docker",
            &[
                "build".into(),
                "-t".into(),
                image.clone(),
                "-f".into(),
                validated_repo.dockerfile_path.display().to_string(),
                cached_repo.repo_dir.display().to_string(),
            ],
            None,
        )?;

        runner.run(
            "docker",
            &[
                "run".into(),
                "-d".into(),
                "--name".into(),
                container_name.clone(),
                "--hostname".into(),
                container_name.clone(),
                "--network".into(),
                network.clone(),
                "--label".into(),
                "jackin.managed=true".into(),
                "--label".into(),
                format!("jackin.class={}", selector.key()),
                "-e".into(),
                format!("DOCKER_HOST=tcp://{dind}:2375"),
                "-v".into(),
                format!("{}:/workspace", cached_repo.repo_dir.display()),
                "-v".into(),
                format!("{}:/home/claude/.claude", state.claude_dir.display()),
                "-v".into(),
                format!("{}:/home/claude/.claude.json", state.claude_json.display()),
                image,
                "tail".into(),
                "-f".into(),
                "/dev/null".into(),
            ],
            None,
        )?;

        bootstrap_plugins(&container_name, manifest, runner)?;

        runner.run(
            "docker",
            &[
                "exec".into(),
                "-it".into(),
                container_name.clone(),
                "env".into(),
                "CLAUDE_ENV=docker".into(),
                "claude".into(),
                "--dangerously-skip-permissions".into(),
                "--verbose".into(),
            ],
            None,
        )?;

        Ok(())
    })();

    if let Err(error) = load_result {
        cleanup.run(runner);
        return Err(error);
    }

    cleanup.disarm();
    Ok(())
}

pub fn hardline_agent(
    container_name: &str,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    runner.run(
        "docker",
        &[
            "exec".into(),
            "-it".into(),
            container_name.to_string(),
            "sh".into(),
        ],
        None,
    )
}

fn wait_for_dind(dind_name: &str, runner: &mut impl CommandRunner) -> anyhow::Result<()> {
    for _ in 0..30 {
        if runner
            .run(
                "docker",
                &[
                    "exec".into(),
                    dind_name.to_string(),
                    "docker".into(),
                    "info".into(),
                ],
                None,
            )
            .is_ok()
        {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }

    anyhow::bail!("timed out waiting for Docker-in-Docker sidecar {dind_name}")
}

fn bootstrap_plugins(
    container_name: &str,
    manifest: &AgentManifest,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let _ = runner.run(
        "docker",
        &[
            "exec".into(),
            container_name.to_string(),
            "claude".into(),
            "plugin".into(),
            "marketplace".into(),
            "add".into(),
            "anthropics/claude-plugins-official".into(),
        ],
        None,
    );

    for plugin in &manifest.claude.plugins {
        runner.run(
            "docker",
            &[
                "exec".into(),
                container_name.to_string(),
                "claude".into(),
                "plugin".into(),
                "install".into(),
                plugin.clone(),
            ],
            None,
        )?;
    }

    Ok(())
}

pub fn list_running_agent_names(
    runner: &mut impl CommandRunner,
) -> anyhow::Result<Vec<String>> {
    let output = runner.capture(
        "docker",
        &[
            "ps".into(),
            "--filter".into(),
            "label=jackin.managed=true".into(),
            "--format".into(),
            "{{.Names}}".into(),
        ],
        None,
    )?;

    Ok(output
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect())
}

pub fn matching_family(selector: &ClassSelector, names: &[String]) -> Vec<String> {
    names
        .iter()
        .filter(|name| crate::instance::class_family_matches(selector, name))
        .cloned()
        .collect()
}

pub fn purge_class_data(paths: &JackinPaths, selector: &ClassSelector) -> anyhow::Result<()> {
    if !paths.data_dir.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(&paths.data_dir)? {
        let entry = entry?;
        let file_name = entry.file_name().to_string_lossy().to_string();
        if crate::instance::class_family_matches(selector, &file_name) {
            std::fs::remove_dir_all(entry.path())?;
        }
    }

    Ok(())
}

pub fn eject_agent(
    container_name: &str,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let dind = format!("{container_name}-dind");
    let network = format!("jackin-{container_name}-net");

    runner.run(
        "docker",
        &["rm".into(), "-f".into(), container_name.to_string()],
        None,
    )?;
    runner.run("docker", &["rm".into(), "-f".into(), dind], None)?;
    runner.run("docker", &["network".into(), "rm".into(), network], None)?;

    Ok(())
}

pub fn exile_all(runner: &mut impl CommandRunner) -> anyhow::Result<()> {
    let names = list_running_agent_names(runner)?;
    for name in names {
        eject_agent(&name, runner)?;
    }
    Ok(())
}

fn image_name(selector: &ClassSelector) -> String {
    format!("jackin-{}", selector.key().replace('/', "-"))
}

struct LoadCleanup {
    container_name: String,
    dind: String,
    network: String,
    armed: bool,
}

impl LoadCleanup {
    fn new(container_name: String, dind: String, network: String) -> Self {
        Self {
            container_name,
            dind,
            network,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }

    fn run(&self, runner: &mut impl CommandRunner) {
        if !self.armed {
            return;
        }

        let _ = runner.run(
            "docker",
            &["rm".into(), "-f".into(), self.container_name.clone()],
            None,
        );
        let _ = runner.run(
            "docker",
            &["rm".into(), "-f".into(), self.dind.clone()],
            None,
        );
        let _ = runner.run(
            "docker",
            &["network".into(), "rm".into(), self.network.clone()],
            None,
        );
    }
}

#[derive(Default)]
pub struct FakeRunner {
    pub recorded: Vec<String>,
    pub fail_on: Vec<String>,
}

impl CommandRunner for FakeRunner {
    fn run(
        &mut self,
        program: &str,
        args: &[String],
        _cwd: Option<&std::path::Path>,
    ) -> anyhow::Result<()> {
        let command = format!("{} {}", program, args.join(" "));
        self.recorded.push(command.clone());
        if self.fail_on.iter().any(|pattern| command.contains(pattern)) {
            anyhow::bail!("command failed: {command}");
        }
        Ok(())
    }

    fn capture(
        &mut self,
        program: &str,
        args: &[String],
        _cwd: Option<&std::path::Path>,
    ) -> anyhow::Result<String> {
        let command = format!("{} {}", program, args.join(" "));
        self.recorded.push(command.clone());
        if self.fail_on.iter().any(|pattern| command.contains(pattern)) {
            anyhow::bail!("command failed: {command}");
        }
        Ok(String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use crate::paths::JackinPaths;
    use crate::selector::ClassSelector;
    use tempfile::tempdir;

    #[test]
    fn load_owner_repo_registers_source_and_builds_commands() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = ClassSelector::new(Some("chainargos"), "smith");
        let mut runner = FakeRunner::default();

        let repo_dir = paths.agents_dir.join("chainargos").join("smith");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(repo_dir.join("Dockerfile"), "FROM debian:trixie\n").unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = [\"code-review@claude-plugins-official\"]\n",
        )
        .unwrap();

        load_agent(&paths, &mut config, &selector, &mut runner).unwrap();

        assert!(std::fs::read_to_string(&paths.config_file)
            .unwrap()
            .contains("chainargos/smith"));
        assert!(runner
            .recorded
            .iter()
            .any(|call| call.contains("git -C") || call.contains("git clone")));
        assert!(runner
            .recorded
            .iter()
            .any(|call| call.contains("docker build")));
        assert!(runner.recorded.iter().any(|call| call
            .contains("docker exec -it agent-chainargos-smith env CLAUDE_ENV=docker claude --dangerously-skip-permissions --verbose")));
    }

    #[test]
    fn eject_all_targets_only_requested_class_family() {
        let selector = ClassSelector::new(None, "smith");
        let names = vec![
            "agent-smith".to_string(),
            "agent-smith-clone-1".to_string(),
            "agent-chainargos-smith".to_string(),
        ];

        let matched = matching_family(&selector, &names);

        assert_eq!(matched, vec!["agent-smith", "agent-smith-clone-1"]);
    }

    #[test]
    fn purge_all_removes_matching_state_directories() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        std::fs::create_dir_all(paths.data_dir.join("agent-smith")).unwrap();
        std::fs::create_dir_all(paths.data_dir.join("agent-smith-clone-1")).unwrap();
        std::fs::create_dir_all(paths.data_dir.join("agent-chainargos-smith")).unwrap();
        let selector = ClassSelector::new(None, "smith");

        purge_class_data(&paths, &selector).unwrap();

        assert!(!paths.data_dir.join("agent-smith").exists());
        assert!(!paths.data_dir.join("agent-smith-clone-1").exists());
        assert!(paths.data_dir.join("agent-chainargos-smith").exists());
    }

    #[test]
    fn eject_agent_removes_container_dind_and_network() {
        let mut runner = FakeRunner::default();

        eject_agent("agent-smith", &mut runner).unwrap();

        assert_eq!(runner.recorded, vec![
            "docker rm -f agent-smith",
            "docker rm -f agent-smith-dind",
            "docker network rm jackin-agent-smith-net",
        ]);
    }

    #[test]
    fn exile_all_ejects_all_running_agents() {
        let mut runner = FakeRunner::default();

        exile_all(&mut runner).unwrap();

        assert_eq!(
            runner.recorded,
            vec!["docker ps --filter label=jackin.managed=true --format {{.Names}}"]
        );
    }

    #[test]
    fn hardline_uses_docker_exec_shell() {
        let mut runner = FakeRunner::default();

        hardline_agent("agent-smith", &mut runner).unwrap();

        assert_eq!(runner.recorded.last().unwrap(), "docker exec -it agent-smith sh");
    }

    #[test]
    fn load_agent_rolls_back_runtime_on_bootstrap_failure() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let mut config = AppConfig::load_or_init(&paths).unwrap();
        let selector = ClassSelector::new(None, "smith");
        let mut runner = FakeRunner {
            fail_on: vec!["docker exec agent-smith claude plugin install".to_string()],
            ..Default::default()
        };

        let repo_dir = paths.agents_dir.join("smith");
        std::fs::create_dir_all(&repo_dir).unwrap();
        std::fs::write(repo_dir.join("Dockerfile"), "FROM debian:trixie\n").unwrap();
        std::fs::write(
            repo_dir.join("jackin.agent.toml"),
            "dockerfile = \"Dockerfile\"\n\n[claude]\nplugins = [\"code-review@claude-plugins-official\"]\n",
        )
        .unwrap();

        let error = load_agent(&paths, &mut config, &selector, &mut runner).unwrap_err();

        assert!(error.to_string().contains("docker exec agent-smith claude plugin install"));
        assert!(runner.recorded.iter().any(|call| call == "docker rm -f agent-smith"));
        assert!(runner.recorded.iter().any(|call| call == "docker rm -f agent-smith-dind"));
        assert!(runner
            .recorded
            .iter()
            .any(|call| call == "docker network rm jackin-agent-smith-net"));
    }
}
