use std::path::PathBuf;
use std::process::ExitCode;

use jackin::repo::validate_role_repo;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 && !(args.len() == 3 && args[1] == "--migrate") {
        eprintln!("Usage: jackin-validate [--migrate] <role-repo-path>");
        return ExitCode::FAILURE;
    }

    let migrate = args.len() == 3;
    let repo_dir = PathBuf::from(if migrate { &args[2] } else { &args[1] });
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
                eprintln!("error: {error}");
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
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
