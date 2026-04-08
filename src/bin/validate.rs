use std::path::PathBuf;
use std::process::ExitCode;

use jackin::manifest::AgentManifest;
use jackin::repo_contract::validate_agent_dockerfile;

const REQUIRED_FILES: &[&str] = &[
    "Dockerfile",
    "jackin.agent.toml",
    ".dockerignore",
    ".gitignore",
];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: jackin-validate <agent-repo-path>");
        return ExitCode::FAILURE;
    }

    let repo_dir = PathBuf::from(&args[1]);
    if !repo_dir.is_dir() {
        eprintln!("Error: {} is not a directory", repo_dir.display());
        return ExitCode::FAILURE;
    }

    let mut errors: Vec<String> = Vec::new();

    // Check required files
    for file in REQUIRED_FILES {
        if !repo_dir.join(file).exists() {
            errors.push(format!("missing required file: {file}"));
        }
    }

    // Validate Dockerfile
    let dockerfile_path = repo_dir.join("Dockerfile");
    if dockerfile_path.exists()
        && let Err(e) = validate_agent_dockerfile(&dockerfile_path)
    {
        errors.push(format!("Dockerfile: {e}"));
    }

    // Validate manifest
    let manifest_path = repo_dir.join("jackin.agent.toml");
    if manifest_path.exists() {
        match AgentManifest::load(&repo_dir) {
            Ok(manifest) => match manifest.validate() {
                Ok(warnings) => {
                    for w in &warnings {
                        eprintln!("warning: {}", w.message);
                    }
                }
                Err(e) => errors.push(format!("manifest validation: {e}")),
            },
            Err(e) => errors.push(format!("manifest load: {e}")),
        }
    }

    if errors.is_empty() {
        println!("All checks passed");
        ExitCode::SUCCESS
    } else {
        for error in &errors {
            eprintln!("error: {error}");
        }
        ExitCode::FAILURE
    }
}
