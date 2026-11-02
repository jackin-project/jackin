// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use jackin_core::{
    Agent, BuilderName, CommitId, DockerLayer, FileSystem, HostPath, Platform, ProcessRunner,
    ProcessSpec, ProcessStatus, ProcessStdin, Program, RepoPath, RoleName, StdFileSystem,
};
use jackin_image::{
    BuildxEngine, HookFiles, ImageEvent, ImageRequest, Rebuild, RoleRevision, SourceFile,
    ensure_image,
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

fn options() -> Result<Options, Box<dyn Error>> {
    let mut options = Options {
        role_dir: None,
        role_name: None,
        builder: "orbstack".to_owned(),
        work_dir: std::env::temp_dir().join("jackin-build-images"),
        rebuild: Rebuild::No,
        calls: 2,
    };
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut value = |name: &str| args.next().ok_or_else(|| format!("{name} needs a value"));
        match flag.as_str() {
            "--minimal" => {}
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
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
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
    let listing = git(
        runner,
        dir,
        &["ls-files", "-co", "--exclude-standard", "-z"],
    )
    .await?;
    let files = String::from_utf8(listing)?
        .split('\0')
        .filter(|name| !name.is_empty())
        .map(RepoPath::parse)
        .collect::<Result<Vec<_>, _>>()?;
    let head = String::from_utf8(git(runner, dir, &["rev-parse", "HEAD"]).await?)?;
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
        context: HostPath::parse(dir)?,
        context_files: files,
        dockerfile: SourceFile {
            path: RepoPath::parse("Dockerfile")?,
            bytes: fs::read(dir.join("Dockerfile"))?,
        },
        hooks: HookFiles {
            setup: None,
            preflight: None,
        },
        revision: RoleRevision::Clean {
            commit: CommitId::parse(head.trim())?,
        },
        agent: Agent::Codex,
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
    fs::create_dir_all(&options.work_dir)?;
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
