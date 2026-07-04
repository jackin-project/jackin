use super::*;

fn layout() -> (tempfile::TempDir, JackinPaths, DaemonLayout) {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let layout = DaemonLayout::new(&paths);
    (temp, paths, layout)
}

#[test]
fn daemon_layout_uses_private_run_dir() {
    let (_temp, _paths, layout) = layout();

    ensure_run_dir(&layout).unwrap();

    let mode = fs::metadata(&layout.run_dir).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700);
    assert_eq!(layout.socket_path, layout.run_dir.join(SOCKET_FILE_NAME));
}

#[test]
fn hello_reports_protocol_without_adapters() {
    let (_temp, _paths, layout) = layout();
    let request = DaemonRequest {
        id: "r1".to_owned(),
        protocol_version: DAEMON_PROTOCOL_VERSION,
        build_id: "test-build".to_owned(),
        kind: DaemonRequestKind::Hello,
    };

    let response = handle_request_line(
        &serde_json::to_string(&request).unwrap(),
        &layout,
        "test-build",
        &CoredumpPolicy::Disabled,
    );

    assert_eq!(
        response,
        DaemonResponse {
            id: "r1".to_owned(),
            kind: DaemonResponseKind::Hello {
                protocol_version: DAEMON_PROTOCOL_VERSION,
                build_id: "test-build".to_owned(),
                capabilities: Vec::new(),
            },
        }
    );
}

#[test]
fn protocol_and_build_mismatch_fail_closed() {
    let (_temp, _paths, layout) = layout();
    let protocol = DaemonRequest {
        id: "proto".to_owned(),
        protocol_version: DAEMON_PROTOCOL_VERSION + 1,
        build_id: "test-build".to_owned(),
        kind: DaemonRequestKind::Status,
    };
    let build = DaemonRequest {
        id: "build".to_owned(),
        protocol_version: DAEMON_PROTOCOL_VERSION,
        build_id: "old-build".to_owned(),
        kind: DaemonRequestKind::Status,
    };

    let response = handle_request_line(
        &serde_json::to_string(&protocol).unwrap(),
        &layout,
        "test-build",
        &CoredumpPolicy::Disabled,
    );
    assert!(matches!(
        response.kind,
        DaemonResponseKind::Error { ref message }
            if message.contains("unsupported daemon protocol")
    ));

    let response = handle_request_line(
        &serde_json::to_string(&build).unwrap(),
        &layout,
        "test-build",
        &CoredumpPolicy::Disabled,
    );
    assert!(matches!(
        response.kind,
        DaemonResponseKind::Error { ref message }
            if message.contains("daemon build mismatch")
    ));
}

#[test]
fn unit_files_target_explicit_daemon_serve() {
    let (_temp, paths, layout) = layout();
    let units = render_unit_files(&paths, Path::new("/bin/jackin"));

    assert!(units.launchd_plist.contains("<string>daemon</string>"));
    assert!(units.launchd_plist.contains("<string>serve</string>"));
    assert!(
        units
            .systemd_unit
            .contains("ExecStart=/bin/jackin daemon serve")
    );
    assert!(
        units
            .systemd_unit
            .contains(&layout.log_path.display().to_string())
    );
}
