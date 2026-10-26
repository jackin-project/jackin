// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;
#[cfg(not(target_os = "macos"))]
use std::os::unix::ffi::OsStringExt;
use std::os::unix::{ffi::OsStrExt, process::ExitStatusExt};
use std::process::{Command, ExitStatus, Output};

fn write_executable(path: &Path, contents: &[u8]) {
    fs::write(path, contents).expect("tool file");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("executable mode");
}

fn command_result(status: i32, stdout: &[u8]) -> Output {
    Output {
        status: ExitStatus::from_raw(status << 8),
        stdout: stdout.to_vec(),
        stderr: Vec::new(),
    }
}

fn physical_path(path: &Path, cwd: &Path) -> PathBuf {
    crate::cargo_paths::resolve(path, cwd).expect("physical fixture path")
}

#[test]
fn rustup_path_resolution_handles_actual_raw_unix_paths() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    #[cfg(target_os = "macos")]
    let raw_component = OsString::from(" rustup \t\r\n ");
    #[cfg(not(target_os = "macos"))]
    let raw_component = OsString::from_vec(b" rustup \t\r\n\xff ".to_vec());
    let rustup_home = temporary.path().join(raw_component);
    let toolchain_bin = rustup_home.join("toolchains/fixture-pin/bin");
    fs::create_dir_all(&toolchain_bin).expect("raw path toolchain directory");
    for name in ["rustc", "rustdoc", "clippy-driver", "rustfmt"] {
        write_executable(&toolchain_bin.join(name), b"fixture tool");
    }
    let rustup = temporary.path().join("rustup");
    let rustup_script = concat!(
        "#!/bin/sh\n",
        "set -eu\n",
        "[ \"$1\" = which ] && [ \"$2\" = --toolchain ] && [ \"$3\" = fixture-pin ] || exit 73\n",
        "path=\"$RUSTUP_HOME/toolchains/$3/bin/$4\"\n",
        "[ -f \"$path\" ] || exit 74\n",
        "printf '%s\\n' \"$path\"\n",
    );
    write_executable(&rustup, rustup_script.as_bytes());

    let resolved = resolve_rust_tools(
        &rustup,
        "fixture-pin",
        OsStr::new("/inherited/path"),
        Some(rustup_home.as_os_str()),
        None,
    )
    .expect("resolve raw Unix tool paths");

    for name in ["rustc", "rustdoc", "clippy-driver", "rustfmt"] {
        assert_eq!(
            resolved[name],
            fs::canonicalize(toolchain_bin.join(name)).expect("physical raw tool path")
        );
    }
}

#[test]
fn host_input_constructor_admits_named_context_and_private_mise_config() {
    let temporary = tempfile::tempdir().expect("temporary root");
    let root = temporary.path().join("reviewed");
    let isolation = temporary.path().join("isolation");
    let custom_data = temporary.path().join("custom mise data with spaces");
    fs::create_dir_all(&root).expect("reviewed root");
    fs::create_dir(&isolation).expect("isolation root");
    let inherited = BTreeMap::from([
        (OsString::from("PATH"), OsString::from("/host/bin")),
        (
            OsString::from("HOME"),
            temporary.path().join("home").into_os_string(),
        ),
        (
            OsString::from("CARGO_HOME"),
            temporary.path().join("cargo home").into_os_string(),
        ),
        (
            OsString::from("MISE_DATA_DIR"),
            custom_data.clone().into_os_string(),
        ),
        (
            OsString::from("MISE_CONFIG_DIR"),
            OsString::from("/host/global/mise"),
        ),
        (
            OsString::from("MISE_SYSTEM_CONFIG_DIR"),
            OsString::from("/host/system/mise"),
        ),
        (
            OsString::from("MISE_CONFIG_FILE"),
            OsString::from("/host/secret.toml"),
        ),
        (OsString::from("MISE_ENV"), OsString::from("host-profile")),
        (
            OsString::from("XDG_CACHE_HOME"),
            temporary.path().join("xdg cache").into_os_string(),
        ),
        (
            OsString::from("XDG_STATE_HOME"),
            temporary.path().join("xdg state").into_os_string(),
        ),
        (
            OsString::from("MISE_CACHE_DIR"),
            OsString::from("/tmp/attacker-cache"),
        ),
        (
            OsString::from("MBX_CACHE_DIR"),
            temporary.path().join("mbx cache").into_os_string(),
        ),
        (
            OsString::from("MBX_SESSION_PATH"),
            temporary.path().join("session file").into_os_string(),
        ),
        (OsString::from("BASH_ENV"), OsString::from("/host/bash-env")),
        (OsString::from("CARGO_ALIAS_POISON"), OsString::from("true")),
        (
            OsString::from("CARGO_BUILD_RUSTFLAGS"),
            OsString::from("--cfg hostile_build_flags"),
        ),
        (
            OsString::from("CARGO_TARGET_DIR"),
            OsString::from("/host/target"),
        ),
        (OsString::from("RUSTFLAGS"), OsString::from("--cfg hostile")),
        (OsString::from("RUSTC_BOOTSTRAP"), OsString::from("1")),
        (
            OsString::from("LD_PRELOAD"),
            OsString::from("/host/loader.dylib"),
        ),
        (
            OsString::from("DYLD_INSERT_LIBRARIES"),
            OsString::from("/host/injected.dylib"),
        ),
        (
            OsString::from("GIT_CONFIG_PARAMETERS"),
            OsString::from("poison"),
        ),
        (OsString::from("HK_LOG_FORMAT"), OsString::from("json")),
        (OsString::from("MBX_GC_AUTO"), OsString::from("1")),
    ]);
    let inputs = HostInputs::from_environment(
        &inherited,
        temporary.path(),
        Some(temporary.path().join("passwd home").into_os_string()),
        &root,
        &isolation,
    )
    .expect("admitted host inputs");

    let discovery = inputs.command_environment(&root);
    let provisioning = inputs.command_environment(&root);
    let gate_path = OsString::from(format!("/private/tools:{FIXED_SYSTEM_PATH}"));
    let gate = inputs.environment_for(&root, &gate_path);
    let fixed_path = OsString::from(FIXED_SYSTEM_PATH);
    for (environment, expected_path) in [
        (&discovery, &fixed_path),
        (&provisioning, &fixed_path),
        (&gate, &gate_path),
    ] {
        assert_admitted_host_environment(
            environment,
            expected_path,
            &root,
            &custom_data,
            temporary.path(),
            &isolation,
        );
    }
}

#[test]
fn managed_mise_projection_rejects_rustc_bootstrap_injected_by_mise() {
    let temporary = tempfile::tempdir().expect("temporary root");
    let root = temporary.path().join("reviewed");
    fs::create_dir(&root).expect("reviewed root");
    let fake_mise = temporary.path().join("fake-mise");
    write_executable(
        &fake_mise,
        b"#!/bin/sh\nset -eu\n[ \"$1\" = exec ]\n[ \"$2\" = --locked ]\n[ \"$3\" = -- ]\nshift 3\nexport RUSTC_BOOTSTRAP=1\nexec \"$@\"\n",
    );
    let environment = BTreeMap::new();
    let controls = SetupControls::for_test(std::time::Duration::from_secs(10));

    let error = verify_managed_projection(&fake_mise, &environment, &root, &controls)
        .expect_err("Mise must not reintroduce RUSTC_BOOTSTRAP");
    assert!(format!("{error:#}").contains("reintroduces RUSTC_BOOTSTRAP"));
}

fn assert_admitted_host_environment(
    environment: &BTreeMap<OsString, OsString>,
    expected_path: &OsString,
    root: &Path,
    custom_data: &Path,
    temporary: &Path,
    isolation: &Path,
) {
    let mise_keys = environment
        .keys()
        .filter(|key| key.to_string_lossy().starts_with("MISE_"))
        .cloned()
        .collect::<BTreeSet<_>>();
    assert_eq!(
        mise_keys,
        BTreeSet::from([
            OsString::from("MISE_AUTO_INSTALL"),
            OsString::from("MISE_CACHE_DIR"),
            OsString::from("MISE_CONFIG_DIR"),
            OsString::from("MISE_DATA_DIR"),
            OsString::from("MISE_EXEC_AUTO_INSTALL"),
            OsString::from("MISE_INSTALLS_DIR"),
            OsString::from("MISE_STATE_DIR"),
            OsString::from("MISE_SYSTEM_CONFIG_DIR"),
            OsString::from("MISE_TRUSTED_CONFIG_PATHS"),
        ])
    );
    assert_eq!(
        environment.get(OsStr::new("MISE_DATA_DIR")),
        Some(&physical_path(custom_data, temporary).into_os_string())
    );
    assert_eq!(
        environment.get(OsStr::new("MISE_INSTALLS_DIR")),
        Some(
            &physical_path(custom_data, temporary)
                .join("installs")
                .into_os_string()
        )
    );
    assert_eq!(
        environment.get(OsStr::new("MISE_CACHE_DIR")),
        Some(&physical_path(Path::new("/tmp/attacker-cache"), temporary).into_os_string())
    );
    assert_eq!(
        environment.get(OsStr::new("MISE_STATE_DIR")),
        Some(
            &physical_path(&temporary.join("xdg state"), temporary)
                .join("mise")
                .into_os_string()
        )
    );
    assert_eq!(
        environment.get(OsStr::new("MISE_TRUSTED_CONFIG_PATHS")),
        Some(&root.as_os_str().to_owned())
    );
    assert_eq!(
        environment.get(OsStr::new("MISE_AUTO_INSTALL")),
        Some(&OsString::from("0"))
    );
    assert_eq!(
        environment.get(OsStr::new("MISE_EXEC_AUTO_INSTALL")),
        Some(&OsString::from("false"))
    );
    assert_eq!(
        environment.get(OsStr::new("MISE_CONFIG_DIR")),
        Some(
            &fs::canonicalize(isolation.join("mise-config"))
                .unwrap()
                .into_os_string()
        )
    );
    assert_eq!(
        environment.get(OsStr::new("MISE_SYSTEM_CONFIG_DIR")),
        Some(
            &fs::canonicalize(isolation.join("mise-system-config"))
                .unwrap()
                .into_os_string()
        )
    );
    assert_eq!(
        environment.get(OsStr::new("MBX_CACHE_DIR")),
        Some(&physical_path(&temporary.join("mbx cache"), temporary).into_os_string())
    );
    assert_eq!(
        environment.get(OsStr::new("MBX_SESSION_PATH")),
        Some(&physical_path(&temporary.join("session file"), temporary).into_os_string())
    );
    assert_eq!(
        environment.get(OsStr::new("MBX_CARGO_SHIM_MODE")),
        Some(&OsString::from("1"))
    );
    assert_eq!(
        environment.get(OsStr::new("CARGO_HOME")),
        Some(&physical_path(&temporary.join("cargo home"), temporary).into_os_string())
    );
    for key in [
        "BASH_ENV",
        "CARGO_ALIAS_POISON",
        "CARGO_BUILD_RUSTFLAGS",
        "CARGO_TARGET_DIR",
        "RUSTFLAGS",
        "RUSTC_BOOTSTRAP",
        "LD_PRELOAD",
        "DYLD_INSERT_LIBRARIES",
        "GIT_CONFIG_PARAMETERS",
        "MISE_CONFIG_FILE",
        "MISE_ENV",
        "MBX_GC_AUTO",
    ] {
        assert!(
            !environment.contains_key(OsStr::new(key)),
            "unexpected {key}"
        );
    }
    assert_eq!(environment.get(OsStr::new("PATH")), Some(expected_path));
}

#[test]
fn fake_mise_receives_private_global_and_system_config_roots_in_each_phase() {
    let temporary = tempfile::tempdir().expect("temporary root");
    let root = temporary.path().join("reviewed");
    let isolation = temporary.path().join("isolation");
    let custom_data = temporary.path().join("custom data root with spaces");
    let marker = temporary.path().join("mise phases");
    fs::create_dir_all(&root).expect("reviewed root");
    fs::create_dir(&isolation).expect("isolation root");
    let inherited = BTreeMap::from([
        (OsString::from("PATH"), OsString::from("/host/bin")),
        (
            OsString::from("HOME"),
            temporary.path().join("home").into_os_string(),
        ),
        (
            OsString::from("CARGO_HOME"),
            temporary.path().join("cargo home").into_os_string(),
        ),
        (
            OsString::from("MISE_DATA_DIR"),
            custom_data.clone().into_os_string(),
        ),
        (
            OsString::from("MISE_CONFIG_DIR"),
            OsString::from("/host/mise/global"),
        ),
        (
            OsString::from("MISE_SYSTEM_CONFIG_DIR"),
            OsString::from("/host/mise/system"),
        ),
        (
            OsString::from("MISE_GLOBAL_CONFIG_FILE"),
            OsString::from("/host/global.toml"),
        ),
        (
            OsString::from("XDG_CACHE_HOME"),
            temporary.path().join("cache with spaces").into_os_string(),
        ),
    ]);
    let inputs =
        HostInputs::from_environment(&inherited, temporary.path(), None, &root, &isolation)
            .expect("admit fake Mise inputs");
    let fake_mise = temporary.path().join("fake-mise");
    let script = format!(
        "#!/bin/sh\nset -eu\ntest \"$MISE_CONFIG_DIR\" = '{}'\ntest \"$MISE_SYSTEM_CONFIG_DIR\" = '{}'\ntest \"$MISE_DATA_DIR\" = '{}'\ntest \"$MISE_INSTALLS_DIR\" = '{}/installs'\ntest \"$MISE_CACHE_DIR\" = '{}/mise'\ntest \"$MISE_AUTO_INSTALL\" = 0\ntest \"$MISE_EXEC_AUTO_INSTALL\" = false\ntest \"${{MISE_GLOBAL_CONFIG_FILE+x}}\" != x\ntest \"${{MISE_CONFIG_FILE+x}}\" != x\ntest -z \"$(/bin/ls -A \"$MISE_CONFIG_DIR\")\"\ntest -z \"$(/bin/ls -A \"$MISE_SYSTEM_CONFIG_DIR\")\"\nprintf x >> '{}'\n",
        fs::canonicalize(isolation.join("mise-config"))
            .unwrap()
            .display(),
        fs::canonicalize(isolation.join("mise-system-config"))
            .unwrap()
            .display(),
        physical_path(&custom_data, temporary.path()).display(),
        physical_path(&custom_data, temporary.path()).display(),
        physical_path(
            &temporary.path().join("cache with spaces"),
            temporary.path()
        )
        .display(),
        marker.display(),
    );
    write_executable(&fake_mise, script.as_bytes());

    let discovery = inputs.command_environment(&root);
    let provisioning = inputs.command_environment(&root);
    let gate = inputs.environment_for(&root, OsStr::new(FIXED_SYSTEM_PATH));
    for environment in [&discovery, &provisioning, &gate] {
        let status = Command::new(&fake_mise)
            .env_clear()
            .envs(environment)
            .status()
            .expect("run fake Mise phase probe");
        assert!(status.success(), "fake Mise rejected admitted environment");
    }
    assert_eq!(fs::read(&marker).expect("phase marker"), b"xxx");
}

#[test]
fn actual_bash_ignores_hostile_bash_env_and_inherited_path_wrappers() {
    let temporary = tempfile::tempdir().expect("temporary root");
    let root = temporary.path().join("reviewed");
    let isolation = temporary.path().join("isolation");
    let wrapper_dir = temporary.path().join("host wrappers");
    fs::create_dir_all(&root).expect("reviewed root");
    fs::create_dir(&isolation).expect("isolation root");
    fs::create_dir(&wrapper_dir).expect("host wrappers");
    let bash_env = temporary.path().join("hostile bash env");
    let marker = temporary.path().join("hostile marker");
    fs::write(
        &bash_env,
        format!("#!/bin/sh\nprintf ran > '{}'\nexit 0\n", marker.display()),
    )
    .expect("hostile BASH_ENV");
    let inherited = BTreeMap::from([
        (OsString::from("PATH"), wrapper_dir.as_os_str().to_owned()),
        (OsString::from("HOME"), root.join("home").into_os_string()),
        (
            OsString::from("CARGO_HOME"),
            root.join("cargo home").into_os_string(),
        ),
        (OsString::from("BASH_ENV"), bash_env.into_os_string()),
    ]);
    for name in ["bash", "find", "xargs"] {
        let wrapper = wrapper_dir.join(name);
        write_executable(
            &wrapper,
            format!("#!/bin/sh\nprintf ran > '{}'\nexit 0\n", marker.display()).as_bytes(),
        );
    }
    let inputs =
        HostInputs::from_environment(&inherited, temporary.path(), None, &root, &isolation)
            .expect("admit safe fixture environment");
    let environment = inputs.command_environment(&root);
    let status = Command::new("/bin/bash")
        .args([
            "-c",
            "bash -c ':' && find / -maxdepth 0 >/dev/null && printf x | xargs cat >/dev/null; exit 23",
        ])
        .current_dir(&root)
        .env_clear()
        .envs(&environment)
        .status()
        .expect("run actual system bash");
    assert_eq!(status.code(), Some(23));
    assert!(!marker.exists(), "host shell setup or PATH wrappers ran");
}

#[test]
fn cargo_config_admission_allows_missing_or_empty_external_files_and_preserves_credentials() {
    let temporary = tempfile::tempdir().expect("temporary root");
    let parent = temporary.path();
    let root = parent.join("workspace/reviewed");
    let isolation = parent.join("isolation");
    let cargo_home = parent.join("shared cargo home");
    fs::create_dir_all(&root).expect("reviewed root");
    fs::create_dir(&isolation).expect("isolation root");
    fs::create_dir(&cargo_home).expect("shared Cargo home");
    let credentials = cargo_home.join("credentials.toml");
    fs::write(&credentials, b"token = 'sentinel-do-not-read-or-mutate'\n")
        .expect("credentials fixture");
    let inherited = BTreeMap::from([
        (OsString::from("PATH"), OsString::from("/host/bin")),
        (OsString::from("HOME"), parent.join("home").into_os_string()),
        (
            OsString::from("CARGO_HOME"),
            cargo_home.clone().into_os_string(),
        ),
    ]);
    let missing = HostInputs::from_environment(&inherited, parent, None, &root, &isolation)
        .expect("missing external Cargo config files are allowed");
    assert_eq!(missing.cargo_home(), physical_path(&cargo_home, parent));

    let ancestor_config = parent.join(".cargo/config");
    fs::create_dir_all(ancestor_config.parent().unwrap()).expect("external Cargo config dir");
    fs::write(&ancestor_config, b"# deliberately empty\n\n").expect("empty ancestor config");
    fs::write(
        cargo_home.join("config.toml"),
        "# empty global configuration\n",
    )
    .expect("empty global config");
    let empty = HostInputs::from_environment(&inherited, parent, None, &root, &isolation)
        .expect("empty external Cargo config files are allowed");
    assert_eq!(empty.cargo_home(), physical_path(&cargo_home, parent));
    assert_eq!(
        fs::read(credentials).expect("credentials remain unchanged"),
        b"token = 'sentinel-do-not-read-or-mutate'\n"
    );

    let staged_config_dir = root.join(".cargo");
    fs::create_dir(&staged_config_dir).expect("reviewed Cargo config dir");
    fs::write(staged_config_dir.join("config.toml"), "[build]\njobs = 1\n")
        .expect("reviewed staged Cargo config");
    let nested_workspace = root.join("fuzz/jackin-env");
    fs::create_dir_all(nested_workspace.join(".cargo")).expect("nested reviewed Cargo config dir");
    fs::write(nested_workspace.join("Cargo.toml"), "[workspace]\n")
        .expect("nested workspace manifest");
    fs::write(
        nested_workspace.join(".cargo/config.toml"),
        "include = ['local.toml']\n",
    )
    .expect("nested reviewed Cargo config with contained include");
    fs::write(
        nested_workspace.join(".cargo/local.toml"),
        "[build]\njobs = 1\n",
    )
    .expect("contained nested Cargo include");
    HostInputs::from_environment(&inherited, parent, None, &root, &isolation)
        .expect("reviewed root and nested Cargo configs with contained includes are admitted");

    let home_fallback = BTreeMap::from([
        (OsString::from("PATH"), OsString::from("/host/bin")),
        (
            OsString::from("HOME"),
            parent.join("fallback home").into_os_string(),
        ),
    ]);
    let fallback = HostInputs::from_environment(&home_fallback, parent, None, &root, &isolation)
        .expect("HOME fallback for effective Cargo home");
    assert_eq!(
        fallback.cargo_home(),
        physical_path(&parent.join("fallback home/.cargo"), parent)
    );

    let passwd_fallback = BTreeMap::from([(OsString::from("PATH"), OsString::from("/host/bin"))]);
    let passwd_home = parent.join("real uid home");
    let passwd_selected = HostInputs::from_environment(
        &passwd_fallback,
        parent,
        Some(passwd_home.clone().into_os_string()),
        &root,
        &isolation,
    )
    .expect("real UID passwd home fallback");
    assert_eq!(
        passwd_selected.cargo_home(),
        physical_path(&passwd_home.join(".cargo"), parent)
    );
    assert_eq!(
        passwd_selected.home.as_deref(),
        Some(physical_path(&passwd_home, parent).as_os_str())
    );
}

#[test]
fn nonempty_external_cargo_configs_fail_without_exposing_contents() {
    let temporary = tempfile::tempdir().expect("temporary root");
    let parent = temporary.path();
    let root = parent.join("workspace/reviewed");
    let isolation = parent.join("isolation");
    let cargo_home = parent.join("cargo home");
    fs::create_dir_all(&root).expect("reviewed root");
    fs::create_dir(&isolation).expect("isolation root");
    fs::create_dir(&cargo_home).expect("Cargo home");
    let credentials = cargo_home.join("credentials.toml");
    fs::write(&credentials, b"token = 'private-sentinel'\n").expect("credential fixture");
    let global_config = cargo_home.join("config.toml");
    let inherited = BTreeMap::from([
        (OsString::from("PATH"), OsString::from("/host/bin")),
        (OsString::from("HOME"), parent.join("home").into_os_string()),
        (OsString::from("CARGO_HOME"), cargo_home.into_os_string()),
    ]);
    for config in [
        "[build]\nrustflags = ['--cfg secret']\n",
        "[alias]\nsecret = 'build'\n",
        "[env]\nSECRET = { value = 'private-sentinel', force = true }\n",
        "include = []\n",
        "unknown_host_setting = 'private-sentinel'\n",
    ] {
        fs::write(&global_config, config).expect("write hostile external Cargo config");
        let Err(error) = HostInputs::from_environment(&inherited, parent, None, &root, &isolation)
        else {
            panic!("nonempty external Cargo config must be rejected");
        };
        let message = format!("{error:#}");
        assert!(message.contains(&global_config.display().to_string()));
        assert!(!message.contains("private-sentinel"));
        let credentials_after = fs::read(&credentials).expect("read credential sentinel");
        assert_eq!(credentials_after, b"token = 'private-sentinel'\n");
    }

    let external_config = parent.join("external config.toml");
    fs::write(&external_config, "[build]\njobs = 1\n").expect("external target");
    let staged_config_dir = root.join(".cargo");
    fs::create_dir(&staged_config_dir).expect("staged Cargo config dir");
    symlink(&external_config, staged_config_dir.join("config.toml"))
        .expect("escaping staged config symlink");
    let Err(error) = HostInputs::from_environment(&inherited, parent, None, &root, &isolation)
    else {
        panic!("staged Cargo config symlink must not escape reviewed root");
    };
    assert!(format!("{error:#}").contains("escapes the reviewed snapshot"));

    let staged_config = staged_config_dir.join("config.toml");
    fs::remove_file(&staged_config).expect("remove staged config symlink");
    fs::write(
        &staged_config,
        "include = ['../../../external config.toml']\n",
    )
    .expect("staged Cargo include escaping reviewed root");
    let Err(error) = HostInputs::from_environment(&inherited, parent, None, &root, &isolation)
    else {
        panic!("staged Cargo config include must not escape reviewed root");
    };
    assert!(format!("{error:#}").contains("escapes the reviewed snapshot"));
}

#[test]
fn hostile_external_and_nested_cargo_configs_fail_before_mise_or_rustup_child_probes() {
    const CHILD_MARKER: &str = "JACKIN_EXTERNAL_CARGO_CONFIG_PROBE_CHILD";
    const FIXTURE_ROOT: &str = "JACKIN_EXTERNAL_CARGO_CONFIG_PROBE_ROOT";
    const PROBE_MARKER: &str = "JACKIN_EXTERNAL_CARGO_CONFIG_PROBE_MARKER";
    const TEST_NAME: &str = "precommit_snapshot::toolchain::tests::hostile_external_and_nested_cargo_configs_fail_before_mise_or_rustup_child_probes";

    if env::var_os(CHILD_MARKER).is_some() {
        let root = PathBuf::from(env::var_os(FIXTURE_ROOT).expect("fixture root"));
        let cargo_home = PathBuf::from(env::var_os("CARGO_HOME").expect("Cargo home"));
        let config = cargo_home.join("config.toml");
        let marker = PathBuf::from(env::var_os(PROBE_MARKER).expect("probe marker"));
        for contents in [
            "[build]\nrustflags = ['--cfg secret']\n",
            "[alias]\nsecret = 'build'\n",
            "[env]\nSECRET = { value = 'private-sentinel', force = true }\n",
            "include = []\n",
            "unknown_host_setting = 'private-sentinel'\n",
        ] {
            fs::write(&config, contents).expect("hostile Cargo config");
            let Err(error) = admit_install_environment(&root) else {
                panic!("installer admission must fail on external Cargo config");
            };
            let message = format!("{error:#}");
            assert!(message.contains(&config.display().to_string()));
            assert!(!message.contains("private-sentinel"));
            assert!(!marker.exists(), "installer admission ran a child probe");

            let Err(error) = verify_provision_tool_paths(
                &root,
                Path::new("/unused/cargo"),
                Path::new("/unused/rustc"),
            ) else {
                panic!("external Cargo config must fail admission");
            };
            let message = format!("{error:#}");
            assert!(message.contains(&config.display().to_string()));
            assert!(!message.contains("private-sentinel"));
            assert!(!marker.exists(), "Mise or rustup child probe ran");
        }

        fs::write(&config, "# empty after the global-config cases\n")
            .expect("clear external Cargo config before nested-workspace case");
        assert_nested_config_rejected_before_child_probes(
            &root,
            &marker,
            "fuzz/jackin-env",
            "config",
            "host.toml",
        );
        fs::remove_file(root.join("fuzz/jackin-env/.cargo/config"))
            .expect("remove earlier nested workspace config");
        assert_nested_config_rejected_before_child_probes(
            &root,
            &marker,
            "target/nested",
            "config.toml",
            "host-target.toml",
        );
        return;
    }

    let temporary = tempfile::tempdir().expect("temporary root");
    let parent = temporary.path();
    let root = parent.join("reviewed");
    let cargo_home = parent.join("shared cargo home");
    let home = parent.join("home");
    let temp = parent.join("temp");
    let fake_bin = parent.join("fake tool bin");
    let marker = parent.join("probe marker");
    for directory in [&root, &cargo_home, &home, &temp, &fake_bin] {
        fs::create_dir(directory).expect("probe fixture directory");
    }
    fs::write(
        root.join("rust-toolchain.toml"),
        "[toolchain]\nchannel = '1.99.1'\n",
    )
    .expect("exact Rust pin");
    let script = format!(
        "#!/bin/sh\nprintf probe >> '{}'\nexit 71\n",
        marker.display()
    );
    for name in ["mise", "rustup"] {
        write_executable(&fake_bin.join(name), script.as_bytes());
    }

    let status = Command::new(env::current_exe().expect("current test executable"))
        .args(["--exact", TEST_NAME, "--nocapture"])
        .current_dir(&root)
        .env_clear()
        .env(CHILD_MARKER, "1")
        .env(FIXTURE_ROOT, &root)
        .env(PROBE_MARKER, &marker)
        .env("PATH", &fake_bin)
        .env("HOME", &home)
        .env("CARGO_HOME", &cargo_home)
        .env("TMPDIR", &temp)
        .status()
        .expect("run isolated hostile-config child test");
    assert!(status.success(), "hostile-config child failed: {status}");
    assert!(!marker.exists(), "Mise or rustup child probe ran");
}

fn assert_nested_config_rejected_before_child_probes(
    root: &Path,
    marker: &Path,
    workspace: &str,
    config_name: &str,
    host_config: &str,
) {
    let nested_workspace = root.join(workspace);
    fs::create_dir_all(nested_workspace.join(".cargo"))
        .expect("nested workspace Cargo config directory");
    fs::write(nested_workspace.join("Cargo.toml"), "[workspace]\n")
        .expect("nested workspace manifest");
    fs::write(
        root.parent().expect("fixture parent").join(host_config),
        "[env]\nJACKIN_NESTED_CONFIG_SENTINEL = 'must not load'\n",
    )
    .expect("external nested Cargo include");
    let nested_config = nested_workspace.join(".cargo").join(config_name);
    fs::write(
        &nested_config,
        format!("include = ['../../../../{host_config}']\n"),
    )
    .expect("nested Cargo config with escaping include");

    let Err(error) = admit_install_environment(root) else {
        panic!("nested external Cargo include must be rejected");
    };
    assert_nested_config_error(&error, host_config);
    assert!(!marker.exists(), "nested Cargo config ran a child probe");

    let Err(error) =
        verify_provision_tool_paths(root, Path::new("/unused/cargo"), Path::new("/unused/rustc"))
    else {
        panic!("nested external Cargo include must fail admission");
    };
    assert_nested_config_error(&error, host_config);
    assert!(!marker.exists(), "Mise or rustup child probe ran");
}

fn assert_nested_config_error(error: &anyhow::Error, host_config: &str) {
    let message = format!("{error:#}");
    assert!(message.contains("staged Cargo config path escapes the reviewed snapshot"));
    assert!(message.contains(host_config));
    assert!(!message.contains("must not load"));
}

#[test]
fn path_output_removes_only_one_terminal_protocol_lf() {
    let output = command_result(0, b"/tmp/path \xff with spaces\nembedded\n\n");
    let path = command_path(output, "reading path").expect("raw path record");
    assert_eq!(
        path.as_os_str().as_bytes(),
        b"/tmp/path \xff with spaces\nembedded\n"
    );
}

#[test]
fn path_output_rejects_missing_terminator_and_nul() {
    for (bytes, reason) in [
        (
            b"/tmp/path".to_vec(),
            "path record is missing its LF terminator",
        ),
        (b"/tmp\0/path\n".to_vec(), "path record contains NUL"),
        (b"\n".to_vec(), "empty path record"),
    ] {
        let output = command_result(0, &bytes);
        command_path(output, "reading path").expect_err(reason);
    }

    let error = command_path(command_result(7, b"/repo\n"), "reading path")
        .expect_err("nonzero command must fail before path decoding");
    assert!(error.to_string().contains("failed with exit status: 7"));
    verify::command_output(command_result(0, b"/repo/\xff\n"), "fixture text")
        .expect_err("text output must reject non-UTF-8 bytes");
}

#[test]
fn cargo_path_selects_the_qualified_mbx_wrapper_before_rustup_cargo() {
    let temporary = tempfile::tempdir().unwrap();
    let managed = temporary.path().join("managed");
    let rust_bin = temporary.path().join("rust-bin");
    fs::create_dir(&managed).unwrap();
    fs::create_dir(&rust_bin).unwrap();
    let mbx_cargo = temporary.path().join("mbx-cargo");
    write_executable(&mbx_cargo, b"MBX wrapper");
    write_executable(&rust_bin.join("cargo"), b"rustup cargo");
    symlink(&mbx_cargo, managed.join("cargo")).unwrap();
    let path = env::join_paths([&managed, &rust_bin]).unwrap();
    let selected = find_executable("cargo", &path).unwrap();
    assert_eq!(
        fs::canonicalize(selected).unwrap(),
        fs::canonicalize(mbx_cargo).unwrap()
    );
}

#[test]
fn pinned_rustup_resolution_preserves_only_the_admitted_rustup_home() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let rustup_home = temporary.path().join("custom rustup home");
    let toolchain_bin = rustup_home.join("toolchains/fixture-pin/bin");
    fs::create_dir_all(&toolchain_bin).expect("custom rustup toolchain directory");
    for name in ["rustc", "rustdoc", "clippy-driver", "rustfmt"] {
        write_executable(&toolchain_bin.join(name), b"fixture tool");
    }
    let rustup = temporary.path().join("rustup");
    let rustup_script = concat!(
        "#!/bin/sh\n",
        "set -eu\n",
        "if [ -n \"${RUSTUP_HOME:-}\" ]; then\n",
        "  [ \"${HOME+x}\" != x ] || exit 71\n",
        "else\n",
        "  [ -n \"${HOME:-}\" ] || exit 70\n",
        "  RUSTUP_HOME=\"$HOME/.rustup\"\n",
        "fi\n",
        "[ \"${RUSTUP_TOOLCHAIN+x}\" != x ] || exit 72\n",
        "[ \"$1\" = which ] && [ \"$2\" = --toolchain ] && [ \"$3\" = fixture-pin ] || exit 73\n",
        "path=\"$RUSTUP_HOME/toolchains/$3/bin/$4\"\n",
        "[ -f \"$path\" ] || exit 74\n",
        "printf '%s\\n' \"$path\"\n",
    );
    write_executable(&rustup, rustup_script.as_bytes());

    let resolved = resolve_rust_tools(
        &rustup,
        "fixture-pin",
        OsStr::new("/inherited/path"),
        Some(rustup_home.as_os_str()),
        Some(OsStr::new("/unselected/home")),
    )
    .expect("resolve tools from the admitted custom rustup root");

    assert_eq!(resolved.len(), 4);
    for name in ["rustc", "rustdoc", "clippy-driver", "rustfmt"] {
        assert_eq!(
            resolved[name],
            fs::canonicalize(toolchain_bin.join(name)).expect("physical fixture tool")
        );
    }
    let home = temporary.path().join("default rustup home");
    fs::create_dir(&home).expect("custom HOME");
    symlink(&rustup_home, home.join(".rustup")).expect("HOME-selected rustup root");
    let default_resolved = resolve_rust_tools(
        &rustup,
        "fixture-pin",
        OsStr::new("/inherited/path"),
        None,
        Some(home.as_os_str()),
    )
    .expect("resolve tools from the admitted HOME without RUSTUP_HOME");
    assert_eq!(default_resolved, resolved);
}

#[test]
fn pinned_swiftlint_precedes_an_ambient_global_tool() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let managed = temporary.path().join("managed");
    let rust_bin = temporary.path().join("rust-bin");
    let ambient = temporary.path().join("ambient");
    let pinned_bin = temporary.path().join("mise/installs/swiftlint/0.65.1/bin");
    for directory in [&managed, &rust_bin, &ambient, &pinned_bin] {
        fs::create_dir_all(directory).expect("tool directory");
    }
    let pinned = pinned_bin.join("swiftlint");
    write_executable(&pinned, b"Mise-pinned SwiftLint 0.65.1");
    let ambient_tool = ambient.join("swiftlint");
    write_executable(&ambient_tool, b"ambient SwiftLint");
    let resolved =
        resolve_tool_paths(&["swiftlint"], |_| Ok(pinned.clone())).expect("mock Mise resolution");
    symlink(&resolved["swiftlint"], managed.join("swiftlint")).expect("managed SwiftLint link");
    let path = qualified_path(&managed, &rust_bin).expect("qualified PATH");
    let selected = find_executable("swiftlint", &path).expect("qualified SwiftLint");

    assert_eq!(
        fs::canonicalize(selected).expect("selected SwiftLint"),
        fs::canonicalize(pinned).expect("physical pinned SwiftLint")
    );
}

#[test]
fn local_guard_in_home_is_admitted_and_qualified_without_inherited_path_entry() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let home = temporary.path().join("home");
    let local_bin = home.join(".local/bin");
    let inherited_bin = temporary.path().join("inherited");
    let managed = temporary.path().join("managed");
    let rust_bin = temporary.path().join("rust-bin");
    for directory in [&local_bin, &inherited_bin, &managed, &rust_bin] {
        fs::create_dir_all(directory).expect("tool directory");
    }
    let guard = local_bin.join("rust-build-guard");
    write_executable(&guard, b"trusted guard");
    let inherited = env::join_paths([&inherited_bin]).expect("inherited PATH");

    let resolved = resolve_guard(&inherited, Some(&home)).expect("home guard fallback");
    assert_eq!(
        resolved,
        fs::canonicalize(&guard).expect("physical guard path")
    );
    symlink(&resolved, managed.join("rust-build-guard")).expect("managed guard link");
    let qualified = qualified_path(&managed, &rust_bin).expect("qualified PATH");
    let selected = find_executable("rust-build-guard", &qualified).expect("qualified guard");
    assert_eq!(
        fs::canonicalize(selected).expect("selected physical guard"),
        resolved
    );
}

#[test]
fn missing_local_guard_fails_closed() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let home = temporary.path().join("home");
    let inherited_bin = temporary.path().join("inherited");
    fs::create_dir_all(&home).expect("home directory");
    fs::create_dir_all(&inherited_bin).expect("inherited PATH directory");
    let inherited = env::join_paths([&inherited_bin]).expect("inherited PATH");

    let error = resolve_guard(&inherited, Some(&home)).expect_err("missing guard must fail");
    assert!(
        error
            .to_string()
            .contains("rust-build-guard is not installed")
    );
}

#[test]
fn inherited_path_guard_precedes_the_home_fallback() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let home = temporary.path().join("home");
    let home_bin = home.join(".local/bin");
    let inherited_bin = temporary.path().join("inherited");
    fs::create_dir_all(&home_bin).expect("home guard directory");
    fs::create_dir_all(&inherited_bin).expect("inherited guard directory");
    let home_guard = home_bin.join("rust-build-guard");
    let path_guard = inherited_bin.join("rust-build-guard");
    write_executable(&home_guard, b"home guard");
    write_executable(&path_guard, b"PATH guard");
    let inherited = env::join_paths([&inherited_bin]).expect("inherited PATH");

    let resolved = resolve_guard(&inherited, Some(&home)).expect("PATH guard");
    assert_eq!(
        resolved,
        fs::canonicalize(path_guard).expect("physical PATH guard")
    );
}

#[cfg(target_os = "macos")]
#[test]
fn missing_swiftlint_resolution_fails_before_resolving_hk() {
    assert_eq!(MANAGED_TOOLS.first(), Some(&"swiftlint"));
    let mut requested = Vec::new();
    let result = resolve_tool_paths(MANAGED_TOOLS, |name| {
        requested.push(name);
        if name == "swiftlint" {
            anyhow::bail!("Mise could not resolve pinned SwiftLint");
        }
        Ok(PathBuf::from(name))
    });

    assert!(
        result
            .expect_err("missing pinned SwiftLint must stop resolution")
            .to_string()
            .contains("Mise could not resolve pinned SwiftLint")
    );
    assert_eq!(requested, ["swiftlint"]);
}

#[cfg(target_os = "linux")]
#[test]
fn linux_managed_tools_do_not_require_macos_only_swiftlint() {
    assert!(!MANAGED_TOOLS.contains(&"swiftlint"));
}
