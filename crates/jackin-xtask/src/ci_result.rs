use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;

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
    }
}

fn find(args: FindArgs) -> Result<()> {
    let names = result_names(&args);
    let artifact = if args.cache_key.is_empty() {
        None
    } else {
        lookup_artifact(&args.repository, &names.name, &args.package)
    }
    .or_else(|| lookup_artifact(&args.repository, &names.sha_name, &args.package));
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

fn lookup_artifact(repository: &str, name: &str, package: &str) -> Option<Artifact> {
    let endpoint = format!("repos/{repository}/actions/artifacts?name={name}&per_page=10");
    let response = cmd::output(Command::new("gh").args(["api", &endpoint])).and_then(|output| {
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
            return None;
        }
    };
    artifacts.retain(|artifact| !artifact.expired);
    artifacts.sort_unstable_by(|left, right| right.created_at.cmp(&left.created_at));
    artifacts.into_iter().next()
}

fn write_output(name: &str, value: &str) -> Result<()> {
    let output = env::var_os("GITHUB_OUTPUT").context("GITHUB_OUTPUT must be set")?;
    let path = Path::new(&output);
    let mut contents = fs::read(path).unwrap_or_default();
    writeln!(contents, "{name}={value}").context("formatting GitHub Actions output")?;
    fs::write(path, contents).with_context(|| format!("writing {}", path.display()))
}
