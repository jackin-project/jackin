// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use jackin_xtask::Repo;
use tempfile::TempDir;

pub(crate) type TestResult = Result<(), Box<dyn Error>>;

pub(crate) const URL: &str = "https://github.com/tailrocks/terminal-components-baseline";
pub(crate) const REV: &str = "74244972a1183db85111332ddab7eeabcbae81cb";
pub(crate) const OTHER_REV: &str = "1111111111111111111111111111111111111111";

pub(crate) struct Workspace {
    dir: TempDir,
    root: PathBuf,
}

impl Workspace {
    pub(crate) fn empty() -> Result<Self, Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().join("workspace");
        fs::create_dir(&root)?;
        let workspace = Self { dir, root };
        for name in ["jackin-console", "junie-tui"] {
            let sibling = workspace.dir.path().join("elsewhere").join(name);
            fs::create_dir_all(sibling.join("src"))?;
            fs::write(
                sibling.join("Cargo.toml"),
                format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n"),
            )?;
            fs::write(sibling.join("src/lib.rs"), "")?;
        }
        workspace.write(".gitignore", "target/\n")?;
        let status = jackin_testkit::git_command()
            .args(["init", "-q"])
            .current_dir(&workspace.root)
            .status()?;
        if !status.success() {
            return Err(format!("git init exited with {status}").into());
        }
        Ok(workspace)
    }

    pub(crate) fn write(&self, relative: &str, text: &str) -> TestResult {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, text)?;
        Ok(())
    }

    pub(crate) fn repo(&self) -> Result<Repo, Box<dyn Error>> {
        Ok(Repo::open(self.root.clone())?)
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
}

pub(crate) fn pin(rev: &str) -> String {
    format!("git = \"{URL}\", rev = \"{rev}\"")
}

pub(crate) fn workspace_manifest(members: &[&str], dependencies: &[String]) -> String {
    let members: Vec<String> = members
        .iter()
        .map(|member| format!("\"crates/{member}\""))
        .collect();
    format!(
        "[workspace]\nmembers = [{}]\nresolver = \"3\"\n\n[workspace.package]\nversion = \"0.1.0\"\nedition = \"2024\"\nrust-version = \"1.99.1\"\n\n[workspace.dependencies]\n{}\n",
        members.join(", "),
        dependencies.join("\n")
    )
}

pub(crate) fn console_line(rev: &str) -> String {
    format!(
        "jackin-console = {{ version = \"0.1.0\", {}, default-features = false }}",
        pin(rev)
    )
}

pub(crate) fn junie_line(rev: &str) -> String {
    format!(
        "junie-tui = {{ version = \"0.1.0\", {}, default-features = false, features = [\"terminal\"] }}",
        pin(rev)
    )
}

pub(crate) fn member_manifest(name: &str, sections: &str) -> String {
    format!(
        "[package]\nname = \"{name}\"\nversion.workspace = true\nedition.workspace = true\n\n{sections}"
    )
}

pub(crate) fn lock_entry(name: &str, source: &str) -> String {
    format!("[[package]]\nname = \"{name}\"\nversion = \"0.1.0\"\nsource = \"{source}\"\n\n")
}

pub(crate) fn lock_source(rev: &str) -> String {
    format!("git+{URL}?rev={rev}#{rev}")
}

pub(crate) fn lock_text(entries: &[(&str, String)]) -> String {
    let body: String = entries
        .iter()
        .map(|(name, source)| lock_entry(name, source))
        .collect();
    format!("version = 4\n\n{body}")
}

pub(crate) fn deny_text(allow_git: &[&str], unknown_git: &str) -> String {
    let urls: Vec<String> = allow_git.iter().map(|url| format!("\"{url}\"")).collect();
    format!(
        "[sources]\nunknown-git = \"{unknown_git}\"\nallow-git = [{}]\n",
        urls.join(", ")
    )
}

pub(crate) const OPENSHELL_URL: &str = "https://github.com/NVIDIA/OpenShell";
pub(crate) const TERMPANE_URL: &str = "https://github.com/tailrocks/termpane";

pub(crate) fn green_deny() -> String {
    deny_text(&[OPENSHELL_URL, URL, TERMPANE_URL], "deny")
}

pub(crate) fn green_lock() -> String {
    lock_text(&[
        ("jackin-console", lock_source(REV)),
        ("junie-tui", lock_source(REV)),
    ])
}

pub(crate) fn green_root() -> String {
    workspace_manifest(&["jackin-tui"], &[console_line(REV), junie_line(REV)])
}

pub(crate) fn green_tui() -> String {
    member_manifest(
        "jackin-tui",
        "[dependencies]\njackin-console = { workspace = true }\njunie-tui = { workspace = true }\n",
    )
}

pub(crate) fn testkit_manifest() -> String {
    member_manifest(
        "jackin-testkit",
        "[features]\nconsole-demo = [\"jackin-console/demo\"]\n\n[dependencies]\njackin-console = { workspace = true, features = [\"demo\"] }\n",
    )
}

pub(crate) fn green() -> Result<Workspace, Box<dyn Error>> {
    let workspace = Workspace::empty()?;
    workspace.write("Cargo.toml", &green_root())?;
    workspace.write("crates/jackin-tui/Cargo.toml", &green_tui())?;
    workspace.write("crates/jackin-tui/src/lib.rs", "")?;
    workspace.write("deny.toml", &green_deny())?;
    workspace.write("Cargo.lock", &green_lock())?;
    Ok(workspace)
}

pub(crate) fn add_member(workspace: &Workspace, name: &str, manifest: &str) -> TestResult {
    let root = fs::read_to_string(workspace.root().join("Cargo.toml"))?;
    let members_end = root
        .find("]\nresolver")
        .ok_or("the root manifest has no members list")?;
    let mut updated = String::new();
    updated.push_str(&root[..members_end]);
    updated.push_str(&format!(", \"crates/{name}\""));
    updated.push_str(&root[members_end..]);
    workspace.write("Cargo.toml", &updated)?;
    workspace.write(&format!("crates/{name}/Cargo.toml"), manifest)?;
    workspace.write(&format!("crates/{name}/src/lib.rs"), "")
}
