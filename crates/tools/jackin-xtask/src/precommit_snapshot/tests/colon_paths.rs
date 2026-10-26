use super::*;
use std::os::unix::fs::symlink;

#[test]
fn installed_owner_runs_qualified_tools_when_tmpdir_and_rust_path_contain_colons() {
    let mut fixture = InstalledOwnerFixture::new(OwnerFixtureMode::IsolationGate);
    let support = fixture.repository.join(".git/f3-test");
    let requested_tmp = support.join("requested:temporary-root");
    let rust_bin = support.join("rust:toolchain/bin");
    fs::create_dir(&requested_tmp).expect("create colon-containing requested TMPDIR");
    fs::create_dir_all(&rust_bin).expect("create colon-containing Rust bin directory");
    fixture.tmpdir_override = Some(requested_tmp.clone());

    let rustc = rust_bin.join("rustc");
    let rustup = fixture.repository.join(".git/f3-test/bin/rustup");
    let tool_body = concat!(
        "#!/bin/sh\n",
        "name=${0##*/}\n",
        "case \"$name\" in\n",
        "  rustc) if [ -n \"${JACKIN_F3_OWNER_DRIVER_RUSTC_PROVIDER:-}\" ]; then printf 'fixture-pinned-rustc\\n' > \"$JACKIN_F3_OWNER_DRIVER_RUSTC_PROVIDER\"; fi; printf 'rustc 1.99.1 (fixture)\\nbinary: rustc\\ncommit-hash: 0123456789abcdef0123456789abcdef01234567\\nrelease: 1.99.1\\n' ;;\n",
        "  rustdoc) printf 'rustdoc 1.99.1 (fixture)\\nbinary: rustdoc\\ncommit-hash: 0123456789abcdef0123456789abcdef01234567\\nrelease: 1.99.1\\n' ;;\n",
        "  clippy-driver) printf 'clippy 0.1.99 (0123456789abcdef0123456789abcdef01234567 fixture)\\n' ;;\n",
        "  rustfmt) printf 'rustfmt 1.99.1-stable (0123456789abcdef0123456789abcdef01234567 fixture)\\n' ;;\n",
        "  *) exit 91 ;;\n",
        "esac\n",
    );
    for name in ["rustc", "rustdoc", "clippy-driver", "rustfmt"] {
        write_executable(&rust_bin.join(name), tool_body.as_bytes());
    }
    write_executable(
        &rustup,
        format!(
            "#!/bin/sh\n[ \"$1\" = which ] && [ \"$2\" = --toolchain ] && [ \"$3\" = 1.99.1 ] || exit 90\ncase \"$4\" in rustc|rustdoc|clippy-driver|rustfmt) printf '%s/%s\\n' {} \"$4\" ;; *) exit 91 ;; esac\n",
            shell_quote(&rust_bin)
        )
        .as_bytes(),
    );
    fixture.expected_rustc = fs::canonicalize(&rustc).expect("canonical colon Rust path");

    let before = capture_live_canary_state(&fixture.repository);
    let mut owner = fixture.spawn_owner("status", 0, 20_000, 20_000, 20_000);
    let release_gate = GateReleaseOnDrop(fixture.gate_release.clone());
    wait_for_owner_readiness(&mut owner, &fixture.gate_ready, &fixture.ticks);
    assert_eq!(
        capture_live_canary_state(&fixture.repository),
        before,
        "live repository stays unchanged while the private gate runs"
    );
    release_gate.release();
    let output = owner.wait_output(std::time::Duration::from_secs(8));
    assert!(
        output.status.success(),
        "colon-path owner failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert_eq!(
        fs::read(&fixture.cargo_provider).expect("qualified Cargo provider marker"),
        b"fixture-mbx-cargo\n"
    );
    assert_eq!(
        fs::read(&fixture.rustc_provider).expect("qualified rustc provider marker"),
        b"fixture-pinned-rustc\n"
    );
    let report = fs::read_to_string(&fixture.probe_output).expect("qualified tool report");
    let target = PathBuf::from(report.lines().next().expect("private target path"));
    assert!(
        target
            .parent()
            .is_some_and(|scratch| scratch.starts_with(&requested_tmp)),
        "private build scratch stays under requested TMPDIR: {target:?}"
    );
    assert!(
        target
            .to_string_lossy()
            .contains("requested:temporary-root"),
        "private build scratch preserves colon-containing TMPDIR: {target:?}"
    );
    let path = fs::read_to_string(&fixture.path_output).expect("qualified PATH record");
    let alias = env::split_paths(OsStr::new(path.trim_end_matches('\n')))
        .next()
        .expect("first qualified PATH entry");
    assert!(alias.is_absolute());
    assert!(!alias.as_os_str().as_encoded_bytes().contains(&b':'));
    assert!(
        !alias.exists(),
        "qualified PATH aliases are removed after gate"
    );
}

#[test]
fn installed_owner_anchors_empty_and_relative_path_entries_before_snapshot_chdir() {
    for component in [PathBuf::new(), PathBuf::from("relative-tools")] {
        let parent = tempfile::tempdir_in("/tmp").expect("owner fixture parent");
        assert!(
            !parent.path().as_os_str().as_encoded_bytes().contains(&b':'),
            "fixture PATH alias parent is delimiter-free"
        );
        let colon_cwd_parent = parent.path().join("hook:cwd");
        fs::create_dir(&colon_cwd_parent).expect("colon-containing hook directory");
        let mut fixture =
            InstalledOwnerFixture::new_in(OwnerFixtureMode::IsolationGate, &colon_cwd_parent);
        let bin = env::split_paths(&fixture.path)
            .next()
            .expect("fixture tool directory");
        let fixture_bin = fixture.repository.join(".git/f3-test/bin");
        symlink("/bin/mkdir", fixture_bin.join("mkdir"))
            .expect("provide fixed system mkdir utility");
        symlink("/usr/bin/touch", fixture_bin.join("touch"))
            .expect("provide fixed system touch utility");
        let provider_bin = parent.path().join("provider-bin");
        symlink(&bin, &provider_bin).expect("create delimiter-free provider path");
        let git_directory = if component.as_os_str().is_empty() {
            fixture.repository.clone()
        } else {
            fixture.repository.join(&component)
        };
        fs::create_dir_all(&git_directory).expect("create relative PATH directory");
        symlink(real_git_executable(), git_directory.join("git"))
            .expect("install Git only in selected relative PATH directory");
        fixture.path = env::join_paths([provider_bin.as_path(), component.as_path()])
            .expect("relative fixture PATH");

        let mut owner = fixture.spawn_owner("status", 0, 20_000, 20_000, 20_000);
        let release_gate = GateReleaseOnDrop(fixture.gate_release.clone());
        wait_for_owner_readiness(&mut owner, &fixture.gate_ready, &fixture.ticks);
        let path = fs::read_to_string(&fixture.path_output).expect("qualified PATH record");
        let entries = env::split_paths(OsStr::new(path.trim_end_matches('\n'))).collect::<Vec<_>>();
        assert_eq!(entries.len(), 3, "qualified PATH entries: {entries:?}");
        assert!(
            entries
                .iter()
                .all(|entry| !entry.as_os_str().as_encoded_bytes().contains(&b':')),
            "qualified PATH projection uses delimiter-free entries: {entries:?}"
        );
        assert_eq!(entries[1], provider_bin);
        assert_eq!(
            fs::read_link(&entries[2]).expect("anchored relative PATH alias"),
            if component.as_os_str().is_empty() {
                fixture.repository.clone()
            } else {
                fixture.repository.join(&component)
            }
        );
        let alias = entries[0].clone();
        release_gate.release();
        let output = owner.wait_output(std::time::Duration::from_secs(8));
        assert!(
            output.status.success(),
            "anchored PATH owner failed for {:?}: {}",
            component,
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!alias.exists(), "PATH aliases are removed after gate");
    }
}
