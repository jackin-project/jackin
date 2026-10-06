use super::*;

#[test]
fn comparator_rejects_doctored_six_percent_regression() {
    let dir = tempfile::tempdir().unwrap();
    let baseline = dir.path().join("baseline.json");
    let current = dir.path().join("current.json");
    fs::write(
        &baseline,
        r#"{"max_regression_percent":5.0,"unit":"ns","benchmarks":{"render":100.0}}"#,
    )
    .unwrap();
    fs::write(
        &current,
        r#"{"max_regression_percent":5.0,"unit":"ns","benchmarks":{"render":106.0}}"#,
    )
    .unwrap();
    assert!(compare(&baseline, &current).is_err());
}

#[test]
fn comparator_accepts_change_within_threshold() {
    let dir = tempfile::tempdir().unwrap();
    let baseline = dir.path().join("baseline.json");
    let current = dir.path().join("current.json");
    fs::write(
        &baseline,
        r#"{"max_regression_percent":5.0,"unit":"ns","benchmarks":{"render":100.0}}"#,
    )
    .unwrap();
    fs::write(
        &current,
        r#"{"max_regression_percent":5.0,"unit":"ns","benchmarks":{"render":104.9}}"#,
    )
    .unwrap();
    compare(&baseline, &current).unwrap();
}

#[test]
fn comparator_rejects_invalid_benchmark_measurement() {
    let dir = tempfile::tempdir().unwrap();
    let baseline = dir.path().join("baseline.json");
    let current = dir.path().join("current.json");
    fs::write(
        &baseline,
        r#"{"max_regression_percent":5.0,"unit":"ns","benchmarks":{"render":100.0}}"#,
    )
    .unwrap();
    fs::write(
        &current,
        r#"{"max_regression_percent":5.0,"unit":"ns","benchmarks":{"render":0.0}}"#,
    )
    .unwrap();
    assert!(compare(&baseline, &current).is_err());

    fs::write(
        &current,
        r#"{"max_regression_percent":5.0,"unit":"ns","benchmarks":{"render":null}}"#,
    )
    .unwrap();
    assert!(compare(&baseline, &current).is_err());
}
