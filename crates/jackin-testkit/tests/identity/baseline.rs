// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

const PACKAGE: &str = "jackin-console";

fn console_directory() -> PathBuf {
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--offline", "--locked"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdout(Stdio::piped())
        .spawn()
        .expect("start cargo metadata")
        .wait_with_output()
        .expect("read cargo metadata");
    assert!(output.status.success(), "cargo metadata failed");
    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("parse cargo metadata");
    let manifest = metadata["packages"]
        .as_array()
        .expect("the package list")
        .iter()
        .find(|package| package["name"] == PACKAGE)
        .and_then(|package| package["manifest_path"].as_str())
        .unwrap_or_else(|| panic!("cargo metadata lists no package {PACKAGE}"));
    Path::new(manifest)
        .parent()
        .expect("the manifest has a directory")
        .to_owned()
}

fn directory() -> &'static Path {
    static DIRECTORY: OnceLock<PathBuf> = OnceLock::new();
    DIRECTORY.get_or_init(console_directory)
}

pub(crate) fn root() -> &'static Path {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| directory().join("tests/functional/baselines"))
}

pub(crate) fn journeys() -> Vec<(String, String)> {
    ["jackin", "terminal_panes"]
        .into_iter()
        .map(|name| {
            let path = directory().join(format!("tests/functional/{name}.rs"));
            let source = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            (name.to_owned(), source)
        })
        .collect()
}
