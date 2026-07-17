use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Args, Subcommand, ValueEnum};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::cmd;

#[cfg(test)]
mod tests;

#[derive(Subcommand, Debug)]
pub(crate) enum CiResultCommand {
    /// Find an input-identical successful crate result.
    Find(FindArgs),
    /// Stage the runner-independent successful-result marker.
    Stage(StageArgs),
}

#[derive(Args, Debug)]
pub(crate) struct StageArgs {
    #[arg(long)]
    package: String,
    #[arg(long)]
    source_key: String,
    #[arg(long)]
    source_sha: String,
    #[arg(long, default_value = "crate-result.txt")]
    output: PathBuf,
}

#[derive(Args, Debug)]
pub(crate) struct FindArgs {
    #[arg(long)]
    package: String,
    #[arg(long, default_value = "")]
    cache_key: String,
    #[arg(long)]
    all_features: Toggle,
    #[arg(long)]
    docker_e2e: Toggle,
    #[arg(long)]
    construct_image_changed: Toggle,
    #[arg(long)]
    common_contract_key: String,
    #[arg(long)]
    docker_contract_key: String,
    #[arg(long)]
    runner_os: String,
    #[arg(long)]
    runner_arch: String,
    #[arg(long)]
    source_sha: String,
    #[arg(long)]
    repository: String,
    /// Current workflow run whose newly published artifacts should be checked.
    #[arg(long, default_value_t = 0)]
    run_id: u64,
    #[arg(long, default_value = "")]
    refresh_package: String,
    #[arg(long)]
    github_output: bool,
}

#[derive(Deserialize)]
struct ArtifactsResponse {
    artifacts: Vec<Artifact>,
}

#[derive(Deserialize)]
struct Artifact {
    id: u64,
    expired: bool,
    created_at: String,
}

struct ResultNames {
    name: String,
    sha_name: String,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Toggle {
    True,
    False,
}

impl Toggle {
    const fn enabled(self) -> bool {
        matches!(self, Self::True)
    }
}

impl std::fmt::Display for Toggle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::True => "true",
            Self::False => "false",
        })
    }
}

pub(crate) fn run(command: CiResultCommand) -> Result<()> {
    match command {
        CiResultCommand::Find(args) => find(args),
        CiResultCommand::Stage(args) => stage(args),
    }
}

fn stage(args: StageArgs) -> Result<()> {
    let marker = format!(
        "package={}\nsource-key={}\nsource-sha={}\n",
        args.package, args.source_key, args.source_sha
    );
    fs::write(&args.output, marker)
        .with_context(|| format!("writing successful crate result {}", args.output.display()))
}

fn find(args: FindArgs) -> Result<()> {
    let names = result_names(&args);
    let artifact = if args.cache_key.is_empty() {
        Lookup::Missing
    } else {
        lookup_artifact(&args.repository, args.run_id, &names.name, &args.package)
    };
    let artifact = match artifact {
        Lookup::Missing => lookup_artifact(
            &args.repository,
            args.run_id,
            &names.sha_name,
            &args.package,
        ),
        result => result,
    };
    let artifact = match artifact {
        Lookup::Found(artifact) => Some(artifact),
        Lookup::Missing | Lookup::Unavailable => None,
    };
    let artifact_id = artifact.map(|artifact| artifact.id);
    let hit = artifact_id.is_some() && args.package != args.refresh_package;
    if args.github_output {
        write_output("name", &names.name)?;
        write_output("sha-name", &names.sha_name)?;
        return write_output("hit", if hit { "true" } else { "false" });
    }
    let result = serde_json::json!({
        "name": names.name,
        "sha_name": names.sha_name,
        "artifact_id": artifact_id,
        "hit": hit,
    });
    writeln!(io::stdout().lock(), "{result}").context("writing crate result JSON")
}

fn result_names(args: &FindArgs) -> ResultNames {
    let common_contract_key = if args.common_contract_key.is_empty() {
        "2c87e22194a8df9228603249e7d4efdee1df4ab829909d9350b0194d8b0ab83b"
    } else {
        &args.common_contract_key
    };
    let (docker_e2e, construct_image_changed, contract_key) = if args.package == "jackin" {
        let mut digest = Sha256::new();
        digest.update(format!(
            "{}:{}",
            common_contract_key, args.docker_contract_key
        ));
        (
            args.docker_e2e.enabled(),
            args.construct_image_changed.enabled(),
            hex::encode(digest.finalize()),
        )
    } else {
        (false, false, common_contract_key.to_owned())
    };
    let suffix = format!(
        "af{}-e2e{}-construct{}",
        args.all_features, docker_e2e, construct_image_changed
    );
    let sha_name = format!(
        "ci-crate-result-sha-v1-{}-{}-{}-{}-{}-{suffix}",
        args.runner_os, args.runner_arch, args.package, args.source_sha, contract_key
    );
    let name = if args.cache_key.is_empty() {
        sha_name.clone()
    } else {
        format!(
            "ci-crate-result-v1-{}-{}-{}-{}-{}-{suffix}",
            args.runner_os, args.runner_arch, args.package, args.cache_key, contract_key
        )
    };
    ResultNames { name, sha_name }
}

enum Lookup {
    Found(Artifact),
    Missing,
    Unavailable,
}

fn lookup_artifact(repository: &str, run_id: u64, name: &str, package: &str) -> Lookup {
    let endpoint = format!("repos/{repository}/actions/artifacts?name={name}&per_page=10");
    let global = lookup_endpoint(&endpoint, package);
    if !matches!(global, Lookup::Missing | Lookup::Unavailable) || run_id == 0 {
        return global;
    }
    let endpoint =
        format!("repos/{repository}/actions/runs/{run_id}/artifacts?name={name}&per_page=10");
    lookup_endpoint(&endpoint, package)
}

fn lookup_endpoint(endpoint: &str, package: &str) -> Lookup {
    let response = cmd::output_timeout(
        Command::new("gh").args(["api", endpoint]),
        Duration::from_secs(5),
    )
    .and_then(|output| {
        serde_json::from_slice::<ArtifactsResponse>(&output)
            .context("parsing GitHub artifact response")
    });
    let mut artifacts = match response {
        Ok(response) => response.artifacts,
        Err(error) => {
            let _write_result = writeln!(
                io::stderr().lock(),
                "::warning::successful-result lookup failed for {package}; scheduling crate: {error:#}"
            );
            return Lookup::Unavailable;
        }
    };
    artifacts.retain(|artifact| !artifact.expired);
    artifacts.sort_unstable_by(|left, right| right.created_at.cmp(&left.created_at));
    artifacts
        .into_iter()
        .next()
        .map_or(Lookup::Missing, Lookup::Found)
}

fn write_output(name: &str, value: &str) -> Result<()> {
    let output = env::var_os("GITHUB_OUTPUT").context("GITHUB_OUTPUT must be set")?;
    let path = Path::new(&output);
    let mut contents = fs::read(path).unwrap_or_default();
    writeln!(contents, "{name}={value}").context("formatting GitHub Actions output")?;
    fs::write(path, contents).with_context(|| format!("writing {}", path.display()))
}
