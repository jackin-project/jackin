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
const TOOL_NAMES: &[&str] = &["dylint-link", "cargo-nextest", "cargo-dylint", "mbx"];

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
    let search_path = tool_search_path(&inherited, root, &mut execute)?;
    let (mut base_env, target, tools) =
        build_environment_with(root, inherited, &search_path, passwd_home)?;
    let _cargo_shim = create_cargo_shim(&mut base_env, &tools)?;
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
        tools["mbx"].as_os_str().to_owned(),
        OsString::from("build"),
        OsString::from("--locked"),
    ]);
    execute(&build, &lint, &base_env, false)?;

    let mut nextest = nightly;
    nextest.extend([
        tools["mbx"].as_os_str().to_owned(),
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
    let body_result = (|| -> Result<()> {
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
    })();

    finish_with_cleanup(body_result, directory.close())
}

fn create_cargo_shim(
    env: &mut Environment,
    tools: &BTreeMap<&'static str, PathBuf>,
) -> Result<tempfile::TempDir> {
    #[cfg(unix)]
    {
        let directory = tempfile::Builder::new()
            .prefix("jackin-custom-lints-mbx-")
            .tempdir()
            .context("creating private MBX Cargo shim directory")?;
        let alias = directory.path().join("cargo");
        std::os::unix::fs::symlink(&tools["mbx"], &alias)
            .context("creating private native Cargo-to-MBX shim")?;
        let old_path = env.get(OsStr::new("PATH")).cloned().unwrap_or_default();
        let nextest_parent = tools["cargo-nextest"]
            .parent()
            .context("pinned cargo-nextest has no parent directory")?
            .to_path_buf();
        let mut path = vec![directory.path().to_path_buf(), nextest_parent];
        let mut previous_path = std::env::split_paths(&old_path);
        if let Some(cargo_bin) = previous_path.next() {
            path.push(cargo_bin);
        }
        path.extend(previous_path);
        env.insert(
            OsString::from("PATH"),
            std::env::join_paths(path).context("prepending MBX Cargo shim to PATH")?,
        );
        env.insert(
            OsString::from("MBX_CARGO_SHIM_PATH"),
            alias.into_os_string(),
        );
        env.insert(OsString::from("MBX_CARGO_SHIM_MODE"), OsString::from("1"));
        Ok(directory)
    }
    #[cfg(not(unix))]
    {
        let _ = (env, tools);
        bail!("custom lint qualification requires Unix native Cargo shims")
    }
}

fn finish_with_cleanup<T>(body: Result<T>, cleanup: std::io::Result<()>) -> Result<T> {
    match (body, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error).context("removing isolated Dylint library directory"),
        (Err(body_error), Err(cleanup_error)) => Err(body_error).context(format!(
            "removing isolated Dylint library directory also failed: {cleanup_error}"
        )),
    }
}

fn build_environment_with<H>(
    root: &Path,
    inherited: Environment,
    search_path: &OsStr,
    passwd_home: H,
) -> Result<(Environment, PathBuf, BTreeMap<&'static str, PathBuf>)>
where
    H: FnOnce() -> Result<Option<OsString>>,
{
    let child_path = inherited
        .get(OsStr::new("PATH"))
        .cloned()
        .unwrap_or_default();
    let mut tools = BTreeMap::new();
    for name in TOOL_NAMES {
        let selected = find_executable(OsStr::new(name), search_path)
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
        std::env::join_paths(std::iter::once(cargo_bin).chain(std::env::split_paths(&child_path)))
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
        "MBX_CARGO_SHIM_MODE",
        "MBX_CARGO_SHIM_PATH",
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

fn tool_search_path<F>(inherited: &Environment, root: &Path, execute: &mut F) -> Result<OsString>
where
    F: FnMut(&[OsString], &Path, &Environment, bool) -> Result<Vec<u8>>,
{
    match inherited.get(OsStr::new("PATH")) {
        Some(path) if path.is_empty() => bail!("PATH is empty"),
        Some(path) => Ok(path.clone()),
        None => {
            let argv = [OsString::from("/usr/bin/getconf"), OsString::from("PATH")];
            let empty_env = Environment::new();
            let search_path = execute(&argv, root, &empty_env, true)
                .ok()
                .and_then(|output| parse_getconf_path(&output));
            Ok(search_path.unwrap_or_else(|| OsString::from("/bin:/usr/bin")))
        }
    }
}

fn parse_getconf_path(output: &[u8]) -> Option<OsString> {
    let output = std::str::from_utf8(output).ok()?;
    let output = output
        .strip_suffix("\r\n")
        .or_else(|| output.strip_suffix('\n'))
        .unwrap_or(output);
    if output.is_empty()
        || output
            .chars()
            .any(|character| matches!(character, '\r' | '\n' | '\0'))
    {
        return None;
    }
    Some(OsString::from(output))
}

fn find_executable(name: &OsStr, path: &OsStr) -> Result<PathBuf> {
    for directory in std::env::split_paths(path) {
        let candidate = directory.join(name);
        let Ok(metadata) = fs::metadata(&candidate) else {
            continue;
        };
        if !metadata.is_file() || !is_executable(&candidate) {
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
fn is_executable(path: &Path) -> bool {
    use nix::unistd::{AccessFlags, access};

    access(path, AccessFlags::F_OK | AccessFlags::X_OK).is_ok()
}

#[cfg(not(unix))]
fn is_executable(_: &Path) -> bool {
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
