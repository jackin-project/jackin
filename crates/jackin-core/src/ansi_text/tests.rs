use super::strip_bytes;

#[test]
fn strip_removes_sgr_sequences() {
    assert_eq!(
        strip_bytes(b"\x1b[31merror\x1b[0m\n").as_slice(),
        b"error\n"
    );
}
