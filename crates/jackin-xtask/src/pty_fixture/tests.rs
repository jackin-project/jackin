use super::*;

#[derive(clap::Parser)]
struct TestCli {
    #[command(flatten)]
    args: PtyFixtureArgs,
}

#[test]
fn parser_accepts_capture_and_output_paths_only() {
    use clap::Parser as _;

    let parsed = TestCli::try_parse_from(["pty-fixture", "capture.bin", "fixture.bin"]).unwrap();
    assert_eq!(parsed.args.capture, PathBuf::from("capture.bin"));
    assert_eq!(parsed.args.out_bin, PathBuf::from("fixture.bin"));
    assert!(
        TestCli::try_parse_from([
            "pty-fixture",
            "run.jsonl",
            "legacy-session-label",
            "fixture.bin",
        ])
        .is_err(),
        "the retired JSONL/session-label parser shape must stay rejected"
    );
}

#[test]
fn copies_capture_byte_exactly() {
    let temp = tempfile::tempdir().unwrap();
    let capture = temp.path().join("capture.bin");
    let output = temp.path().join("fixture.bin");
    let bytes = b"\x1b[2J\0\xffpty";
    fs::write(&capture, bytes).unwrap();
    run(PtyFixtureArgs {
        capture,
        out_bin: output.clone(),
    })
    .unwrap();
    assert_eq!(fs::read(output).unwrap(), bytes);
}

#[test]
fn rejects_empty_capture() {
    let temp = tempfile::tempdir().unwrap();
    let capture = temp.path().join("capture.bin");
    fs::write(&capture, []).unwrap();
    assert!(
        run(PtyFixtureArgs {
            capture,
            out_bin: temp.path().join("fixture.bin"),
        })
        .is_err()
    );
}
