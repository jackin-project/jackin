use super::*;

#[test]
fn comparator_rejects_doctored_six_percent_regression() {
    let dir = tempfile::tempdir().unwrap();
    let baseline = dir.path().join("baseline.json");
    let current = dir.path().join("current.json");
    fs::write(
        &baseline,
        r#"{"max_regression_percent":5.0,"unit":"ns","calibration":10.0,"benchmarks":{"render":100.0}}"#,
    )
    .unwrap();
    fs::write(
        &current,
        r#"{"max_regression_percent":5.0,"unit":"ns","calibration":10.0,"benchmarks":{"render":106.0}}"#,
    )
    .unwrap();
    assert!(compare(&baseline, &current).is_err());
}

#[test]
fn comparator_accepts_uniform_runner_slowdown() {
    let dir = tempfile::tempdir().unwrap();
    let baseline = dir.path().join("baseline.json");
    let current = dir.path().join("current.json");
    fs::write(
        &baseline,
        r#"{"max_regression_percent":5.0,"unit":"ns","calibration":10.0,"benchmarks":{"render":100.0}}"#,
    )
    .unwrap();
    fs::write(
        &current,
        r#"{"max_regression_percent":5.0,"unit":"ns","calibration":20.0,"benchmarks":{"render":200.0}}"#,
    )
    .unwrap();
    compare(&baseline, &current).unwrap();
}

#[test]
fn comparator_rejects_missing_or_invalid_calibration() {
    let dir = tempfile::tempdir().unwrap();
    let baseline = dir.path().join("baseline.json");
    let current = dir.path().join("current.json");
    fs::write(
        &baseline,
        r#"{"max_regression_percent":5.0,"unit":"ns","calibration":10.0,"benchmarks":{"render":100.0}}"#,
    )
    .unwrap();
    fs::write(
        &current,
        r#"{"max_regression_percent":5.0,"unit":"ns","calibration":0.0,"benchmarks":{"render":100.0}}"#,
    )
    .unwrap();
    assert!(compare(&baseline, &current).is_err());

    fs::write(
        &current,
        r#"{"max_regression_percent":5.0,"unit":"ns","benchmarks":{"render":100.0}}"#,
    )
    .unwrap();
    assert!(compare(&baseline, &current).is_err());
}
