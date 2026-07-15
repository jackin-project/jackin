use super::*;

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
