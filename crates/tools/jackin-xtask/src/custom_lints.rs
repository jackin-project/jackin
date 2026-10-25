use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use jackin_process::{ExecRequest, ExecResult, StdioMode};

mod paths;

const NIGHTLY: &str = "nightly-2026-10-18";
const COMPILER_COMMIT: &str = "8006091eb71ae7d9e9bccb66c6122c9d68af4466";
const COMPILER_RELEASE: &str = "1.101.0-nightly";
const COMPONENTS: &[&str] = &["clippy", "llvm-tools", "rustc-dev", "rustfmt"];
const TOOL_NAMES: &[&str] = &["dylint-link", "cargo-nextest", "cargo-dylint"];

type Environment = BTreeMap<OsString, OsString>;

pub(crate) fn run(root: &Path) -> Result<()> {
    run_with(root, std::env::vars_os().collect(), execute)
}

fn run_with<F>(root: &Path, inherited: Environment, execute: F) -> Result<()>
where
    F: FnMut(&[OsString], &Path, &Environment, bool) -> Result<Vec<u8>>,
{
    run_with_home(root, inherited, paths::passwd_home_for_real_uid, execute)
}

fn run_with_home<F, H>(
    root: &Path,
    inherited: Environment,
    passwd_home: H,
    mut execute: F,
) -> Result<()>
where
    F: FnMut(&[OsString], &Path, &Environment, bool) -> Result<Vec<u8>>,
    H: FnOnce() -> Result<Option<OsString>>,
{
    let lint = root.join("crates/tools/jackin-lints");
    let (base_env, target, tools) = build_environment_with(root, inherited, passwd_home)?;
    let mut nightly = vec![OsString::from("rustup"), OsString::from("run")];
    nightly.push(OsString::from(NIGHTLY));

    let mut rustc = nightly.clone();
    rustc.extend([OsString::from("rustc"), OsString::from("-Vv")]);
    let version = execute(&rustc, &lint, &base_env, true)?;
    let host = compiler_host(&String::from_utf8(version).context("decoding rustc -Vv output")?)?;

    let components = vec![
        OsString::from("rustup"),
        OsString::from("component"),
        OsString::from("list"),
        OsString::from("--toolchain"),
        OsString::from(NIGHTLY),
        OsString::from("--installed"),
    ];
    let installed = execute(&components, &lint, &base_env, true)?;
    let installed = String::from_utf8(installed).context("decoding rustup component list")?;
    for component in COMPONENTS {
        let required = format!("{component}-{host}");
        if !installed.lines().any(|line| line == required) {
            bail!("missing pinned nightly component: {required}");
        }
    }

    let mut build = nightly.clone();
    build.extend([
        OsString::from("cargo"),
        OsString::from("build"),
        OsString::from("--locked"),
    ]);
    execute(&build, &lint, &base_env, false)?;

    let mut nextest = nightly;
    nextest.extend([
        tools["cargo-nextest"].as_os_str().to_owned(),
        OsString::from("nextest"),
        OsString::from("run"),
        OsString::from("--locked"),
    ]);
    execute(&nextest, &lint, &base_env, false)?;

    let library = library_path(&target, &host)?;
    let directory = tempfile::Builder::new()
        .prefix("jackin-custom-lints-")
        .tempdir()
        .context("creating isolated Dylint library directory")?;
    let destination = directory.path().join(
        library
            .file_name()
            .context("nightly lint library has no filename")?,
    );
    fs::copy(&library, &destination).context("copying qualified nightly lint library")?;

    let mut workspace_env = base_env;
    workspace_env.insert(
        OsString::from("CARGO_TARGET_DIR"),
        target.join("workspace").into_os_string(),
    );
    workspace_env.insert(
        OsString::from("DYLINT_LIBRARY_PATH"),
        directory.path().as_os_str().to_owned(),
    );
    workspace_env.remove(OsStr::new("DYLINT_LINK_ENABLE_COPY_LIBRARY"));
    workspace_env.remove(OsStr::new("CARGO_ENCODED_RUSTFLAGS"));
    workspace_env.remove(OsStr::new("RUSTFLAGS"));

    let mut dylint = vec![tools["cargo-dylint"].as_os_str().to_owned()];
    dylint.extend([
        OsString::from("dylint"),
        OsString::from("--no-metadata"),
        OsString::from("--all"),
        OsString::from("--fail-on-no-libraries"),
        OsString::from("--"),
        OsString::from("--workspace"),
        OsString::from("--locked"),
    ]);
    execute(&dylint, root, &workspace_env, false)?;
    Ok(())
}

fn build_environment_with<H>(
    root: &Path,
    inherited: Environment,
    passwd_home: H,
) -> Result<(Environment, PathBuf, BTreeMap<&'static str, PathBuf>)>
where
    H: FnOnce() -> Result<Option<OsString>>,
{
    let path = inherited
        .get(OsStr::new("PATH"))
        .cloned()
        .unwrap_or_default();
    let mut tools = BTreeMap::new();
    for name in TOOL_NAMES {
        let selected = find_executable(OsStr::new(name), &path)
            .with_context(|| format!("missing Mise-pinned {name}"))?;
        tools.insert(*name, selected);
    }

    let cargo_home = paths::cargo_home(&inherited, passwd_home)?;
    let target = match inherited.get(OsStr::new("CARGO_TARGET_DIR")) {
        Some(value) => PathBuf::from(value),
        None => root.join("target/custom-lints"),
    };
    let current_dir = if cargo_home.is_absolute() && target.is_absolute() {
        None
    } else {
        Some(std::env::current_dir().context("reading current directory")?)
    };
    let current_dir = current_dir.as_deref().unwrap_or_else(|| Path::new("/"));
    let cargo_home = paths::resolve(&cargo_home, current_dir)?;
    let target = paths::resolve(&target, current_dir)?;
    let cargo_bin = cargo_home.join("bin");
    let mut env = inherited;
    env.insert(
        OsString::from("PATH"),
        std::env::join_paths(std::iter::once(cargo_bin).chain(std::env::split_paths(&path)))
            .context("prepending Cargo home to PATH")?,
    );
    for key in env
        .keys()
        .filter(|key| key.to_string_lossy().starts_with("DYLINT_"))
        .cloned()
        .collect::<Vec<_>>()
    {
        env.remove(&key);
    }
    for key in [
        "CARGO",
        "RUSTUP_TOOLCHAIN",
        "RUSTC",
        "RUSTDOC",
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
    ] {
        env.remove(OsStr::new(key));
    }

    env.insert(
        OsString::from("CARGO_TARGET_DIR"),
        target.join("library").into_os_string(),
    );
    env.insert(
        OsString::from("DYLINT_LINK_ENABLE_COPY_LIBRARY"),
        OsString::from("1"),
    );
    let linker = tools["dylint-link"].as_os_str();
    let mut encoded_flags = OsString::from("-C\x1flinker=");
    encoded_flags.push(linker);
    env.insert(OsString::from("CARGO_ENCODED_RUSTFLAGS"), encoded_flags);
    Ok((env, target, tools))
}

fn find_executable(name: &OsStr, path: &OsStr) -> Result<PathBuf> {
    for directory in std::env::split_paths(path) {
        let candidate = directory.join(name);
        let Ok(metadata) = fs::metadata(&candidate) else {
            continue;
        };
        if !metadata.is_file() || !is_executable(&metadata) {
            continue;
        }
        return fs::canonicalize(&candidate)
            .with_context(|| format!("resolving executable {}", candidate.display()));
    }
    bail!(
        "executable {} is not present on PATH",
        name.to_string_lossy()
    )
}

#[cfg(unix)]
fn is_executable(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_: &fs::Metadata) -> bool {
    true
}

fn compiler_host(version: &str) -> Result<String> {
    let fields = version
        .lines()
        .filter_map(|line| line.split_once(": "))
        .collect::<BTreeMap<_, _>>();
    if fields.get("commit-hash") != Some(&COMPILER_COMMIT)
        || fields.get("release") != Some(&COMPILER_RELEASE)
    {
        bail!("custom lint compiler does not match the qualified nightly");
    }
    let host = fields.get("host").copied().unwrap_or_default();
    if !valid_host(host) {
        bail!("invalid compiler host");
    }
    Ok(host.to_owned())
}

fn valid_host(host: &str) -> bool {
    let mut segments = host.split('-');
    segments.clone().count() >= 2
        && segments.all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        })
}

fn library_path(target: &Path, host: &str) -> Result<PathBuf> {
    if !valid_host(host) {
        bail!("invalid compiler host");
    }
    let extension = match std::env::consts::OS {
        "macos" => "dylib",
        "linux" => "so",
        _ => bail!("custom lint qualification supports macOS and Linux"),
    };
    let name = format!("libjackin_lints@{NIGHTLY}-{host}.{extension}");
    let path = target.join("library/debug").join(name);
    let metadata = fs::symlink_metadata(&path)
        .with_context(|| format!("reading nightly-labelled lint library {}", path.display()))?;
    if !metadata.file_type().is_file() {
        bail!(
            "missing regular nightly-labelled lint library: {}",
            path.display()
        );
    }
    Ok(path)
}

fn execute(argv: &[OsString], cwd: &Path, env: &Environment, capture: bool) -> Result<Vec<u8>> {
    let display = argv
        .iter()
        .map(|part| part.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ");
    {
        let mut stdout = std::io::stdout().lock();
        writeln!(stdout, "custom-lints: {display}")
            .context("writing custom-lints command trace")?;
        stdout
            .flush()
            .context("flushing custom-lints command trace")?;
    }
    let request = build_exec_request(argv, cwd, env, capture)?;
    let output = jackin_process::exec_sync(&request)
        .with_context(|| format!("starting custom-lints command: {display}"))?;
    if capture {
        check_output(&output, &display)
    } else {
        check_status(&output, &display)?;
        Ok(Vec::new())
    }
}

fn build_exec_request(
    argv: &[OsString],
    cwd: &Path,
    env: &Environment,
    capture: bool,
) -> Result<ExecRequest> {
    let (program, args) = argv.split_first().context("empty custom-lints command")?;
    let mut request = ExecRequest::new(program.as_os_str(), args)
        .cwd(cwd)
        .env_clear()
        .envs(env.iter())
        .stdin_mode(StdioMode::Inherit)
        .stderr_mode(StdioMode::Inherit);
    if !capture {
        request = request.stdout_mode(StdioMode::Inherit);
    }
    Ok(request)
}

fn check_status(output: &ExecResult, display: &str) -> Result<()> {
    if !output.success {
        bail!(
            "custom-lints command failed ({}): {display}",
            status_description(output)
        );
    }
    Ok(())
}

fn check_output(output: &ExecResult, display: &str) -> Result<Vec<u8>> {
    if !output.success {
        bail!(
            "custom-lints command failed ({}){}: {display}",
            status_description(output),
            if output.stderr.is_empty() {
                String::new()
            } else {
                format!(": {}", String::from_utf8_lossy(&output.stderr).trim())
            }
        );
    }
    Ok(output.stdout.clone())
}

fn status_description(output: &ExecResult) -> String {
    match output.code {
        Some(code) => format!("exit status: {code}"),
        None if output.timed_out => "timed out".to_owned(),
        None => "terminated without an exit code".to_owned(),
    }
}

#[cfg(test)]
mod tests;
