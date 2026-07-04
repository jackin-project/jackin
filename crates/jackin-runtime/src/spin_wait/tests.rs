//! Tests for `spin_wait`.

use super::*;

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

#[tokio::test(start_paused = true)]
async fn spin_wait_sub_frame_interval_still_throttles_each_attempt() {
    let attempts = Rc::new(Cell::new(0_u32));
    let start = tokio::time::Instant::now();

    let err = spin_wait("waiting", 3, Duration::from_millis(20), {
        let attempts = Rc::clone(&attempts);
        move || {
            let attempts = Rc::clone(&attempts);
            async move {
                attempts.set(attempts.get() + 1);
                anyhow::bail!("not ready")
            }
        }
    })
    .await
    .unwrap_err();

    assert_eq!(attempts.get(), 3);
    assert_eq!(
        tokio::time::Instant::now() - start,
        Duration::from_millis(60)
    );
    assert_eq!(err.to_string(), "not ready");
}
