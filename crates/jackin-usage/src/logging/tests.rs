use super::*;

#[test]
fn rotates_oversized_multiplexer_log() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("multiplexer.log");
    let old = File::create(&path).unwrap();
    old.set_len(MAX_LOG_BYTES + 1).unwrap();
    drop(old);

    rotate_if_oversized(&path).unwrap();

    let rotated = temp.path().join("multiplexer.log.1");
    assert!(rotated.exists(), "oversized log should rotate to .1");
    assert!(
        !path.exists(),
        "rotation should move the oversized live log before init reopens it"
    );
}
