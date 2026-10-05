use std::time::Duration;

use super::{
    PROCESS_OUTPUT_MAX, UsageFormatPrefs, compact_duration_label, read_rpc_frames,
    reset_label_with_prefs,
};

#[test]
fn under_one_hour_is_minutes() {
    assert_eq!(compact_duration_label(0), "<1m");
    assert_eq!(compact_duration_label(59), "<1m");
    assert_eq!(compact_duration_label(45 * 60), "45m");
    assert_eq!(compact_duration_label(3_599), "59m");
}

#[test]
fn sub_minute_reset_uses_compact_and_honest_long_forms() {
    let label = reset_label_with_prefs(10_059, 10_000, UsageFormatPrefs::default());
    assert!(label.starts_with("Resets in under a minute ("), "{label}");
    assert!(!label.contains("0m"), "{label}");
}

#[test]
fn under_forty_eight_hours_stays_hours_not_days() {
    assert_eq!(compact_duration_label(24 * 3_600), "24h");
    assert_eq!(compact_duration_label(36 * 3_600), "36h");
    assert_eq!(compact_duration_label(36 * 3_600 + 30 * 60), "36h 30m");
    assert_eq!(compact_duration_label(47 * 3_600), "47h");
    assert_eq!(compact_duration_label(47 * 3_600 + 59 * 60), "47h 59m");
    let label = compact_duration_label(47 * 3_600);
    assert!(!label.contains('d'), "got day form under 48h: {label}");
}

#[test]
fn at_and_above_forty_eight_hours_uses_days() {
    assert_eq!(compact_duration_label(48 * 3_600), "2d");
    assert_eq!(compact_duration_label(48 * 3_600 + 3_600), "2d 1h");
    assert_eq!(compact_duration_label(72 * 3_600), "3d");
    assert_eq!(compact_duration_label(3 * 86_400 + 4 * 3_600), "3d 4h");
}

#[test]
fn rpc_frame_reader_rejects_oversized_unterminated_output() {
    struct Endless {
        read: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }
    impl std::io::Read for Endless {
        fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
            bytes.fill(b'x');
            self.read.fetch_add(bytes.len(), std::sync::atomic::Ordering::Relaxed);
            Ok(bytes.len())
        }
    }
    let read = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let input = Endless { read: read.clone() };
    let (tx, rx) = std::sync::mpsc::sync_channel(0);
    let reader = std::thread::spawn(move || read_rpc_frames(input, tx));
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(5)).unwrap().unwrap_err(),
        "RPC output exceeded limit"
    );
    reader.join().unwrap();
    // BufReader may prefetch one bounded buffer past the frame limit.
    assert!(read.load(std::sync::atomic::Ordering::Relaxed) <= PROCESS_OUTPUT_MAX + 8192);
}

#[test]
fn rpc_frame_reader_cancellation_releases_rendezvous_sender() {
    let (tx, rx) = std::sync::mpsc::sync_channel(0);
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        read_rpc_frames(std::io::Cursor::new(b"first\nsecond\nthird\n"), tx);
        done_tx.send(()).unwrap();
    });
    assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap().unwrap(), "first");
    assert!(done_rx.try_recv().is_err(), "reader must not queue later frames");
    drop(rx);
    done_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    reader.join().unwrap();
}

#[test]
fn rpc_frame_reader_reports_invalid_utf8() {
    let (tx, rx) = std::sync::mpsc::sync_channel(0);
    let reader = std::thread::spawn(move || read_rpc_frames(std::io::Cursor::new([255, b'\n']), tx));
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(5)).unwrap().unwrap_err(),
        "RPC output was not UTF-8"
    );
    reader.join().unwrap();
}
