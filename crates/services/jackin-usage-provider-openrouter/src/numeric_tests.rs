// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

use jackin_protocol::control::StatusSlot;

pub(super) const fn key_fixture() -> &'static str {
    r#"{"data":{"label":"test-key","usage":12.5,"usage_daily":1.2,"usage_weekly":5.0,"usage_monthly":12.5,"limit":100.0,"limit_remaining":87.5,"is_free_tier":false}}"#
}

struct CannedSnapshotServer {
    base_url: String,
    stop: std::sync::mpsc::Sender<()>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl CannedSnapshotServer {
    fn start(key_body: &'static str) -> Self {
        use std::net::{Ipv4Addr, TcpListener};
        use std::sync::mpsc;
        use std::time::{Duration, Instant};

        const ACCEPT_TIMEOUT: Duration = Duration::from_secs(5);

        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let (stop, worker_stop) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let accept_deadline = Instant::now() + ACCEPT_TIMEOUT;
            loop {
                if !serve_connection(&listener, &worker_stop, key_body, accept_deadline) {
                    return;
                }
            }
        });
        Self {
            base_url: format!("http://{address}"),
            stop,
            worker: Some(worker),
        }
    }

    fn base_url(&self) -> &str {
        &self.base_url
    }
}

fn serve_connection(
    listener: &std::net::TcpListener,
    stop: &std::sync::mpsc::Receiver<()>,
    key_body: &'static str,
    accept_deadline: std::time::Instant,
) -> bool {
    use std::io::{Read, Write};
    use std::sync::mpsc::{RecvTimeoutError, TryRecvError};
    use std::time::{Duration, Instant};

    const MAX_REQUEST_BYTES: usize = 8192;
    const IO_TIMEOUT: Duration = Duration::from_millis(250);

    if Instant::now() >= accept_deadline {
        return false;
    }
    let mut stream = match listener.accept() {
        Ok((stream, _)) => stream,
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
            let remaining = accept_deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            return matches!(
                stop.recv_timeout(remaining.min(Duration::from_millis(5))),
                Err(RecvTimeoutError::Timeout)
            );
        }
        Err(_) => return false,
    };

    let header_deadline = Instant::now() + Duration::from_secs(5);
    if stream.set_write_timeout(Some(IO_TIMEOUT)).is_err() {
        return false;
    }
    let mut request = Vec::with_capacity(1024);
    let mut chunk = [0_u8; 1024];
    loop {
        if request.windows(4).any(|part| part == b"\r\n\r\n") {
            break;
        }
        if request.len() >= MAX_REQUEST_BYTES || Instant::now() >= header_deadline {
            return false;
        }
        if matches!(stop.try_recv(), Ok(()) | Err(TryRecvError::Disconnected)) {
            return false;
        }
        let remaining = header_deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero()
            || stream
                .set_read_timeout(Some(remaining.min(IO_TIMEOUT)))
                .is_err()
        {
            return false;
        }
        let count = match stream.read(&mut chunk) {
            Ok(count) if count > 0 => count,
            _ => return false,
        };
        if request.len() + count > MAX_REQUEST_BYTES {
            return false;
        }
        request.extend_from_slice(&chunk[..count]);
    }

    let body = if String::from_utf8_lossy(&request).contains("/credits") {
        serde_json::json!({"data": {"total_credits": 10.0, "total_usage": 2.5}}).to_string()
    } else {
        key_body.to_owned()
    };
    let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    if stream.write_all(response.as_bytes()).is_err() {
        return false;
    }
    true
}

impl Drop for CannedSnapshotServer {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _send_result = self.stop.send(());
            drop(worker.join());
        }
    }
}

mod case_01 {
    use super::*;

    #[test]
    fn openrouter_key_maps_cap_remaining_and_period_spend() {
        let quota =
            parse_openrouter_key_usage_raw(key_fixture(), 1_780_000_000).expect("key fixture");
        assert_eq!(quota.plan_label.as_deref(), Some("Pay as you go"));
        let cap = quota
            .buckets
            .iter()
            .find(|bucket| bucket.label == "Key Limit")
            .expect("cap row");
        assert_eq!(cap.used_label.as_deref(), Some("$12.50"));
        assert_eq!(cap.limit_label.as_deref(), Some("$100"));
        assert_eq!(
            cap.remaining_percent
                .map(jackin_protocol::control::CanonicalRemainingPercent::get),
            Some(87.5)
        );
        assert_eq!(
            cap.remaining_percent_origin,
            Some(jackin_protocol::control::PercentObservationOrigin::Derived)
        );
        assert_eq!(
            jackin_usage_provider_core::money_used_percent(cap)
                .map(jackin_protocol::control::RawPercentPoints::get),
            Some(12.5)
        );
        assert_eq!(
            jackin_usage_provider_core::select_quota_meter(cap).1,
            Some(jackin_protocol::control::UsageMeterSide::Used)
        );
        assert_eq!(cap.status_slot, Some(StatusSlot::Spend));
        let month = quota
            .buckets
            .iter()
            .find(|bucket| bucket.label == "Spent this month")
            .expect("monthly row");
        assert_eq!(month.used_label.as_deref(), Some("$12.50"));
        assert_eq!(month.remaining_percent, None);
        assert_eq!(
            quota
                .buckets
                .iter()
                .filter(|bucket| bucket.label.starts_with("Spent"))
                .count(),
            3
        );
    }
    #[test]
    fn openrouter_subcent_cap_preserves_key_limit_and_money_geometry() {
        let quota = parse_openrouter_key_usage_raw(
            r#"{"data":{"limit":0.001,"limit_remaining":0.0009,"is_free_tier":false}}"#,
            1_780_000_000,
        )
        .expect("subcent key");
        let cap = quota
            .buckets
            .iter()
            .find(|bucket| bucket.label == "Key Limit")
            .expect("subcent cap row");
        assert_eq!(
            cap.limit_money,
            Some(jackin_protocol::control::Money::new(10, "USD", 4))
        );
        assert_eq!(cap.limit_label.as_deref(), Some("$0.001"));
        assert_eq!(
            cap.used_money,
            Some(jackin_protocol::control::Money::new(1, "USD", 4))
        );
        assert_eq!(cap.used_label.as_deref(), Some("$0.0001"));
        assert_eq!(
            cap.remaining_percent
                .map(jackin_protocol::control::CanonicalRemainingPercent::get),
            Some(90.0)
        );
        assert_eq!(
            cap.remaining_percent_origin,
            Some(jackin_protocol::control::PercentObservationOrigin::Derived)
        );
        assert_eq!(
            jackin_usage_provider_core::money_used_percent(cap)
                .map(jackin_protocol::control::RawPercentPoints::get),
            Some(10.0)
        );

        {
            let quota = parse_openrouter_key_usage_raw(
                r#"{"data":{"usage_monthly":12.5,"limit":null,"limit_remaining":null,"is_free_tier":true}}"#,
                1_780_000_000,
            )
            .expect("null cap");
            assert_eq!(quota.plan_label.as_deref(), Some("Free tier"));
            assert!(
                quota
                    .buckets
                    .iter()
                    .all(|bucket| bucket.label != "Key Limit"),
                "no cap row without a configured cap"
            );
            assert!(
                quota
                    .buckets
                    .iter()
                    .all(|bucket| bucket.remaining_percent.is_none()),
                "no percentage without a denominator"
            );
            let month = quota
                .buckets
                .iter()
                .find(|bucket| bucket.label == "Spent this month")
                .expect("monthly spend still renders");
            assert_eq!(month.status_slot, Some(StatusSlot::Spend));
        }

        {
            let quota = parse_openrouter_key_usage_raw(r#"{"data":{"limit":50.0}}"#, 1_780_000_000)
                .expect("cap without remaining");
            let cap = quota
                .buckets
                .iter()
                .find(|bucket| bucket.label == "Key Limit")
                .expect("cap row");
            assert_eq!(cap.limit_label.as_deref(), Some("$50"));
            assert_eq!(cap.used_label, None);
            assert_eq!(cap.remaining_percent, None);
        }

        {
            let quota = parse_openrouter_key_usage_raw(
                r#"{"data":{"limit":0.0009,"limit_remaining":0.001,"is_free_tier":false}}"#,
                1_780_000_000,
            )
            .expect("remaining exceeds subcent cap");
            let cap = quota
                .buckets
                .iter()
                .find(|bucket| bucket.label == "Key Limit")
                .expect("reversed subcent cap row");
            assert_eq!(
                cap.limit_money,
                Some(jackin_protocol::control::Money::new(9, "USD", 4))
            );
            assert_eq!(
                cap.used_money,
                Some(jackin_protocol::control::Money::new(0, "USD", 4))
            );
            assert_eq!(cap.limit_label.as_deref(), Some("$0.0009"));
            assert_eq!(cap.used_label.as_deref(), Some("$0"));
            let remaining_percent = cap
                .remaining_percent
                .map(jackin_protocol::control::CanonicalRemainingPercent::get)
                .expect("derived remaining percentage");
            assert!(
                (remaining_percent - 111.11111111111111).abs() < 1.0e-12,
                "expected approximately 111.11111111111111 percent, got {remaining_percent}"
            );
            assert!(
                remaining_percent > 100.0,
                "overage remains visible above 100 percent"
            );
            assert_eq!(
                cap.remaining_percent_origin,
                Some(jackin_protocol::control::PercentObservationOrigin::Derived)
            );
            assert_eq!(
                jackin_usage_provider_core::money_used_percent(cap)
                    .map(jackin_protocol::control::RawPercentPoints::get),
                Some(0.0)
            );
        }
        {
            use jackin_protocol::control::Money;

            let quota = parse_openrouter_key_usage_raw(
                r#"{"data":{"limit":100000000000000000,"limit_remaining":90000000000000000.01}}"#,
                1_780_000_000,
            )
            .expect("large mixed-scale key response");
            let cap = quota
                .buckets
                .iter()
                .find(|bucket| bucket.label == "Key Limit")
                .expect("cap row");

            assert_eq!(
                cap.limit_money,
                Some(Money::new(100_000_000_000_000_000, "USD", 0))
            );
            assert_eq!(
                cap.used_money,
                Some(Money::new(999_999_999_999_999_999, "USD", 2))
            );
            assert_eq!(cap.limit_label.as_deref(), Some("$100000000000000000"));
            assert_eq!(cap.used_label.as_deref(), Some("$9999999999999999.99"));
            let points = jackin_usage_provider_core::money_used_percent(cap)
                .map(jackin_protocol::control::RawPercentPoints::get)
                .expect("mixed-scale ratio is available");
            assert!((points - 10.0).abs() < 1e-12);
        }
    }
    #[test]
    fn openrouter_period_spend_preserves_decimal_scale() {
        let quota = parse_openrouter_key_usage_raw(
            r#"{"data":{"usage_monthly":0.100000000000000001}}"#,
            1_780_000_000,
        )
        .expect("high precision key");
        let month = quota
            .buckets
            .iter()
            .find(|bucket| bucket.label == "Spent this month")
            .expect("monthly spend row");
        assert_eq!(
            month.used_money,
            Some(jackin_protocol::control::Money::new(
                100_000_000_000_000_001,
                "USD",
                18
            ))
        );
        assert_eq!(month.used_label.as_deref(), Some("$0.100000000000000001"));

        {
            let quota = parse_openrouter_key_usage_raw(
                r#"{"data":{"usage_monthly":12.50}}"#,
                1_780_000_000,
            )
            .expect("cent-scale monthly spend");
            let month = quota
                .buckets
                .iter()
                .find(|bucket| bucket.label == "Spent this month")
                .expect("cent-scale monthly row");
            assert_eq!(
                month.used_money,
                Some(jackin_protocol::control::Money::new(1_250, "USD", 2))
            );
            assert_eq!(month.used_label.as_deref(), Some("$12.50"));
        }

        {
            let quota = parse_openrouter_key_usage_raw(
                r#"{"data":{"usage_monthly":92233720368547758070e-1}}"#,
                1_780_000_000,
            )
            .expect("normalized maximum monthly spend");
            let month = quota
                .buckets
                .iter()
                .find(|bucket| bucket.label == "Spent this month")
                .expect("maximum monthly row");
            assert_eq!(
                month.used_money,
                Some(jackin_protocol::control::Money::new(i64::MAX, "USD", 0))
            );
            assert_eq!(month.used_label.as_deref(), Some("$9223372036854775807"));
        }

        {
            let quota = parse_openrouter_key_usage_raw(
                r#"{"data":{"usage_monthly":9223372036854775808}}"#,
                1_780_000_000,
            )
            .expect("valid response with unrepresentable monthly spend");
            assert!(
                quota
                    .buckets
                    .iter()
                    .all(|bucket| bucket.label != "Spent this month"),
                "unrepresentable monthly spend omits the monthly row"
            );
        }
        {
            let quota =
                parse_openrouter_key_usage_raw(r#"{"data":{"usage_monthly":1e-3}}"#, 1_780_000_000)
                    .expect("exponent-form monthly spend");
            let month = quota
                .buckets
                .iter()
                .find(|bucket| bucket.label == "Spent this month")
                .expect("exponent-form monthly row");
            assert_eq!(
                month.used_money,
                Some(jackin_protocol::control::Money::new(1, "USD", 3))
            );
            assert_eq!(month.used_label.as_deref(), Some("$0.001"));
        }

        {
            let quota =
                parse_openrouter_key_usage_raw(r#"{"data":{"usage_monthly":3.0}}"#, 1_780_000_000)
                    .expect("whole decimal monthly spend");
            let month = quota
                .buckets
                .iter()
                .find(|bucket| bucket.label == "Spent this month")
                .expect("whole decimal monthly row");
            assert_eq!(
                month.used_money,
                Some(jackin_protocol::control::Money::new(300, "USD", 2))
            );
            assert_eq!(month.used_label.as_deref(), Some("$3"));
        }
        {
            let server = CannedSnapshotServer::start(
                r#"{"data":{"limit":0.001,"limit_remaining":0.0009,"usage_monthly":0.100000000000000001,"is_free_tier":false}}"#,
            );
            let view = openrouter_snapshot_with_base(
                "opencode",
                Some("fixture-key"),
                server.base_url(),
                1_780_000_000,
            );

            let cap = view
                .buckets
                .iter()
                .find(|bucket| bucket.label == "Key Limit")
                .expect("cap row");
            assert_eq!(cap.limit_label.as_deref(), Some("$0.001"));
            assert_eq!(cap.used_label.as_deref(), Some("$0.0001"));
            assert_eq!(
                cap.limit_money,
                Some(jackin_protocol::control::Money::new(10, "USD", 4))
            );
            assert_eq!(
                cap.used_money,
                Some(jackin_protocol::control::Money::new(1, "USD", 4))
            );
            let month = view
                .buckets
                .iter()
                .find(|bucket| bucket.label == "Spent this month")
                .expect("monthly row");
            assert_eq!(month.used_label.as_deref(), Some("$0.100000000000000001"));
            assert_eq!(
                month.used_money,
                Some(jackin_protocol::control::Money::new(
                    100_000_000_000_000_001,
                    "USD",
                    18
                ))
            );
        }
    }
    #[test]
    fn openrouter_byok_fallback_sums_exact_subcent_values() {
        let quota = parse_openrouter_key_usage_raw(
            r#"{"data":{"byok_spend_a":0.001,"byok_cost_b":0.0009}}"#,
            1_780_000_000,
        )
        .expect("subcent BYOK key");
        let byok = quota
            .buckets
            .iter()
            .find(|bucket| bucket.label == "BYOK spend")
            .expect("BYOK spend row");
        assert_eq!(
            byok.used_money,
            Some(jackin_protocol::control::Money::new(19, "USD", 4))
        );
        assert_eq!(byok.used_label.as_deref(), Some("$0.0019"));

        {
            let quota = parse_openrouter_key_usage_raw(
                r#"{"data":{"byok_spend_a":1,"byok_cost_null":null,"byok_usage_text":"1.00"}}"#,
                1_780_000_000,
            )
            .expect("mixed BYOK key");
            let byok = quota
                .buckets
                .iter()
                .find(|bucket| bucket.label == "BYOK spend")
                .expect("numeric BYOK component remains available");
            assert_eq!(
                byok.used_money,
                Some(jackin_protocol::control::Money::new(100, "USD", 2))
            );
            assert_eq!(byok.used_label.as_deref(), Some("$1"));
        }
    }
    #[test]
    fn openrouter_byok_sums_fractional_carries_before_narrowing() {
        let quota = parse_openrouter_key_usage_raw(
            r#"{"data":{"byok_spend_a":9223372036854775806,"byok_spend_b":0.1,"byok_spend_c":0.9}}"#,
            1_780_000_000,
        )
        .expect("fractional carry BYOK key");
        let byok = quota
            .buckets
            .iter()
            .find(|bucket| bucket.label == "BYOK spend")
            .expect("BYOK spend row");
        assert_eq!(
            byok.used_money,
            Some(jackin_protocol::control::Money::new(i64::MAX, "USD", 0))
        );
    }
    #[test]
    fn openrouter_byok_omits_unrepresentable_numeric_fallback_groups() {
        for json in [
            r#"{"data":{"byok_spend_a":1,"byok_cost_b":1e-256}}"#,
            r#"{"data":{"byok_spend_a":1,"byok_cost_b":1e30}}"#,
        ] {
            let quota = parse_openrouter_key_usage_raw(json, 1_780_000_000)
                .expect("unrepresentable BYOK key remains a valid response");
            assert!(
                quota
                    .buckets
                    .iter()
                    .all(|bucket| bucket.label != "BYOK spend")
            );
        }
    }
    #[test]
    fn openrouter_byok_primary_skips_unrepresentable_month_and_selects_week() {
        let quota = parse_openrouter_key_usage_raw(
            r#"{"data":{"byok_usage_monthly":1e-256,"byok_usage_weekly":1}}"#,
            1_780_000_000,
        )
        .expect("period BYOK key");
        let byok = quota
            .buckets
            .iter()
            .find(|bucket| bucket.label == "BYOK spend")
            .expect("weekly BYOK fallback");
        assert_eq!(
            byok.used_money,
            Some(jackin_protocol::control::Money::new(100, "USD", 2))
        );
        assert_eq!(byok.used_label.as_deref(), Some("$1"));
    }
    #[test]
    fn openrouter_byok_fallback_ignores_caps() {
        let quota = parse_openrouter_key_usage_raw(
            r#"{"data":{"usage_monthly":12.5,"byok_limit":100.0,"byok_remaining":40.0}}"#,
            1_780_000_000,
        )
        .expect("byok caps fixture");
        assert!(
            quota
                .buckets
                .iter()
                .all(|bucket| bucket.label != "BYOK spend"),
            "caps never become a spend row"
        );
        let quota = parse_openrouter_key_usage_raw(
            r#"{"data":{"usage_monthly":12.5,"byok_spend_total":3.0}}"#,
            1_780_000_000,
        )
        .expect("byok spend fixture");
        let byok = quota
            .buckets
            .iter()
            .find(|bucket| bucket.label == "BYOK spend")
            .expect("byok row");
        assert_eq!(byok.used_label.as_deref(), Some("$3"));
        {
            let quota = parse_openrouter_key_usage_raw(
                r#"{"data":{"usage_monthly":12.5,"limit":100.0,"limit_remaining":87.5,"byok_usage_monthly":3.0}}"#,
                1_780_000_000,
            )
            .expect("byok fixture");
            let byok = quota
                .buckets
                .iter()
                .find(|bucket| bucket.label == "BYOK spend")
                .expect("byok row");
            assert_eq!(byok.used_label.as_deref(), Some("$3"));
            let cap = quota
                .buckets
                .iter()
                .find(|bucket| bucket.label == "Key Limit")
                .expect("cap row");
            assert_eq!(cap.used_label.as_deref(), Some("$12.50"));
        }
    }
    #[test]
    fn openrouter_raw_byok_preserves_dynamic_numeric_fallbacks_and_ignores_other_values() {
        let quota = parse_openrouter_key_usage_raw(
            r#"{"data":{"BYOK_COST_first":0.001,"my_Byok_Spend":0.0009,"byok_null":null,"byok_text":"5.00","byok_array":[3],"byok_flag":false}}"#,
            1_780_000_000,
        )
        .expect("mixed dynamic BYOK fields");
        let byok = quota
            .buckets
            .iter()
            .find(|bucket| bucket.label == "BYOK spend")
            .expect("numeric BYOK row");
        assert_eq!(byok.used_label.as_deref(), Some("$0.0019"));
        assert_eq!(
            byok.used_money,
            Some(jackin_protocol::control::Money::new(19, "USD", 4))
        );
    }
    #[test]
    fn openrouter_raw_byok_omits_incomplete_groups_and_selects_first_convertible_period() {
        for json in [
            r#"{"data":{"byok_spend_a":1,"byok_cost_b":1e-256}}"#,
            r#"{"data":{"byok_spend_a":1,"byok_cost_b":1e30}}"#,
            r#"{"data":{"byok_spend_a":9223372036854775807,"byok_cost_b":1}}"#,
        ] {
            let quota = parse_openrouter_key_usage_raw(json, 1_780_000_000)
                .expect("incomplete numeric group is a valid response");
            assert!(
                quota
                    .buckets
                    .iter()
                    .all(|bucket| bucket.label != "BYOK spend")
            );
        }

        let quota = parse_openrouter_key_usage_raw(
            r#"{"data":{"byok_usage_monthly":1e-256,"byok_usage_weekly":1}}"#,
            1_780_000_000,
        )
        .expect("period fallback key");
        let byok = quota
            .buckets
            .iter()
            .find(|bucket| bucket.label == "BYOK spend")
            .expect("weekly BYOK row");
        assert_eq!(byok.used_label.as_deref(), Some("$1"));
    }
    #[test]
    fn openrouter_raw_parser_preserves_duplicate_last_wins_and_typed_validation() {
        let quota = parse_openrouter_key_usage_raw(
            r#"{"data":{"usage_monthly":1},"data":{"usage_monthly":1.25,"usage_monthly":2.5}}"#,
            1_780_000_000,
        )
        .expect("duplicate keys use the final value");
        let month = quota
            .buckets
            .iter()
            .find(|bucket| bucket.label == "Spent this month")
            .expect("monthly row");
        assert_eq!(month.used_label.as_deref(), Some("$2.50"));
        assert_eq!(
            month.used_money,
            Some(jackin_protocol::control::Money::new(250, "USD", 2))
        );

        for json in [
            "{}",
            r#"{"data":null}"#,
            r#"{"data":{"usage_monthly":"1"}}"#,
        ] {
            assert!(
                parse_openrouter_key_usage_raw(json, 1_780_000_000).is_err(),
                "invalid typed response should fail: {json}"
            );
        }

        let quota = parse_openrouter_key_usage_raw(
            r#"{"data":{"usage_monthly":1.25,"usage_daily":null,"byok_cost_null":null,"byok_usage_text":"4.00"}}"#,
            1_780_000_000,
        )
        .expect("nullable known and nonnumeric dynamic fields are skipped");
        assert_eq!(
            quota
                .buckets
                .iter()
                .find(|bucket| bucket.label == "Spent this month")
                .and_then(|bucket| bucket.used_label.as_deref()),
            Some("$1.25")
        );
        assert!(
            quota
                .buckets
                .iter()
                .all(|bucket| bucket.label != "BYOK spend")
        );

        let quota = parse_openrouter_key_usage_raw(
            r#"{"data":{"usage_monthly":0e-400,"unknown_number":1e400}}"#,
            1_780_000_000,
        )
        .expect("zero exponent and unknown numeric field are accepted");
        let month = quota
            .buckets
            .iter()
            .find(|bucket| bucket.label == "Spent this month")
            .expect("zero monthly row");
        assert_eq!(month.used_label.as_deref(), Some("$0"));
        assert_eq!(
            month.used_money,
            Some(jackin_protocol::control::Money::new(0, "USD", 2))
        );

        for json in [
            r#"{"data":{"usage_monthly":1e-400}}"#,
            r#"{"data":{"usage_monthly":1e400}}"#,
        ] {
            let quota = parse_openrouter_key_usage_raw(json, 1_780_000_000)
                .expect("valid f64-shaped exponent is omitted from exact money");
            assert!(
                quota
                    .buckets
                    .iter()
                    .all(|bucket| bucket.label != "Spent this month")
            );
        }
        {
            let quota = parse_openrouter_key_usage_raw(
                r#"{"data":{"byok_spend_a":1.25,"BYOK_cost_b":2.5,"byok_cost_b":0.5,"byok_usage_text":"5","byok_null":null}}"#,
                1_780_000_000,
            )
            .expect("dynamic BYOK fields");
            let byok = quota
                .buckets
                .iter()
                .find(|bucket| bucket.label == "BYOK spend")
                .expect("numeric BYOK row");
            assert_eq!(
                byok.used_money,
                Some(jackin_protocol::control::Money::new(425, "USD", 2))
            );
            assert_eq!(byok.used_label.as_deref(), Some("$4.25"));

            let quota = parse_openrouter_key_usage_raw(
                r#"{"data":{"byok_spend_a":1,"byok_spend_a":3,"byok_cost_b":2}}"#,
                1_780_000_000,
            )
            .expect("duplicate BYOK key");
            let byok = quota
                .buckets
                .iter()
                .find(|bucket| bucket.label == "BYOK spend")
                .expect("duplicate BYOK row");
            assert_eq!(
                byok.used_money,
                Some(jackin_protocol::control::Money::new(500, "USD", 2))
            );
            assert_eq!(byok.used_label.as_deref(), Some("$5"));

            let quota = parse_openrouter_key_usage_raw(
                r#"{"data":{"usage_monthly":1},"data":{"usage_monthly":2}}"#,
                1_780_000_000,
            )
            .expect("duplicate top-level data key");
            let month = quota
                .buckets
                .iter()
                .find(|bucket| bucket.label == "Spent this month")
                .expect("last data object supplies monthly row");
            assert_eq!(
                month.used_money,
                Some(jackin_protocol::control::Money::new(200, "USD", 2))
            );
            assert_eq!(month.used_label.as_deref(), Some("$2"));
        }
    }
    #[test]
    fn openrouter_raw_parser_preserves_key_data_sequence_shapes_and_defaults() {
        for (json, expected_plan) in [
            (r#"{"data":[]}"#, None),
            (r#"{"data":[1.25]}"#, None),
            (r#"{"data":[null,null,null,2.5]}"#, None),
            (
                r#"{"data":[null,null,null,null,null,null,true]}"#,
                Some("Free tier"),
            ),
        ] {
            let raw = parse_openrouter_key_usage_raw(json, 1_780_000_000)
                .expect("typed-valid key-data sequence");
            assert!(
                raw.buckets.is_empty(),
                "sequence fields are not named money fields"
            );
            assert_eq!(raw.plan_label.as_deref(), expected_plan, "input: {json}");
        }

        for json in [
            r#"{"data":null}"#,
            r#"{"data":"key"}"#,
            r#"{"data":1}"#,
            r#"{"data":true}"#,
            r#"{"data":["1"]}"#,
            r#"{"data":[null,"1"]}"#,
            r#"{"data":[null,null,null,null,null,null,0]}"#,
            r#"{"data":[null,null,null,null,null,null,true,false]}"#,
        ] {
            assert!(
                parse_openrouter_key_usage_raw(json, 1_780_000_000).is_err(),
                "invalid key-data shape should fail: {json}"
            );
        }

        for json in [
            r#"{"data":[1e400]}"#,
            r#"{"data":[null,null,null,1e400]}"#,
            r#"{"data":[null,null,null,0e400]}"#,
            r#"{"data":[null,null,null,1e-400]}"#,
        ] {
            let raw = parse_openrouter_key_usage_raw(json, 1_780_000_000)
                .expect("extreme positional f64 value remains typed-valid");
            assert!(
                raw.buckets.is_empty(),
                "sequence fields are not named money fields"
            );
            assert_eq!(raw.plan_label, None, "input: {json}");
        }
    }
    #[test]
    fn openrouter_raw_parser_preserves_extreme_typed_numeric_behavior() {
        let zero = jackin_protocol::control::Money::new(0, "USD", 2);
        for json in [
            r#"{"data":{"usage_monthly":0e-400}}"#,
            r#"{"data":{"usage_monthly":-0e-400}}"#,
            r#"{"data":{"usage_monthly":0e400}}"#,
        ] {
            let quota = parse_openrouter_key_usage_raw(json, 1_780_000_000)
                .expect("signed or exponent-form zero remains typed-valid");
            let month = quota
                .buckets
                .iter()
                .find(|bucket| bucket.label == "Spent this month")
                .expect("zero monthly spend remains visible");
            assert_eq!(month.used_money, Some(zero.clone()), "input: {json}");
            assert_eq!(month.used_label.as_deref(), Some("$0"), "input: {json}");
        }

        for json in [
            r#"{"data":{"usage_monthly":1e-400}}"#,
            r#"{"data":{"usage_monthly":1e400}}"#,
        ] {
            let quota = parse_openrouter_key_usage_raw(json, 1_780_000_000)
                .expect("typed f64 field remains accepted");
            assert!(
                quota
                    .buckets
                    .iter()
                    .all(|bucket| bucket.label != "Spent this month"),
                "unrepresentable exact monthly value is omitted: {json}"
            );
        }

        let quota = parse_openrouter_key_usage_raw(
            r#"{"data":{"usage_monthly":1,"unknown_number":1e400}}"#,
            1_780_000_000,
        )
        .expect("unknown extreme field is ignored after typed projection");
        let month = quota
            .buckets
            .iter()
            .find(|bucket| bucket.label == "Spent this month")
            .expect("known monthly value survives an unknown extreme field");
        assert_eq!(
            month.used_money,
            Some(jackin_protocol::control::Money::new(100, "USD", 2))
        );
        assert_eq!(month.used_label.as_deref(), Some("$1"));
    }
    #[test]
    fn openrouter_raw_byok_skips_unrepresentable_negative_numeric_components() {
        for json in [
            r#"{"data":{"byok_spend_a":1,"byok_cost_b":-1e-256}}"#,
            r#"{"data":{"byok_cost_b":-1e-256,"byok_spend_a":1}}"#,
            r#"{"data":{"byok_spend_a":-1e-256,"byok_cost_b":1}}"#,
            r#"{"data":{"byok_spend_a":1,"byok_cost_b":-1e30}}"#,
            r#"{"data":{"byok_spend_a":1,"byok_cost_b":-1e400}}"#,
            r#"{"data":{"byok_spend_a":1,"byok_cost_b":-0.01}}"#,
        ] {
            let quota = parse_openrouter_key_usage_raw(json, 1_780_000_000).expect("BYOK key");
            let byok = quota
                .buckets
                .iter()
                .find(|bucket| bucket.label == "BYOK spend")
                .expect("representable positive fallback remains visible");
            assert_eq!(
                byok.used_money,
                Some(jackin_protocol::control::Money::new(100, "USD", 2))
            );
            assert_eq!(byok.used_label.as_deref(), Some("$1"));
        }

        let all_negative = parse_openrouter_key_usage_raw(
            r#"{"data":{"byok_spend_a":-1e-256,"byok_cost_b":-1e30,"byok_cost_c":-0.01}}"#,
            1_780_000_000,
        )
        .expect("all-negative BYOK key");
        assert!(
            all_negative
                .buckets
                .iter()
                .all(|bucket| bucket.label != "BYOK spend")
        );

        for token in ["-0", "-0.0e-400", "-0e400"] {
            let json = format!(r#"{{"data":{{"byok_spend_a":{token}}}}}"#);
            let quota = parse_openrouter_key_usage_raw(&json, 1_780_000_000).expect("BYOK key");
            let byok = quota
                .buckets
                .iter()
                .find(|bucket| bucket.label == "BYOK spend")
                .expect("negative numeric zero remains a reported contribution");
            assert_eq!(
                byok.used_money,
                Some(jackin_protocol::control::Money::new(0, "USD", 2))
            );
            assert_eq!(byok.used_label.as_deref(), Some("$0"));
        }
    }
    #[test]
    fn openrouter_raw_cap_minus_zero_preserves_fractional_usage() {
        for remaining in ["0", "-0", "-0.0e-400"] {
            let json = format!(
                r#"{{"data":{{"limit":0.5,"limit_remaining":{remaining},"is_free_tier":false}}}}"#
            );
            let quota =
                parse_openrouter_key_usage_raw(&json, 1_780_000_000).expect("fractional raw cap");
            let cap = quota
                .buckets
                .iter()
                .find(|bucket| bucket.label == "Key Limit")
                .expect("cap row");
            assert_eq!(
                cap.limit_money,
                Some(jackin_protocol::control::Money::new(50, "USD", 2))
            );
            assert_eq!(cap.limit_label.as_deref(), Some("$0.50"));
            assert_eq!(
                cap.used_money,
                Some(jackin_protocol::control::Money::new(50, "USD", 2))
            );
            assert_eq!(cap.used_label.as_deref(), Some("$0.50"));
            assert_eq!(
                cap.remaining_percent
                    .map(jackin_protocol::control::CanonicalRemainingPercent::get),
                Some(0.0)
            );
            assert_eq!(
                cap.remaining_percent_origin,
                Some(jackin_protocol::control::PercentObservationOrigin::Derived)
            );
            assert_eq!(
                jackin_usage_provider_core::money_used_percent(cap)
                    .map(jackin_protocol::control::RawPercentPoints::get),
                Some(100.0)
            );
        }

        let quota = parse_openrouter_key_usage_raw(
            r#"{"data":{"limit":0.001,"limit_remaining":0,"is_free_tier":false}}"#,
            1_780_000_000,
        )
        .expect("subcent raw cap");
        let cap = quota
            .buckets
            .iter()
            .find(|bucket| bucket.label == "Key Limit")
            .expect("subcent cap row");
        assert_eq!(
            cap.used_money,
            Some(jackin_protocol::control::Money::new(1, "USD", 3))
        );
        assert_eq!(cap.used_label.as_deref(), Some("$0.001"));
    }
}
