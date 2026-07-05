use super::*;

#[test]
fn ramped_interval_doubles_until_cap() {
    let initial = std::time::Duration::from_millis(100);
    let cap = std::time::Duration::from_millis(500);

    let intervals: Vec<_> = (0..6)
        .map(|attempt| ramped_interval(initial, cap, attempt).as_millis())
        .collect();

    assert_eq!(intervals, vec![100, 200, 400, 500, 500, 500]);
}

#[test]
fn sub_spin_interval_still_yields_one_sleep() {
    const SPIN_MS: u64 = 80;
    let interval = std::time::Duration::from_millis(10);

    let spins = (interval.as_millis() as u64 / SPIN_MS).max(1);

    assert_eq!(spins, 1);
}
