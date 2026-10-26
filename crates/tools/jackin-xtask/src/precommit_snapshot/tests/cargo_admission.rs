use super::*;

#[test]
fn installed_owner_bounds_staged_cargo_config_before_gate() {
    let fixture = InstalledOwnerFixture::new(OwnerFixtureMode::BootstrapClean);
    let config = fixture.repository.join(".cargo/config.toml");
    fs::create_dir(config.parent().expect("Cargo config parent"))
        .expect("create staged Cargo config directory");
    fs::write(&config, "[build]\njobs = 1\n").expect("write valid staged Cargo config");
    git_command(&fixture.repository, &["add", ".cargo/config.toml"]);
    git_command(
        &fixture.repository,
        &[
            "commit",
            "--quiet",
            "--only",
            "-m",
            "valid staged Cargo config",
            "--",
            ".cargo/config.toml",
        ],
    );

    let mut owner = fixture.spawn_owner("status", 0, 20_000, 20_000, 20_000);
    let output = owner.wait_output(std::time::Duration::from_secs(8));
    assert!(
        output.status.success(),
        "valid staged Cargo config was rejected: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(&fixture.gate_marker).expect("valid config gate marker"),
        "absent\n"
    );

    for marker in [&fixture.cargo_marker, &fixture.gate_marker] {
        if marker.exists() {
            fs::remove_file(marker).expect("clear valid-run marker");
        }
    }
    let mut oversized = Vec::with_capacity(1024 * 1024 + 32);
    oversized.extend_from_slice(b"# fixture boundary comment\n");
    oversized.resize(1024 * 1024 + 1, b'\n');
    oversized.extend_from_slice(b"[build]\njobs = 1\n");
    fs::write(&config, &oversized).expect("write oversized staged Cargo config");
    git_command(&fixture.repository, &["add", ".cargo/config.toml"]);
    git_command(
        &fixture.repository,
        &[
            "commit",
            "--quiet",
            "--only",
            "-m",
            "oversized staged Cargo config",
            "--",
            ".cargo/config.toml",
        ],
    );
    let expected_index = fs::read(fixture.index()).expect("record reviewed index");

    let mut owner = fixture.spawn_owner("cargo-config-bound-error", 0, 20_000, 20_000, 20_000);
    let output = owner.wait_output(std::time::Duration::from_secs(8));
    assert!(
        output.status.success(),
        "oversized-config child assertion failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("staged Cargo config exceeds its byte limit"),
        "owner must reject the oversized config before the gate: {stderr}"
    );
    assert!(
        !fixture.gate_marker.exists(),
        "oversized config reached the gate"
    );
    assert_eq!(
        fs::read(fixture.index()).expect("index after oversized config"),
        expected_index
    );
    assert_eq!(
        fs::read(config).expect("oversized config after owner"),
        oversized
    );
}
