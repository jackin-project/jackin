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
    let tool_body = format!(
        concat!(
            "#!/bin/sh\n",
            "name=${{0##*/}}\n",
            "case \"$name\" in\n",
            "  rustc) printf 'fixture-pinned-rustc\\n' > {}; printf 'rustc 1.99.1 (fixture)\\nbinary: rustc\\ncommit-hash: 0123456789abcdef0123456789abcdef01234567\\nrelease: 1.99.1\\n' ;;\n",
            "  rustdoc) printf 'rustdoc 1.99.1 (fixture)\\nbinary: rustdoc\\ncommit-hash: 0123456789abcdef0123456789abcdef01234567\\nrelease: 1.99.1\\n' ;;\n",
            "  clippy-driver) printf 'clippy 0.1.99 (0123456789abcdef0123456789abcdef01234567 fixture)\\n' ;;\n",
            "  rustfmt) printf 'rustfmt 1.99.1-stable (0123456789abcdef0123456789abcdef01234567 fixture)\\n' ;;\n",
            "  *) exit 91 ;;\n",
            "esac\n",
        ),
        shell_quote(&fixture.rustc_provider)
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
    let report = fs::read_to_string(&fixture.probe_output).expect("qualified tool report");
    let mut lines = report.lines();
    let target = PathBuf::from(lines.next().expect("private target path"));
    let _build = lines.next().expect("private build path");
    assert_eq!(
        PathBuf::from(lines.next().expect("selected Rust compiler")),
        fixture.expected_rustc
    );
    let selected_cargo = PathBuf::from(lines.next().expect("selected Cargo"));
    assert!(lines.next().is_none(), "unexpected tool report: {report}");
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
    let entries = env::split_paths(OsStr::new(path.trim_end_matches('\n'))).collect::<Vec<_>>();
    let aliases = entries.first().expect("private qualified alias entry");
    assert!(aliases.is_absolute());
    assert!(!aliases.as_os_str().as_encoded_bytes().contains(&b':'));
    assert!(
        aliases.is_dir(),
        "alias directory remains alive while gate runs"
    );
    assert_eq!(selected_cargo, aliases.join("cargo"));
    assert_eq!(
        fs::canonicalize(&selected_cargo).expect("physical selected Cargo"),
        fixture.expected_cargo
    );

    release_gate.release();
    let output = owner.wait_output(std::time::Duration::from_secs(8));
    assert!(
        output.status.success(),
        "colon-path owner failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read(&fixture.cargo_provider).unwrap(),
        b"fixture-mbx-cargo\n"
    );
    assert_eq!(
        fs::read(&fixture.rustc_provider).unwrap(),
        b"fixture-pinned-rustc\n"
    );
    assert!(
        !aliases.exists(),
        "qualified PATH aliases are removed after gate"
    );
}

#[test]
fn installed_owner_anchors_relative_tmpdir_before_snapshot_and_children() {
    let mut fixture = InstalledOwnerFixture::new(OwnerFixtureMode::IsolationGate);
    let relative_tmp = PathBuf::from("relative:temporary-root");
    let requested_tmp = fixture.repository.join(&relative_tmp);
    fs::create_dir(&requested_tmp).expect("relative colon-containing TMPDIR");
    fixture.tmpdir_override = Some(relative_tmp);

    let bin = fixture.repository.join(".git/f3-test/bin");
    let cargo = bin.join("cargo");
    let original_directory = bin.join("original-cargo");
    fs::create_dir(&original_directory).expect("retain fixture MBX Cargo directory");
    let original_cargo = original_directory.join("cargo");
    fs::rename(&cargo, &original_cargo).expect("retain fixture MBX Cargo provider");
    let observed_tmp = fixture.repository.join(".git/f3-test/observed-tmpdir");
    write_executable(
        &cargo,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$TMPDIR\" > {}\nexec {} \"$@\"\n",
            shell_quote(&observed_tmp),
            shell_quote(&original_cargo)
        )
        .as_bytes(),
    );

    let mut owner = fixture.spawn_owner("status", 0, 20_000, 20_000, 20_000);
    let release_gate = GateReleaseOnDrop(fixture.gate_release.clone());
    wait_for_owner_readiness(&mut owner, &fixture.gate_ready, &fixture.ticks);
    release_gate.release();
    let output = owner.wait_output(std::time::Duration::from_secs(8));
    assert!(
        output.status.success(),
        "relative-TMPDIR owner failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let expected_tmp = fs::canonicalize(requested_tmp).expect("physical requested TMPDIR");
    assert_eq!(
        fs::read_to_string(&observed_tmp)
            .expect("Mise and gate TMPDIR marker")
            .trim_end(),
        expected_tmp.to_str().expect("UTF-8 physical TMPDIR")
    );
    let report = fs::read_to_string(&fixture.probe_output).expect("qualified tool report");
    let target = PathBuf::from(report.lines().next().expect("private target path"));
    assert!(
        target.starts_with(&expected_tmp),
        "private build scratch follows the original relative TMPDIR: {target:?}"
    );
    let path = fs::read_to_string(&fixture.path_output).expect("qualified PATH record");
    let alias = env::split_paths(OsStr::new(path.trim_end_matches('\n')))
        .next()
        .expect("first qualified PATH entry");
    let fallback = fs::canonicalize("/tmp").expect("physical separator-safe fallback");
    assert_eq!(alias.parent(), Some(fallback.as_path()));
    assert!(
        !alias.exists(),
        "private PATH alias is cleaned after the gate"
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
        fs::create_dir(&colon_cwd_parent).expect("create colon-containing hook directory");
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
        assert_eq!(entries.len(), 5, "gate PATH entries: {entries:?}");
        assert!(
            entries[0].is_dir(),
            "private aliases are alive through the gate"
        );
        assert_eq!(
            &entries[1..],
            ["/usr/bin", "/bin", "/usr/sbin", "/sbin"].map(PathBuf::from)
        );
        let anchored_relative = if component.as_os_str().is_empty() {
            fixture.repository.clone()
        } else {
            fixture.repository.join(&component)
        };
        assert!(
            !entries.contains(&provider_bin) && !entries.contains(&anchored_relative),
            "inherited discovery paths do not reach the gate: {entries:?}"
        );
        let alias = entries[0].clone();
        let report = fs::read_to_string(&fixture.probe_output).expect("qualified tool report");
        assert!(
            PathBuf::from(report.lines().nth(3).expect("selected Cargo")).starts_with(&alias),
            "Mise/Cargo discovery succeeded from inherited relative PATH before gate: {report}"
        );
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
