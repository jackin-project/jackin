use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};

use super::{
    Environment, cargo_home, cargo_home_path, command_tools, host, library_name, resolve, run_with,
    run_with_home, version,
};

#[test]
fn cargo_home_uses_explicit_home_or_real_uid_passwd_fallback() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let cwd = temp.path();
    let resolved_cwd = resolve(cwd, cwd).expect("resolved temporary directory");
    let mut inherited = Environment::new();
    let home = temp.path().join("user home");
    inherited.insert(OsString::from("HOME"), home.as_os_str().to_owned());
    let selected = cargo_home(&inherited, cwd, || {
        panic!("HOME should bypass passwd lookup")
    })
    .expect("HOME-based Cargo home");
    assert_eq!(selected, resolved_cwd.join("user home/.cargo"));

    inherited.insert(OsString::from("HOME"), OsString::new());
    let empty_home = cargo_home_path(&inherited, || {
        panic!("empty HOME should bypass passwd lookup")
    })
    .expect("empty HOME Cargo home");
    assert_eq!(empty_home, PathBuf::from("/.cargo"));

    inherited.remove(OsStr::new("HOME"));
    let passwd_home = temp.path().join("passwd home");
    let mut lookups = 0;
    let selected = cargo_home(&inherited, cwd, || {
        lookups += 1;
        Ok(Some(passwd_home.as_os_str().to_owned()))
    })
    .expect("passwd-based Cargo home");
    assert_eq!(lookups, 1);
    assert_eq!(selected, resolved_cwd.join("passwd home/.cargo"));

    let empty_passwd_home = cargo_home_path(&inherited, || Ok(Some(OsString::new())))
        .expect("empty passwd home follows POSIX expanduser behavior");
    assert_eq!(empty_passwd_home, PathBuf::from("/.cargo"));

    inherited.insert(
        OsString::from("CARGO_HOME"),
        OsString::from("explicit cargo home"),
    );
    let explicit = cargo_home(&inherited, cwd, || {
        panic!("CARGO_HOME should bypass passwd lookup")
    })
    .expect("explicit Cargo home");
    assert_eq!(explicit, resolved_cwd.join("explicit cargo home"));

    inherited.remove(OsStr::new("CARGO_HOME"));
    let missing = cargo_home(&inherited, cwd, || Ok(None)).expect_err("missing passwd account");
    assert!(missing.to_string().contains("no passwd entry"));

    let lookup_error = cargo_home(&inherited, cwd, || {
        Err(anyhow::anyhow!("simulated passwd lookup failure"))
    })
    .expect_err("passwd lookup failure");
    assert!(format!("{lookup_error:#}").contains("simulated passwd lookup failure"));
}

#[test]
fn missing_passwd_home_stops_before_the_first_child_command() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let tools = temp.path().join("tools");
    command_tools(&tools);
    let mut inherited = BTreeMap::new();
    inherited.insert(OsString::from("PATH"), tools.as_os_str().to_owned());

    let mut commands = 0;
    let error = run_with_home(
        temp.path(),
        inherited.clone(),
        || Ok(None),
        |_, _, _, _| {
            commands += 1;
            Ok(Vec::new())
        },
    )
    .expect_err("missing passwd home must stop setup");

    assert!(error.to_string().contains("no passwd entry"));
    assert_eq!(commands, 0);

    let error = run_with_home(
        temp.path(),
        inherited,
        || Err(anyhow::anyhow!("simulated passwd lookup failure")),
        |_, _, _, _| {
            commands += 1;
            Ok(Vec::new())
        },
    )
    .expect_err("passwd lookup failure must stop setup");

    assert!(format!("{error:#}").contains("simulated passwd lookup failure"));
    assert_eq!(commands, 0);
}

#[cfg(unix)]
#[test]
fn cargo_paths_follow_symlinks_before_parent_and_keep_missing_tail_behavior() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("temporary directory");
    let cwd = temp.path();
    let resolved_cwd = resolve(cwd, cwd).expect("resolved temporary directory");
    fs::create_dir_all(cwd.join("nested/real")).expect("nested real directory");
    symlink("nested/real", cwd.join("alias")).expect("nested directory alias");

    let expected = resolved_cwd.join("nested/target");
    let direct = resolve(Path::new("alias/../target"), cwd).expect("direct alias path");
    assert_eq!(direct, expected);

    let missing_tail =
        resolve(&cwd.join("alias/missing/../../target"), cwd).expect("missing-tail path");
    assert_eq!(missing_tail, expected);
    assert!(!expected.exists(), "resolving a path must not create it");

    let mut inherited = Environment::new();
    inherited.insert(
        OsString::from("CARGO_HOME"),
        cwd.join("alias/../cargo-home").into_os_string(),
    );
    let selected = cargo_home(&inherited, cwd, || {
        panic!("explicit home should bypass lookup")
    })
    .expect("resolved CARGO_HOME");
    assert_eq!(selected, resolved_cwd.join("nested/cargo-home"));
}

#[cfg(unix)]
#[test]
fn non_strict_paths_keep_dangling_loop_and_not_directory_spellings() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("temporary directory");
    let cwd = temp.path();
    let resolved_cwd = resolve(cwd, cwd).expect("resolved temporary directory");
    symlink("absent", cwd.join("dangling")).expect("dangling link");
    assert_eq!(
        resolve(&cwd.join("dangling/tail"), cwd).expect("dangling target path"),
        resolved_cwd.join("absent/tail")
    );

    symlink("loop-b", cwd.join("loop-a")).expect("first loop link");
    symlink("loop-a", cwd.join("loop-b")).expect("second loop link");
    assert_eq!(
        resolve(&cwd.join("loop-a/tail"), cwd).expect("loop remains unresolved"),
        resolved_cwd.join("loop-a/tail")
    );

    fs::write(cwd.join("regular-file"), b"file").expect("regular file");
    assert_eq!(
        resolve(&cwd.join("regular-file/child"), cwd).expect("ENOTDIR remains unresolved"),
        resolved_cwd.join("regular-file/child")
    );
}

#[cfg(unix)]
#[test]
fn pipeline_keeps_target_binding_after_target_alias_is_repointed() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("temporary directory");
    let repository = temp.path().join("repository");
    fs::create_dir_all(repository.join("crates/tools/jackin-lints")).expect("lint crate directory");
    let tools = temp.path().join("pinned tools");
    command_tools(&tools);

    let target_root = temp.path().join("target paths");
    let resolved_target_root = resolve(&target_root, temp.path()).expect("resolved target root");
    fs::create_dir_all(target_root.join("nested/real")).expect("original target referent");
    fs::create_dir_all(target_root.join("alternate/real")).expect("alternate referent");
    let alias = target_root.join("alias");
    symlink("nested/real", &alias).expect("original target alias");
    let original_target = resolved_target_root.join("nested/target");
    let redirected_target = resolved_target_root.join("alternate/target");
    let selected_host = host();
    let cargo_dylint = fs::canonicalize(tools.join("cargo-dylint"))
        .expect("selected cargo-dylint")
        .into_os_string();

    let mut inherited = Environment::new();
    inherited.insert(OsString::from("PATH"), tools.as_os_str().to_owned());
    inherited.insert(OsString::from("HOME"), temp.path().as_os_str().to_owned());
    inherited.insert(
        OsString::from("CARGO_TARGET_DIR"),
        target_root.join("alias/../target").into_os_string(),
    );

    let mut calls = Vec::new();
    let mut alias_repointed = false;
    run_with(&repository, inherited, |argv, cwd, env, capture| {
        if !alias_repointed {
            fs::remove_file(&alias).expect("remove original target alias");
            symlink("alternate/real", &alias).expect("repoint target alias");
            alias_repointed = true;
        }
        calls.push((argv.to_vec(), cwd.to_path_buf(), env.clone(), capture));

        if argv.ends_with(&[OsString::from("rustc"), OsString::from("-Vv")]) {
            return Ok(version(&selected_host).into_bytes());
        }
        if argv.ends_with(&[OsString::from("--installed")]) {
            let installed =
                super::super::COMPONENTS
                    .iter()
                    .fold(String::new(), |mut output, component| {
                        output.push_str(component);
                        output.push('-');
                        output.push_str(&selected_host);
                        output.push('\n');
                        output
                    });
            return Ok(installed.into_bytes());
        }
        if argv.ends_with(&[OsString::from("build"), OsString::from("--locked")]) {
            let library = original_target
                .join("library/debug")
                .join(library_name(&selected_host));
            fs::create_dir_all(library.parent().expect("library parent"))
                .expect("library directory");
            fs::write(library, b"fresh qualified library").expect("built library");
        }
        if argv.first() == Some(&cargo_dylint) {
            let library_dir = PathBuf::from(
                env.get(OsStr::new("DYLINT_LIBRARY_PATH"))
                    .expect("isolated library path"),
            );
            assert_eq!(
                fs::read(library_dir.join(library_name(&selected_host)))
                    .expect("qualified library copy"),
                b"fresh qualified library"
            );
            assert_eq!(cwd, repository);
            assert_eq!(
                env.get(OsStr::new("CARGO_TARGET_DIR")),
                Some(&original_target.join("workspace").into_os_string())
            );
        }
        Ok(Vec::new())
    })
    .expect("fake custom-lint pipeline");

    assert!(alias_repointed);
    assert_eq!(calls.len(), 5);
    for (_, _, env, _) in calls.iter().take(4) {
        assert_eq!(
            env.get(OsStr::new("CARGO_TARGET_DIR")),
            Some(&original_target.join("library").into_os_string())
        );
    }
    assert!(
        !redirected_target.join("library").exists(),
        "the retargeted alias must not redirect the already-bound target"
    );
}
