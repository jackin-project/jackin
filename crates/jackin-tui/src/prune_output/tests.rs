use super::*;

fn rendered_status_column(action: &str, target: &str, status: &str) -> usize {
    let (prefix, dots) = pending_parts(action, target);
    let line = format!("    {prefix} {dots} {status}");
    line.find(status).unwrap()
}

#[test]
fn pending_rows_align_status_column() {
    let cases = [
        ("Finding", "managed containers", "OK"),
        ("Stopping", "jk-dawwxb7e-jackin-thearchitect", "OK"),
        ("Reading", "instance index", "OK"),
        ("Deleting", "jk-n8ngw2d2-jackin-thearchitect", "FAILED"),
        (
            "Deleting",
            "jk-extraordinarily-long-container-name-that-needs-truncation-thearchitect",
            "FAILED",
        ),
    ];

    let columns: Vec<usize> = cases
        .iter()
        .map(|(action, target, status)| rendered_status_column(action, target, status))
        .collect();

    assert!(
        columns.windows(2).all(|pair| pair[0] == pair[1]),
        "status columns must match: {columns:?}"
    );
}

#[test]
fn complete_propagates_errors_after_finalizing_row() {
    let row = PendingRow { finalized: false };
    let result: Result<(), &str> = row.complete(Err("boom"), ToString::to_string);
    assert_eq!(result, Err("boom"));
}
