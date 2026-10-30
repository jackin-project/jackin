use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

const NIGHTLY: &str = "nightly-2026-10-18";
const COMPILER_COMMIT: &str = "8006091eb71ae7d9e9bccb66c6122c9d68af4466";
const COMPILER_RELEASE: &str = "1.101.0-nightly";
const COMPONENTS: &[&str] = &["clippy", "llvm-tools", "rustc-dev", "rustfmt"];
const TOOL_NAMES: &[&str] = &["dylint-link", "cargo-nextest", "cargo-dylint"];

type Environment = BTreeMap<OsString, OsString>;

pub(crate) fn run(root: &Path) -> Result<()> {
    run_with(root, std::env::vars_os().collect(), execute)
}

fn run_with<F>(root: &Path, inherited: Environment, mut execute: F) -> Result<()>
where
    F: FnMut(&[OsString], &Path, &Environment, bool) -> Result<Vec<u8>>,
{
    let lint = root.join("crates/tools/jackin-lints");
    let (base_env, target, tools) = build_environment(root, inherited)?;
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

fn build_environment(
    root: &Path,
    inherited: Environment,
) -> Result<(Environment, PathBuf, BTreeMap<&'static str, PathBuf>)> {
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

    let cargo_home = match inherited.get(OsStr::new("CARGO_HOME")) {
        Some(value) => PathBuf::from(value),
        None => PathBuf::from(
            inherited
                .get(OsStr::new("HOME"))
                .context("HOME is missing and CARGO_HOME is unset")?,
        )
        .join(".cargo"),
    };
    let cargo_bin = if cargo_home.is_absolute() {
        cargo_home.join("bin")
    } else {
        std::env::current_dir()
            .context("reading current directory")?
            .join(cargo_home)
            .join("bin")
    };
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

    let target = match env.get(OsStr::new("CARGO_TARGET_DIR")) {
        Some(value) => PathBuf::from(value),
        None => root.join("target/custom-lints"),
    };
    let target = if target.is_absolute() {
        target
    } else {
        std::env::current_dir()
            .context("reading current directory")?
            .join(target)
    };
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
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
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

#[expect(
    clippy::print_stderr,
    reason = "xtask command trace is operator-facing CLI output"
)]
fn execute(argv: &[OsString], cwd: &Path, env: &Environment, capture: bool) -> Result<Vec<u8>> {
    let display = argv
        .iter()
        .map(|part| part.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ");
    eprintln!("custom-lints: {display}");
    let (program, args) = argv.split_first().context("empty custom-lints command")?;
    let mut command = Command::new(program);
    command.args(args).current_dir(cwd).env_clear().envs(env);
    if capture {
        let output = execute_request(&command, true)
            .with_context(|| format!("starting custom-lints command: {display}"))?;
        check_output(&output, &display)
    } else {
        let output = execute_request(&command, false)
            .with_context(|| format!("starting custom-lints command: {display}"))?;
        check_status(&output, &display)?;
        Ok(Vec::new())
    }
}

fn execute_request(command: &Command, capture: bool) -> Result<jackin_process::ExecResult> {
    let mut request = command_request(command);
    if !capture {
        request = request
            .stdin_mode(jackin_process::StdioMode::Inherit)
            .stdout_mode(jackin_process::StdioMode::Inherit)
            .stderr_mode(jackin_process::StdioMode::Inherit);
    }
    jackin_process::exec_sync(&request)
}

fn check_status(output: &jackin_process::ExecResult, display: &str) -> Result<()> {
    if !output.success {
        bail!(
            "custom-lints command failed ({}): {display}",
            output
                .code
                .map_or_else(|| "signal".to_owned(), |code| code.to_string())
        );
    }
    Ok(())
}

fn command_request(command: &Command) -> jackin_process::ExecRequest {
    let program = command.get_program();
    let arguments: Vec<_> = command.get_args().collect();
    let mut request = jackin_process::ExecRequest::new(program, arguments)
        .env_clear()
        .envs(command.get_envs().filter_map(|(key, value)| {
            value.map(|value| (key.to_os_string(), value.to_os_string()))
        }));
    if let Some(cwd) = command.get_current_dir() {
        request = request.cwd(cwd);
    }
    request
}

fn check_output(output: &jackin_process::ExecResult, display: &str) -> Result<Vec<u8>> {
    if !output.success {
        bail!(
            "custom-lints command failed ({}){}: {display}",
            output
                .code
                .map_or_else(|| "signal".to_owned(), |code| code.to_string()),
            if output.stderr.is_empty() {
                String::new()
            } else {
                format!(": {}", String::from_utf8_lossy(&output.stderr).trim())
            }
        );
    }
    Ok(output.stdout.clone())
}

#[cfg(test)]
#[path = "custom_lints/tests.rs"]
mod tests;
