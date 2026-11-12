// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use tempfile::TempDir;
use tokio::io::AsyncWriteExt;

pub(crate) struct Trap {
    _dir: TempDir,
    root: PathBuf,
    marker: PathBuf,
}

const PROGRAMS: [&str; 6] = ["fsmonitor", "clean", "process", "ext", "ssh", "altrefs"];
const HOOKS: [&str; 3] = [
    "reference-transaction",
    "post-index-change",
    "post-checkout",
];
const NEEDED: [&str; 10] = [
    "fsmonitor:",
    "reference-transaction:",
    "post-index-change:",
    "post-checkout:",
    "clean:",
    "process:",
    "ext:",
    "ssh:",
    "altrefs:",
    "credential:",
];

fn script(marker: &Path, label: &str) -> String {
    format!(
        "#!/bin/sh\necho \"{label}: $(ps -o args= -p $PPID)\" >> '{}'\nexit 0\n",
        marker.display()
    )
}

async fn run(
    repo: &Path,
    envs: &[(&str, &Path)],
    stdin: Option<&[u8]>,
    args: &[&str],
) -> Result<bool, Box<dyn Error>> {
    let mut command = tokio::process::Command::new("/usr/bin/git");
    command
        .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid"])
        .args(args)
        .current_dir(repo)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for (name, value) in envs {
        command.env(name, value);
    }
    let mut child = command.spawn()?;
    if let (Some(bytes), Some(mut pipe)) = (stdin, child.stdin.take()) {
        pipe.write_all(bytes).await?;
    }
    Ok(child.wait().await?.success())
}

async fn git(repo: &Path, args: &[&str]) -> Result<(), Box<dyn Error>> {
    if run(repo, &[], None, args).await? {
        Ok(())
    } else {
        Err(format!("git {args:?} failed in {}", repo.display()).into())
    }
}

impl Trap {
    pub(crate) async fn new() -> Result<Trap, Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let root = std::fs::canonicalize(dir.path())?;
        let marker = root.join("marker");
        let hooks = root.join("hooks");
        tokio::fs::create_dir(&hooks).await?;
        for label in PROGRAMS {
            let file = root.join(label);
            tokio::fs::write(&file, script(&marker, label)).await?;
            tokio::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).await?;
        }
        let credential = root.join("credential");
        tokio::fs::write(&credential, script(&marker, "credential")).await?;
        tokio::fs::set_permissions(&credential, std::fs::Permissions::from_mode(0o755)).await?;
        for name in HOOKS {
            let hook = hooks.join(name);
            tokio::fs::write(&hook, script(&marker, name)).await?;
            tokio::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).await?;
        }
        git(&root, &["init", "-q", "--bare", "alternate.git"]).await?;
        Ok(Trap {
            _dir: dir,
            root,
            marker,
        })
    }

    pub(crate) fn program(&self, name: &str) -> String {
        self.root.join(name).display().to_string()
    }

    pub(crate) async fn arm_hostile(&self, repo: &Path) -> Result<(), Box<dyn Error>> {
        let ext = format!("ext::{} %s #", self.program("ext"));
        for (key, value) in [
            ("core.fsmonitor", self.program("fsmonitor")),
            ("core.hooksPath", self.program("hooks")),
            ("filter.evilclean.clean", self.program("clean")),
            ("filter.evilproc.process", self.program("process")),
            ("core.sshCommand", self.program("ssh")),
            ("core.alternateRefsCommand", self.program("altrefs")),
            ("credential.helper", self.program("credential")),
            ("protocol.ext.allow", "always".to_owned()),
        ] {
            git(repo, &["config", key, &value]).await?;
        }
        git(repo, &["config", &format!("url.{ext}.insteadOf"), "/"]).await?;
        let alternates = repo.join(".git/objects/info/alternates");
        tokio::fs::create_dir_all(repo.join(".git/objects/info")).await?;
        tokio::fs::write(
            alternates,
            format!("{}\n", self.program("alternate.git/objects")),
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn prove_armed(&self, repo: &Path) -> Result<(), Box<dyn Error>> {
        git(repo, &["ls-files", "-o"]).await?;
        git(repo, &["update-ref", "refs/trap/probe", "HEAD"]).await?;
        git(repo, &["update-ref", "-d", "refs/trap/probe"]).await?;
        run(repo, &[], None, &["ls-remote", "/x"]).await?;
        run(repo, &[], None, &["ls-remote", "ssh://h/x"]).await?;
        run(repo, &[], None, &["rev-list", "--alternate-refs", "--all"]).await?;
        run(
            repo,
            &[],
            Some(b"protocol=https\nhost=h\n\n"),
            &["credential", "fill"],
        )
        .await?;
        let index = self.root.join("probe-index");
        let attributes = repo.join(".git/info/attributes");
        tokio::fs::create_dir_all(repo.join(".git/info")).await?;
        tokio::fs::write(
            &attributes,
            "*.evilclean filter=evilclean\n*.evilproc filter=evilproc\n",
        )
        .await?;
        for name in ["probe.evilclean", "probe.evilproc"] {
            tokio::fs::write(repo.join(name), "probe\n").await?;
            run(repo, &[("GIT_INDEX_FILE", &index)], None, &["add", name]).await?;
            tokio::fs::remove_file(repo.join(name)).await?;
        }
        tokio::fs::remove_file(&attributes).await?;
        run(repo, &[], None, &["checkout", "-q", "-b", "trap-probe"]).await?;
        run(repo, &[], None, &["checkout", "-q", "-"]).await?;
        run(repo, &[], None, &["branch", "-q", "-D", "trap-probe"]).await?;
        tokio::fs::write(repo.join("probe.index"), "probe\n").await?;
        run(
            repo,
            &[("GIT_INDEX_FILE", &index)],
            None,
            &["add", "probe.index"],
        )
        .await?;
        tokio::fs::remove_file(repo.join("probe.index")).await?;
        let fired = self.fired();
        for needed in &NEEDED {
            if !fired.contains(needed) {
                return Err(
                    format!("the trap is not armed, {needed} did not fire: {fired:?}").into(),
                );
            }
        }
        self.clear()
    }

    pub(crate) fn clear(&self) -> Result<(), Box<dyn Error>> {
        std::fs::write(&self.marker, "")?;
        Ok(())
    }

    pub(crate) fn fired(&self) -> String {
        std::fs::read_to_string(&self.marker).unwrap_or_default()
    }
}
