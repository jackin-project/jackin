//! Bounded Cargo identity observation for the file-backed workspace oracle.

use anyhow::{Context, Result, ensure};
use serde_json::Value as YamlValue;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

pub(super) fn workspace_package_ids(root: &Path) -> Result<BTreeMap<PathBuf, String>> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let request = jackin_process::ExecRequest::new(
        cargo,
        [
            "metadata",
            "--no-deps",
            "--locked",
            "--offline",
            "--format-version",
            "1",
        ],
    )
    .cwd(root)
    .timeout(std::time::Duration::from_secs(30))
    .output_limits(16 * 1024 * 1024, 64 * 1024);
    let output = jackin_process::exec_sync(&request)?;
    ensure!(
        output.success && !output.timed_out,
        "bounded read-only Cargo metadata succeeds: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: YamlValue = serde_json::from_slice(&output.stdout)?;
    let members = metadata["workspace_members"]
        .as_array()
        .context("metadata workspace members")?
        .iter()
        .map(|id| id.as_str().context("metadata member ID"))
        .collect::<Result<BTreeSet<_>>>()?;
    let mut packages = BTreeMap::new();
    for package in metadata["packages"]
        .as_array()
        .context("metadata packages")?
    {
        let id = package["id"].as_str().context("metadata package ID")?;
        if members.contains(id) {
            let path = Path::new(
                package["manifest_path"]
                    .as_str()
                    .context("metadata manifest path")?,
            )
            .canonicalize()?;
            ensure!(
                packages.insert(path, id.to_owned()).is_none(),
                "unique metadata manifest"
            );
        }
    }
    ensure!(
        packages.len() == members.len(),
        "every Cargo workspace ID has one package"
    );
    Ok(packages)
}
