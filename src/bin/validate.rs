use std::path::PathBuf;
use std::process::ExitCode;

use jackin::repo::validate_role_repo;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: jackin-validate <role-repo-path>");
        return ExitCode::FAILURE;
    }

    let repo_dir = PathBuf::from(&args[1]);
    if !repo_dir.is_dir() {
        eprintln!("Error: {} is not a directory", repo_dir.display());
        return ExitCode::FAILURE;
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
