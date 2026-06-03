//! Tests for `animation`.
use super::format_universe_duration;
use std::time::Duration;

#[test]
fn formats_session_duration_compactly() {
    assert_eq!(format_universe_duration(Duration::from_secs(45)), "45s");
    assert_eq!(format_universe_duration(Duration::from_secs(450)), "7m 30s");
    assert_eq!(format_universe_duration(Duration::from_mins(134)), "2h 14m");
    assert_eq!(format_universe_duration(Duration::from_secs(0)), "0s");
}
