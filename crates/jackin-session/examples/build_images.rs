// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::fs::{self, DirBuilder};
use std::io::{ErrorKind, Write};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use jackin_core::{
    Agent, BuilderName, CommitId, DockerLayer, FileSystem, HostPath, Platform, ProcessRunner,
    ProcessSpec, ProcessStatus, ProcessStdin, Program, RepoPath, RoleName, Sha256, StdFileSystem,
};
use jackin_image::{
    BuildxEngine, ContextEntry, HookFiles, ImageEvent, ImageRequest, Rebuild, RoleRevision,
    SourceFile, ensure_image, read_regular_file,
};
use jackin_session::TokioProcessRunner;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

struct Options {
    role_dir: Option<PathBuf>,
    role_name: Option<String>,
    builder: String,
    work_dir: PathBuf,
    rebuild: Rebuild,
    calls: usize,
}

fn state_work_dir() -> Result<PathBuf, Box<dyn Error>> {
    let state = match std::env::var_os("XDG_STATE_HOME") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from(std::env::var_os("HOME").ok_or("HOME is not set")?)
            .join(".local")
            .join("state"),
    };
    Ok(state.join("jackin").join("build-images"))
}

fn private_dir(dir: &Path) -> Result<(), Box<dyn Error>> {
    match fs::symlink_metadata(dir) {
        Ok(meta) if meta.is_dir() && meta.permissions().mode() & 0o777 == 0o700 => Ok(()),
        Ok(meta) => Err(format!(
            "{} must be a directory with mode 0700, found mode {:o}",
            dir.display(),
            meta.permissions().mode() & 0o7777
        )
        .into()),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            if let Some(parent) = dir.parent() {
                fs::create_dir_all(parent)?;
            }
            DirBuilder::new().mode(0o700).create(dir)?;
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

fn options() -> Result<Options, Box<dyn Error>> {
    let mut options = Options {
        role_dir: None,
        role_name: None,
        builder: "orbstack".to_owned(),
        work_dir: state_work_dir()?,
        rebuild: Rebuild::No,
        calls: 2,
    };
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut value = |name: &str| args.next().ok_or_else(|| format!("{name} needs a value"));
        match flag.as_str() {
            "--role-dir" => options.role_dir = Some(PathBuf::from(value("--role-dir")?)),
            "--role-name" => options.role_name = Some(value("--role-name")?),
            "--builder" => options.builder = value("--builder")?,
            "--work-dir" => options.work_dir = PathBuf::from(value("--work-dir")?),
            "--calls" => options.calls = value("--calls")?.parse()?,
            "--rebuild" => options.rebuild = Rebuild::Yes,
            other => return Err(format!("unknown argument {other}").into()),
        }
    }
    Ok(options)
}

async fn git(
    runner: &dyn ProcessRunner,
    dir: &Path,
    args: &[&str],
) -> Result<Vec<u8>, Box<dyn Error>> {
    let spec = ProcessSpec {
        program: Program::Git,
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        cwd: Some(HostPath::parse(dir)?),
        env: Vec::new(),
        env_remove_prefixes: vec!["GIT_".to_owned()],
        stdin: ProcessStdin::Null,
        timeout: Duration::from_secs(60),
    };
    let output = runner.run(&spec, None, &CancellationToken::new()).await?;
    if output.status != ProcessStatus::Exited(0) {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git {args:?} ended with {:?}: {stderr}", output.status).into());
    }
    Ok(output.stdout)
}

async fn minimal_role(
    runner: &dyn ProcessRunner,
    work_dir: &Path,
) -> Result<PathBuf, Box<dyn Error>> {
    let dir = work_dir.join("minimal-role");
    match fs::remove_dir_all(&dir) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    fs::create_dir_all(&dir)?;
    fs::write(dir.join("Dockerfile"), "FROM construct AS role\n")?;
    git(runner, &dir, &["init", "-q"]).await?;
    git(runner, &dir, &["add", "-A"]).await?;
    let commit = [
        "-c",
        "user.name=jackin",
        "-c",
        "user.email=jackin@example.invalid",
        "-c",
        "commit.gpgsign=false",
        "commit",
        "-q",
        "-m",
        "minimal role",
    ];
    git(runner, &dir, &commit).await?;
    Ok(dir)
}

async fn request(
    runner: &dyn ProcessRunner,
    dir: &Path,
    options: &Options,
) -> Result<ImageRequest, Box<dyn Error>> {
    let changes = git(
        runner,
        dir,
        &["status", "--porcelain", "-z", "--untracked-files=all"],
    )
    .await?;
    if !changes.is_empty() {
        return Err(format!(
            "{} has uncommitted or untracked files; commit them first:\n{}",
            dir.display(),
            String::from_utf8_lossy(&changes).replace('\0', "\n")
        )
        .into());
    }
    let listing = git(
        runner,
        dir,
        &["ls-files", "-co", "--exclude-standard", "-z"],
    )
    .await?;
    let head = String::from_utf8(git(runner, dir, &["rev-parse", "HEAD"]).await?)?;
    let root = HostPath::parse(dir)?;
    let fs = StdFileSystem::default();
    let mut files = Vec::new();
    let mut digest_input = Vec::new();
    for name in String::from_utf8(listing)?
        .split('\0')
        .filter(|name| !name.is_empty())
    {
        let path = RepoPath::parse(name)?;
        let file = read_regular_file(&fs, &root, &path).await?;
        digest_input.extend(name.as_bytes());
        digest_input.extend(Sha256::of(&file.bytes).as_str().as_bytes());
        files.push(ContextEntry::File {
            path,
            executable: file.mode.0 & 0o111 != 0,
            bytes: file.bytes,
        });
    }
    let dockerfile = RepoPath::parse("Dockerfile")?;
    let bytes = read_regular_file(&fs, &root, &dockerfile).await?.bytes;
    let name = match &options.role_name {
        Some(name) => name.clone(),
        None => dir
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("the role directory has no name")?
            .to_owned(),
    };
    Ok(ImageRequest {
        role: RoleName::parse(&name)?,
        context_files: files,
        dockerfile: SourceFile {
            path: dockerfile,
            bytes,
        },
        hooks: HookFiles {
            setup: None,
            preflight: None,
        },
        revision: RoleRevision {
            commit: CommitId::parse(head.trim())?,
            digest: Sha256::of(&digest_input),
        },
        agent: Agent::Codex,
        agents: vec![Agent::Codex],
        platform: Platform::host_default(),
        builder: BuilderName::parse(&options.builder)?,
        docker: DockerLayer::Absent,
        rebuild: options.rebuild,
    })
}

fn show(event: &ImageEvent) -> Result<(), Box<dyn Error>> {
    let mut out = std::io::stdout().lock();
    match event {
        ImageEvent::Line { stage, text } => writeln!(out, "  [{stage}] {}", text.as_str())?,
        other => writeln!(out, "event {other:?}")?,
    }
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    let options = options()?;
    private_dir(&options.work_dir)?;
    let work = options.work_dir.canonicalize()?;
    let runner = Arc::new(TokioProcessRunner);
    let role_dir = match &options.role_dir {
        Some(dir) => dir.canonicalize()?,
        None => minimal_role(runner.as_ref(), &work).await?,
    };
    let request = request(runner.as_ref(), &role_dir, &options).await?;
    let fs: Arc<dyn FileSystem> = Arc::new(StdFileSystem::default());
    let engine = BuildxEngine::new(runner, fs, HostPath::parse(&work.join("engine"))?);
    let cancel = CancellationToken::new();
    for call in 1..=options.calls {
        let (tx, mut rx) = mpsc::channel::<ImageEvent>(256);
        let run = async {
            let result = ensure_image(&engine, &request, &tx, &cancel).await;
            drop(tx);
            result
        };
        let print = async {
            while let Some(event) = rx.recv().await {
                show(&event)?;
            }
            Ok::<(), Box<dyn Error>>(())
        };
        let (result, printed) = tokio::join!(run, print);
        printed?;
        let image = result?;
        writeln!(
            std::io::stdout().lock(),
            "call {call}: tag={} id={} source={:?}",
            image.tag,
            image.id,
            image.source
        )?;
    }
    Ok(())
}
