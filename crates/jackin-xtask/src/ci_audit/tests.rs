use super::{duration, scan_log, strip_ansi};

#[test]
fn scanner_counts_dependency_and_cache_markers() {
    let markers = scan_log(
        "\u{1b}[32m   Compiling serde v1.0.0\u{1b}[0m\nDownloading crates ...\nCache not found\n",
    );
    assert_eq!(markers.builds, 1);
    assert_eq!(markers.downloads, 1);
    assert_eq!(markers.cache_misses, 1);
}

#[test]
fn ansi_stripping_and_duration_are_stable() {
    assert_eq!(strip_ansi("a\u{1b}[31mred\u{1b}[0mz"), "aredz");
    assert_eq!(duration(128), "2m 08s");
}
