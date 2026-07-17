use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::Args;

use crate::cmd;

#[cfg(test)]
mod tests;

const TOOLS: &[&str] = &[
    "sccache",
    "cargo-nextest",
    "cargo-deny",
    "cargo-shear",
    "cargo-audit",
    "cargo-dylint",
    "cargo-fuzz",
    "cargo-hack",
    "cargo-hakari",
    "cargo-llvm-cov",
    "cargo-mutants",
    "cargo-zigbuild",
    "dylint-link",
];

#[derive(Args, Debug)]
pub(crate) struct CiStageArgs {
    #[arg(long)]
    xtask_hit: bool,
    #[arg(long)]
    tools_hit: bool,
    #[arg(long, default_value = "")]
    cached_xtask: PathBuf,
    #[arg(long, default_value = "")]
    cached_tools: PathBuf,
    #[arg(long, default_value = "target/debug/jackin-xtask")]
    built_xtask: PathBuf,
    #[arg(long, default_value = ".ci-prebuilt-tools")]
    tools_output: PathBuf,
    #[arg(long, default_value = ".ci-prebuilt-xtask")]
    xtask_output: PathBuf,
    #[arg(long, default_value = "target/ci-tools")]
    combined_output: PathBuf,
}

pub(crate) fn run(args: CiStageArgs) -> Result<()> {
    let tools_in_place = args.tools_hit && same_path(&args.cached_tools, &args.tools_output);
    let staged_xtask = args.xtask_output.join("jackin-xtask");
    let xtask_in_place = args.xtask_hit && same_path(&args.cached_xtask, &staged_xtask);
    if !tools_in_place {
        recreate_dir(&args.tools_output)?;
    }
    if !xtask_in_place {
        recreate_dir(&args.xtask_output)?;
    }
    recreate_dir(&args.combined_output)?;

    if args.tools_hit {
        if tools_in_place {
            validate_cached_tools(&args.tools_output)?;
        } else {
            copy_cached_tools(&args.cached_tools, &args.tools_output)?;
        }
    } else {
        stage_mise_tools(&args.tools_output)?;
    }

    if args.xtask_hit {
        if !xtask_in_place {
            copy_file(&args.cached_xtask, &staged_xtask)?;
        }
        let metadata = args
            .cached_xtask
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join("workspace-metadata.json");
        if !xtask_in_place && metadata.is_file() {
            copy_file(
                &metadata,
                &args.xtask_output.join("workspace-metadata.json"),
            )?;
        }
    } else {
        copy_file(&args.built_xtask, &staged_xtask)?;
        strip(&staged_xtask)?;
        write_workspace_metadata(&args.xtask_output.join("workspace-metadata.json"))?;
    }

    copy_dir_files(&args.tools_output, &args.combined_output)?;
    copy_dir_files(&args.xtask_output, &args.combined_output)
}

fn same_path(left: &Path, right: &Path) -> bool {
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn recreate_dir(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_dir_all(path)
            .with_context(|| format!("removing stale CI staging directory {}", path.display()))?;
    }
    fs::create_dir_all(path)
        .with_context(|| format!("creating CI staging directory {}", path.display()))
}

fn copy_cached_tools(source: &Path, destination: &Path) -> Result<()> {
    validate_cached_tools(source)?;
    for tool in TOOLS {
        copy_file(&source.join(tool), &destination.join(tool))?;
    }
    Ok(())
}

fn validate_cached_tools(source: &Path) -> Result<()> {
    for tool in TOOLS {
        if !source.join(tool).is_file() {
            bail!(
                "required staged CI input is missing: {}",
                source.join(tool).display()
            );
        }
    }
    Ok(())
}

fn stage_mise_tools(destination: &Path) -> Result<()> {
    for tool in TOOLS {
        let mut command = Command::new("mise");
        command.args(["which", tool]);
        let source = cmd::output_string(&mut command)
            .with_context(|| format!("locating {tool} through mise"))?;
        let staged = destination.join(tool);
        copy_file(Path::new(source.trim()), &staged)?;
        strip(&staged)?;
    }
    Ok(())
}

fn strip(path: &Path) -> Result<()> {
    let mut command = Command::new("strip");
    command.arg(path);
    cmd::run_streaming(&mut command)
        .with_context(|| format!("stripping staged binary {}", path.display()))
}

fn write_workspace_metadata(destination: &Path) -> Result<()> {
    let mut command = Command::new("cargo");
    command.args(["metadata", "--format-version", "1", "--locked", "--offline"]);
    let output = cmd::output(&mut command).context("collecting offline workspace metadata")?;
    fs::write(destination, output).with_context(|| format!("writing {}", destination.display()))
}

fn copy_dir_files(source: &Path, destination: &Path) -> Result<()> {
    for entry in crate::fs_util::read_dir_sorted(source)? {
        if entry.file_type()?.is_file() {
            copy_file(&entry.path(), &destination.join(entry.file_name()))?;
        }
    }
    Ok(())
}

fn copy_file(source: &Path, destination: &Path) -> Result<()> {
    if !source.is_file() {
        bail!("required staged CI input is missing: {}", source.display());
    }
    fs::copy(source, destination).with_context(|| {
        format!(
            "copying staged CI input {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(())
}
