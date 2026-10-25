use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};

use jackin_process::{RetryPolicy, StdioMode};

use super::paths::{cargo_home as cargo_home_path, resolve};
use super::{
    COMPILER_COMMIT, COMPILER_RELEASE, COMPONENTS, Environment, NIGHTLY, build_environment_with,
    build_exec_request, compiler_host, finish_with_cleanup, library_path, run_with, run_with_home,
};

mod case_01;

fn cargo_home<F>(inherited: &Environment, cwd: &Path, passwd_home: F) -> anyhow::Result<PathBuf>
where
    F: FnOnce() -> anyhow::Result<Option<OsString>>,
{
    let path = cargo_home_path(inherited, passwd_home)?;
    resolve(&path, cwd)
}

fn host() -> String {
    match std::env::consts::OS {
        "macos" => format!("{}-apple-darwin", std::env::consts::ARCH),
        "linux" => format!("{}-unknown-linux-gnu", std::env::consts::ARCH),
        other => panic!("unsupported test platform: {other}"),
    }
}

fn version(host: &str) -> String {
    format!("commit-hash: {COMPILER_COMMIT}\nrelease: {COMPILER_RELEASE}\nhost: {host}\n")
}

fn command_tools(path: &Path) {
    fs::create_dir_all(path).expect("tool directory");
    for name in ["dylint-link", "cargo-nextest", "cargo-dylint"] {
        let tool = path.join(name);
        fs::write(&tool, b"tool").expect("tool file");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).expect("executable mode");
        }
    }
}

#[test]
fn compiler_identity_accepts_only_qualified_commit_release_and_safe_host() {
    let host = host();
    assert_eq!(
        compiler_host(&version(&host)).expect("qualified compiler"),
        host
    );
    for supported in ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"] {
        assert_eq!(
            compiler_host(&version(supported)).expect("supported compiler host"),
            supported
        );
    }
    for invalid in [
        version(&host).replace(COMPILER_COMMIT, "other-commit"),
        version(&host).replace(COMPILER_RELEASE, "1.99.1"),
        version("../../escape"),
        version("x86_64-../../escape"),
        version("aarch64--darwin"),
    ] {
        assert!(compiler_host(&invalid).is_err(), "accepted {invalid:?}");
    }
}

#[test]
fn library_lookup_rejects_missing_and_symlinked_libraries() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let target = temp.path().join("target");
    let host = host();
    assert!(
        library_path(&target, &host)
            .expect_err("missing library should be rejected")
            .to_string()
            .contains("reading nightly-labelled lint library")
    );
    let debug = target.join("library/debug");
    fs::create_dir_all(&debug).expect("debug directory");
    let tagged = debug.join(library_name(&host));
    let foreign = temp.path().join("foreign-library");
    fs::write(&foreign, b"foreign ABI").expect("foreign library");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&foreign, &tagged).expect("library symlink");
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&foreign, &tagged).expect("library symlink");
    assert!(
        library_path(&target, &host)
            .expect_err("symlinked library should be rejected")
            .to_string()
            .contains("missing regular nightly-labelled lint library")
    );
}

fn library_name(host: &str) -> String {
    let extension = if cfg!(target_os = "macos") {
        "dylib"
    } else {
        "so"
    };
    format!("libjackin_lints@{NIGHTLY}-{host}.{extension}")
}

#[test]
fn environment_cleans_inherited_compiler_and_dylint_state_and_preserves_spaced_paths() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let tools = temp.path().join("pinned tools");
    command_tools(&tools);
    let cargo_home = temp.path().join("cargo home");
    fs::create_dir_all(cargo_home.join("bin")).expect("Cargo home bin");
    let root = temp.path().join("repository");
    fs::create_dir_all(&root).expect("repository");
    let mut inherited = BTreeMap::new();
    inherited.insert(OsString::from("PATH"), tools.as_os_str().to_owned());
    inherited.insert(
        OsString::from("CARGO_HOME"),
        cargo_home.as_os_str().to_owned(),
    );
    inherited.insert(
        OsString::from("CARGO_TARGET_DIR"),
        OsString::from("target dir"),
    );
    inherited.insert(OsString::from("RUSTUP_TOOLCHAIN"), OsString::from("stable"));
    inherited.insert(OsString::from("RUSTC"), OsString::from("foreign-rustc"));
    inherited.insert(OsString::from("RUSTDOC"), OsString::from("foreign-rustdoc"));
    inherited.insert(OsString::from("RUSTFLAGS"), OsString::from("--cfg foreign"));
    inherited.insert(
        OsString::from("CARGO_ENCODED_RUSTFLAGS"),
        OsString::from("foreign"),
    );
    inherited.insert(
        OsString::from("RUSTC_WRAPPER"),
        OsString::from("foreign-wrapper"),
    );
    inherited.insert(OsString::from("CARGO"), OsString::from("foreign-cargo"));
    inherited.insert(
        OsString::from("DYLINT_DRIVER_PATH"),
        OsString::from("foreign-driver"),
    );
    inherited.insert(
        OsString::from("DYLINT_RUSTFLAGS"),
        OsString::from("foreign-flags"),
    );
    inherited.insert(OsString::from("KEEP_ME"), OsString::from("preserved"));

    let (env, target, selected) = build_environment_with(&root, inherited, || {
        panic!("explicit CARGO_HOME should bypass passwd lookup")
    })
    .expect("build environment");
    let current_dir = std::env::current_dir().expect("current directory");
    let resolved_current_dir =
        resolve(&current_dir, &current_dir).expect("resolved current directory");
    let resolved_cargo_home = resolve(&cargo_home, &current_dir).expect("resolved Cargo home");
    assert_eq!(target, resolved_current_dir.join("target dir"));
    assert_eq!(
        env.get(OsStr::new("CARGO_TARGET_DIR")),
        Some(&target.join("library").into_os_string())
    );
    assert_eq!(
        env.get(OsStr::new("KEEP_ME")),
        Some(&OsString::from("preserved"))
    );
    for key in [
        "CARGO",
        "RUSTUP_TOOLCHAIN",
        "RUSTC",
        "RUSTDOC",
        "RUSTFLAGS",
        "RUSTC_WRAPPER",
        "DYLINT_DRIVER_PATH",
        "DYLINT_RUSTFLAGS",
    ] {
        assert!(!env.contains_key(OsStr::new(key)), "retained {key}");
    }
    let path =
        std::env::split_paths(env.get(OsStr::new("PATH")).expect("PATH")).collect::<Vec<_>>();
    assert_eq!(path[0], resolved_cargo_home.join("bin"));
    assert_eq!(path[1], tools);
    let encoded = env
        .get(OsStr::new("CARGO_ENCODED_RUSTFLAGS"))
        .expect("encoded flags");
    let parts = encoded
        .to_string_lossy()
        .split('\x1f')
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert_eq!(parts[0], "-C");
    assert_eq!(
        parts[1],
        format!("linker={}", selected["dylint-link"].display())
    );

    let cwd = root.join("crates/tools/jackin-lints");
    let argv = vec![
        OsString::from("rustup"),
        OsString::from("run"),
        OsString::from(NIGHTLY),
        OsString::from("rustc"),
        OsString::from("-Vv"),
    ];
    let expected_env = env
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<Vec<_>>();
    let captured = build_exec_request(&argv, &cwd, &env, true).expect("capture request");
    assert_eq!(captured.program, PathBuf::from(argv[0].as_os_str()));
    assert_eq!(captured.args, argv[1..].to_vec());
    assert_eq!(captured.cwd.as_deref(), Some(cwd.as_path()));
    assert!(captured.env_clear);
    assert!(captured.env_remove.is_empty());
    assert_eq!(captured.env, expected_env);
    assert_eq!(captured.stdin_mode, StdioMode::Inherit);
    assert_eq!(captured.stdout_mode, StdioMode::Capture);
    assert_eq!(captured.stderr_mode, StdioMode::Inherit);
    assert_eq!(captured.retry, RetryPolicy::none());
    assert_eq!(captured.timeout, None);

    let streamed = build_exec_request(&argv, &cwd, &env, false).expect("streaming request");
    assert_eq!(streamed.program, PathBuf::from(argv[0].as_os_str()));
    assert_eq!(streamed.args, argv[1..].to_vec());
    assert_eq!(streamed.cwd.as_deref(), Some(cwd.as_path()));
    assert!(streamed.env_clear);
    assert!(streamed.env_remove.is_empty());
    assert_eq!(streamed.env, expected_env);
    assert_eq!(streamed.stdin_mode, StdioMode::Inherit);
    assert_eq!(streamed.stdout_mode, StdioMode::Inherit);
    assert_eq!(streamed.stderr_mode, StdioMode::Inherit);
    assert_eq!(streamed.retry, RetryPolicy::none());
    assert_eq!(streamed.timeout, None);
}

#[test]
fn pipeline_checks_components_builds_one_fresh_abi_library_and_runs_workspace_dylint() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let root = temp.path().join("repository");
    fs::create_dir_all(root.join("crates/tools/jackin-lints")).expect("lint crate");
    let tools = temp.path().join("tools");
    command_tools(&tools);
    let target = temp.path().join("build target");
    let host = host();
    let mut inherited = BTreeMap::new();
    inherited.insert(OsString::from("PATH"), tools.as_os_str().to_owned());
    inherited.insert(OsString::from("HOME"), temp.path().as_os_str().to_owned());
    inherited.insert(
        OsString::from("CARGO_TARGET_DIR"),
        target.as_os_str().to_owned(),
    );
    inherited.insert(OsString::from("RUSTUP_TOOLCHAIN"), OsString::from("stable"));
    inherited.insert(
        OsString::from("DYLINT_LIBRARY_PATH"),
        OsString::from("stale"),
    );
    let failure_inherited = inherited.clone();

    let stale_library = target
        .join("library/debug")
        .join(library_name(&host).replace(NIGHTLY, "nightly-2026-10-17"));
    fs::create_dir_all(stale_library.parent().expect("stale library directory"))
        .expect("stale library directory");
    fs::write(&stale_library, b"stale ABI library").expect("stale library");

    let cargo_dylint = fs::canonicalize(tools.join("cargo-dylint"))
        .expect("selected Dylint executable")
        .into_os_string();
    let expected_workspace_target = fs::canonicalize(&target)
        .expect("created target directory")
        .join("workspace")
        .into_os_string();
    let mut calls = Vec::new();
    run_with(&root, inherited, |argv, cwd, env, capture| {
        calls.push((argv.to_vec(), cwd.to_path_buf(), env.clone(), capture));
        if argv.ends_with(&[OsString::from("rustc"), OsString::from("-Vv")]) {
            return Ok(version(&host).into_bytes());
        }
        if argv.ends_with(&[OsString::from("--installed")]) {
            let installed = COMPONENTS
                .iter()
                .fold(String::new(), |mut output, component| {
                    output.push_str(component);
                    output.push('-');
                    output.push_str(&host);
                    output.push('\n');
                    output
                });
            return Ok(installed.into_bytes());
        }
        if argv.ends_with(&[OsString::from("build"), OsString::from("--locked")]) {
            let library = PathBuf::from(env.get(OsStr::new("CARGO_TARGET_DIR")).expect("target"))
                .join("debug")
                .join(library_name(&host));
            fs::create_dir_all(library.parent().expect("library parent"))
                .expect("library directory");
            fs::write(library, b"fresh qualified library").expect("built library");
        }
        if argv.first() == Some(&cargo_dylint) {
            let library_dir = PathBuf::from(
                env.get(OsStr::new("DYLINT_LIBRARY_PATH"))
                    .expect("library path"),
            );
            let entries = crate::fs_util::read_dir_sorted(&library_dir)
                .expect("isolated Dylint library directory");
            assert_eq!(entries.len(), 1);
            assert!(
                stale_library.is_file(),
                "stale build output remains isolated"
            );
            assert_eq!(
                entries[0].file_name().to_string_lossy(),
                library_name(&host)
            );
            assert_eq!(
                fs::read(entries[0].path()).expect("library bytes"),
                b"fresh qualified library"
            );
            assert!(
                !library_dir
                    .join(stale_library.file_name().expect("stale filename"))
                    .exists(),
                "stale build output was copied into the Dylint search directory"
            );
            assert_eq!(cwd, root);
            assert_eq!(
                env.get(OsStr::new("CARGO_TARGET_DIR")),
                Some(&expected_workspace_target)
            );
            assert!(!env.contains_key(OsStr::new("CARGO_ENCODED_RUSTFLAGS")));
            assert!(!env.contains_key(OsStr::new("RUSTFLAGS")));
            assert!(!env.contains_key(OsStr::new("DYLINT_LINK_ENABLE_COPY_LIBRARY")));
            assert!(!env.contains_key(OsStr::new("RUSTUP_TOOLCHAIN")));
        }
        Ok(Vec::new())
    })
    .expect("custom lint pipeline");

    assert_eq!(calls.len(), 5);
    assert_eq!(calls[4].0.first(), Some(&cargo_dylint));
    assert!(calls[0].3);
    assert!(calls[1].3);
    assert_eq!(calls[2].0[3..], ["cargo", "build", "--locked"]);
    assert_eq!(
        calls[3].0[3],
        fs::canonicalize(tools.join("cargo-nextest"))
            .expect("selected Nextest executable")
            .into_os_string()
    );
    assert_eq!(calls[3].0[4..], ["nextest", "run", "--locked"]);
    assert_eq!(
        calls[4].0[1..],
        [
            "dylint",
            "--no-metadata",
            "--all",
            "--fail-on-no-libraries",
            "--",
            "--workspace",
            "--locked"
        ]
    );
    for (_, cwd, env, _) in calls.iter().take(4) {
        assert_eq!(cwd, &root.join("crates/tools/jackin-lints"));
        assert!(!env.contains_key(OsStr::new("DYLINT_LIBRARY_PATH")));
    }
    let isolated_library_dir = PathBuf::from(
        calls[4]
            .2
            .get(OsStr::new("DYLINT_LIBRARY_PATH"))
            .expect("isolated library path"),
    );
    assert!(
        !isolated_library_dir.exists(),
        "temporary Dylint library directory survived the pipeline"
    );

    assert_workspace_failure_cleans_library(
        &root,
        &target,
        &host,
        failure_inherited,
        &cargo_dylint,
    );
    assert_cleanup_result_combinations();
}

fn assert_workspace_failure_cleans_library(
    root: &Path,
    target: &Path,
    host: &str,
    inherited: Environment,
    cargo_dylint: &OsString,
) {
    let mut failure_library_dir = None;
    let error = run_with(root, inherited, |argv, _, env, _| {
        if argv.ends_with(&[OsString::from("rustc"), OsString::from("-Vv")]) {
            return Ok(version(host).into_bytes());
        }
        if argv.ends_with(&[OsString::from("--installed")]) {
            let installed = COMPONENTS
                .iter()
                .fold(String::new(), |mut output, component| {
                    output.push_str(component);
                    output.push('-');
                    output.push_str(host);
                    output.push('\n');
                    output
                });
            return Ok(installed.into_bytes());
        }
        if argv.ends_with(&[OsString::from("build"), OsString::from("--locked")]) {
            let library = target.join("library/debug").join(library_name(host));
            fs::create_dir_all(library.parent().expect("library parent"))
                .expect("library directory");
            fs::write(library, b"fresh qualified library").expect("built library");
        }
        if argv.first() == Some(cargo_dylint) {
            failure_library_dir = Some(PathBuf::from(
                env.get(OsStr::new("DYLINT_LIBRARY_PATH"))
                    .expect("isolated library path"),
            ));
            anyhow::bail!("simulated workspace failure");
        }
        Ok(Vec::new())
    })
    .expect_err("workspace failure should be returned");
    assert!(format!("{error:#}").contains("simulated workspace failure"));
    assert!(
        !failure_library_dir
            .expect("workspace command was reached")
            .exists(),
        "temporary Dylint library directory survived a workspace failure"
    );
}

fn assert_cleanup_result_combinations() {
    assert_eq!(
        finish_with_cleanup(Ok(7), Ok(())).expect("successful body and cleanup"),
        7
    );

    let body_error = finish_with_cleanup::<()>(Err(anyhow::anyhow!("body failure")), Ok(()))
        .expect_err("body failure should be returned");
    assert_eq!(format!("{body_error:#}"), "body failure");

    let cleanup_error =
        finish_with_cleanup::<()>(Ok(()), Err(std::io::Error::other("cleanup failure")))
            .expect_err("cleanup failure should be returned");
    let cleanup_message = format!("{cleanup_error:#}");
    assert!(cleanup_message.contains("removing isolated Dylint library directory"));
    assert!(cleanup_message.contains("cleanup failure"));

    let body_source = std::io::Error::other("body source");
    let body_error = anyhow::Error::new(body_source).context("body context");
    let both_errors = finish_with_cleanup::<()>(
        Err(body_error),
        Err(std::io::Error::other("cleanup failure")),
    )
    .expect_err("both failures should be reported");
    let both_message = format!("{both_errors:#}");
    assert!(both_message.contains("body context"));
    assert!(both_message.contains("cleanup failure"));
    assert_eq!(both_errors.root_cause().to_string(), "body source");
}

#[test]
fn missing_nightly_component_stops_before_compilation() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let root = temp.path().join("repository");
    fs::create_dir_all(root.join("crates/tools/jackin-lints")).expect("lint crate");
    let tools = temp.path().join("tools");
    command_tools(&tools);
    let mut inherited = BTreeMap::new();
    inherited.insert(OsString::from("PATH"), tools.as_os_str().to_owned());
    inherited.insert(OsString::from("HOME"), temp.path().as_os_str().to_owned());
    let mut calls = Vec::new();
    let result = run_with(&root, inherited, |argv, _, _, _| {
        calls.push(argv.to_vec());
        if argv.ends_with(&[OsString::from("rustc"), OsString::from("-Vv")]) {
            Ok(version(&host()).into_bytes())
        } else {
            Ok(Vec::new())
        }
    });
    assert!(result.is_err());
    assert!(
        result
            .expect_err("missing component")
            .to_string()
            .contains("missing pinned nightly component")
    );
    assert_eq!(calls.len(), 2);
}

#[test]
fn nightly_build_failure_stops_before_nextest_and_workspace_lints() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let root = temp.path().join("repository");
    fs::create_dir_all(root.join("crates/tools/jackin-lints")).expect("lint crate");
    let tools = temp.path().join("tools");
    command_tools(&tools);
    let mut inherited = BTreeMap::new();
    inherited.insert(OsString::from("PATH"), tools.as_os_str().to_owned());
    inherited.insert(OsString::from("HOME"), temp.path().as_os_str().to_owned());
    let mut calls = Vec::new();
    let result = run_with(&root, inherited, |argv, _, _, _| {
        calls.push(argv.to_vec());
        if argv.ends_with(&[OsString::from("rustc"), OsString::from("-Vv")]) {
            return Ok(version(&host()).into_bytes());
        }
        if argv.ends_with(&[OsString::from("--installed")]) {
            let host = host();
            let installed = COMPONENTS
                .iter()
                .fold(String::new(), |mut output, component| {
                    output.push_str(component);
                    output.push('-');
                    output.push_str(&host);
                    output.push('\n');
                    output
                });
            return Ok(installed.into_bytes());
        }
        if argv.ends_with(&[OsString::from("build"), OsString::from("--locked")]) {
            anyhow::bail!("simulated nightly build failure");
        }
        Ok(Vec::new())
    });
    assert!(
        result
            .expect_err("nightly build failure")
            .to_string()
            .contains("simulated nightly build failure")
    );
    assert_eq!(calls.len(), 3);
    assert!(calls[2].ends_with(&[OsString::from("build"), OsString::from("--locked")]));
}
