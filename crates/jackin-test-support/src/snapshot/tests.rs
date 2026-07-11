use super::*;

#[test]
fn redact_digit_runs_keeps_short_and_scrubs_long() {
    assert_eq!(redact_digit_runs("port 80 ok"), "port 80 ok");
    assert_eq!(
        redact_digit_runs("ts 1700000000 done"),
        "ts <digits> done"
    );
}

#[test]
fn normalize_strips_trailing_blank_and_spaces() {
    assert_eq!(normalize_snapshot_text("a  \n\nb \n\n"), "a\n\nb\n");
}
