#[cfg(test)]
use super::resolve_run_log_location;

#[test]
fn diagnostics_path_prefers_host_supplied_path() {
    let (display, href) = resolve_run_log_location(
        "jk-run-abc123",
        Some("/Users/operator/.jackin/data/diagnostics/runs/jk-run-abc123.jsonl"),
        "/home/agent",
    );

    assert_eq!(
        display,
        "/Users/operator/.jackin/data/diagnostics/runs/jk-run-abc123.jsonl"
    );
    assert_eq!(
        href.as_deref(),
        Some("file:///Users/operator/.jackin/data/diagnostics/runs/jk-run-abc123.jsonl")
    );
}

#[test]
fn diagnostics_path_falls_back_to_container_home_for_older_launches() {
    let (display, href) = resolve_run_log_location("jk-run-abc123", None, "/home/agent");

    assert_eq!(
        display,
        "~/.jackin/data/diagnostics/runs/jk-run-abc123.jsonl"
    );
    assert_eq!(
        href.as_deref(),
        Some("file:///home/agent/.jackin/data/diagnostics/runs/jk-run-abc123.jsonl")
    );
}
}
