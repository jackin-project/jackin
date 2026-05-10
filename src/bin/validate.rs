use std::path::PathBuf;
use std::process::ExitCode;

use jackin::repo::validate_role_repo;

// Hand-rolled argv handling instead of `clap`: the binary takes one optional
// flag and one positional, so a 30-line dispatcher is cheaper than the
// dependency. If a third option is added, switch to `clap`.
fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let (migrate, repo_arg) = match parse_args(&args[1..]) {
        Ok(parsed) => parsed,
        Err(err) => {
            eprintln!("{err}");
            eprintln!("Usage: jackin-validate [--migrate] <role-repo-path>");
            return ExitCode::FAILURE;
        }
    };

    let repo_dir = PathBuf::from(repo_arg);
    if !repo_dir.is_dir() {
        eprintln!("Error: {} is not a directory", repo_dir.display());
        return ExitCode::FAILURE;
    }

    if migrate {
        let manifest_path = repo_dir.join("jackin.role.toml");
        match jackin::manifest::migrations::migrate_manifest_file(&manifest_path) {
            Ok(Some((old, new))) => println!("Migrated manifest {old} -> {new}"),
            Ok(None) => println!("Manifest already at current version"),
            Err(error) => {
                eprintln!("error: {error:#}");
                return ExitCode::FAILURE;
            }
        }
    }

    match validate_role_repo(&repo_dir) {
        Ok(_) => {
            println!("All checks passed");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

// Accept `--migrate` in any position so `jackin-validate <path> --migrate`
// works as well as `jackin-validate --migrate <path>`.
fn parse_args(args: &[String]) -> Result<(bool, &str), String> {
    let mut migrate = false;
    let mut repo: Option<&str> = None;
    for arg in args {
        if arg == "--migrate" {
            if migrate {
                return Err("error: --migrate specified twice".into());
            }
            migrate = true;
        } else if arg.starts_with("--") {
            return Err(format!("error: unknown flag {arg}"));
        } else if repo.is_some() {
            return Err("error: too many positional arguments".into());
        } else {
            repo = Some(arg);
        }
    }
    let repo = repo.ok_or_else(|| "error: missing role-repo-path".to_string())?;
    Ok((migrate, repo))
}
