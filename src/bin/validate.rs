use std::path::PathBuf;
use std::process::ExitCode;

use jackin::repo::validate_role_repo;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let (migrate, print_construct_version, repo_arg) = match parse_args(&args[1..]) {
        Ok(parsed) => parsed,
        Err(err) => {
            eprintln!("error: {err}");
            eprintln!("Usage: jackin-validate [--migrate] [--print-construct-version] <role-repo-path>");
            return ExitCode::FAILURE;
        }
    };

    let repo_dir = PathBuf::from(repo_arg);
    match std::fs::metadata(&repo_dir) {
        Ok(m) if m.is_dir() => {}
        Ok(_) => {
            eprintln!("error: {} is not a directory", repo_dir.display());
            return ExitCode::FAILURE;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("error: {} does not exist", repo_dir.display());
            return ExitCode::FAILURE;
        }
        Err(e) => {
            eprintln!("error: cannot inspect {}: {e}", repo_dir.display());
            return ExitCode::FAILURE;
        }
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
        Ok(validated) => {
            if print_construct_version {
                println!("{}", validated.dockerfile.construct_version);
            } else {
                println!("All checks passed");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

// Accept flags in any position so `jackin-validate <path> --migrate` works as
// well as `jackin-validate --migrate <path>`. Errors return plain messages;
// main prepends a single `error:` prefix at print time.
fn parse_args(args: &[String]) -> Result<(bool, bool, &str), String> {
    let mut migrate = false;
    let mut print_construct_version = false;
    let mut repo: Option<&str> = None;
    for arg in args {
        match arg.as_str() {
            "--migrate" => {
                if migrate {
                    return Err("--migrate specified twice".into());
                }
                migrate = true;
            }
            "--print-construct-version" => {
                if print_construct_version {
                    return Err("--print-construct-version specified twice".into());
                }
                print_construct_version = true;
            }
            _ if arg.starts_with("--") => return Err(format!("unknown flag {arg}")),
            _ if repo.is_some() => return Err("too many positional arguments".into()),
            _ => repo = Some(arg),
        }
    }
    let repo = repo.ok_or_else(|| "missing role-repo-path".to_string())?;
    Ok((migrate, print_construct_version, repo))
}
