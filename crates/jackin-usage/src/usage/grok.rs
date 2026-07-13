// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `Grok` / `xAI` usage snapshot.
//!
//! Carved out of `usage.rs` during codebase-health-enforcement Workstream W5
//! (file-size ratchet). Items in this module are `pub(crate)` so the
//! coordinator (`usage.rs`) can re-export them.

#[allow(
    clippy::wildcard_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
use super::*;
use serde::Deserialize;

pub(crate) fn grok_snapshot(
    agent: &str,
    now: i64,
    rpc_gate: &mut ManagedCliLaunchGate,
) -> FocusedUsageView {
    let data = home_path(".grok");
    let home_auth = data.join("auth.json");
    let handoff_auth = PathBuf::from(GROK_HANDOFF_AUTH_PATH);
    let home_exists = home_auth.exists();
    let auth = if home_exists { home_auth } else { handoff_auth };
    // `home_exists` short-circuits when home won, so the resolved path is
    // checked at most once.
    let has_auth = home_exists || auth.exists();
    let has_xai_api_key = env_value("XAI_API_KEY").is_some();
    let has_deployment_key = env_value("GROK_DEPLOYMENT_KEY").is_some();
    let billing_result = fetch_grok_billing(&auth, now, rpc_gate);
    grok_snapshot_from_rpc_result(
        agent,
        now,
        &auth,
        has_auth,
        has_xai_api_key,
        has_deployment_key,
        billing_result,
    )
}

pub(crate) fn grok_snapshot_from_rpc_result(
    agent: &str,
    now: i64,
    auth: &Path,
    has_auth: bool,
    has_xai_api_key: bool,
    has_deployment_key: bool,
    billing_result: Result<GrokBillingSnapshot, String>,
) -> FocusedUsageView {
    let has_credentials = has_auth || has_xai_api_key || has_deployment_key;
    let (billing_usage, billing_error) = match billing_result {
        Ok(usage) => (Some(usage), None),
        Err(error) => (None, Some(error)),
    };
    // credential_origin reflects the resolver arm that actually won
    // (`auth` is the resolved path — home `~/.grok/auth.json` or the handoff).
    let credential_origin = if has_auth {
        Some(if auth == Path::new(GROK_HANDOFF_AUTH_PATH) {
            format!("OAuth · {GROK_HANDOFF_AUTH_PATH}")
        } else {
            "OAuth · ~/.grok/auth.json".to_owned()
        })
    } else if has_xai_api_key {
        Some("API token · env XAI_API_KEY".to_owned())
    } else if has_deployment_key {
        Some("API token · env GROK_DEPLOYMENT_KEY".to_owned())
    } else {
        None
    };
    let account =
        grok_account_label_or_presence(auth, has_auth, has_xai_api_key, has_deployment_key);
    let status = if billing_usage.is_some() {
        UsageSnapshotStatus::Fresh
    } else if has_credentials {
        UsageSnapshotStatus::Error
    } else {
        UsageSnapshotStatus::NeedsLogin
    };
    let buckets = billing_usage
        .as_ref()
        .map(|usage| usage.buckets(now))
        .filter(|buckets| !buckets.is_empty())
        .unwrap_or_else(|| {
            vec![bucket(
                "Credits",
                None,
                None,
                None,
                None,
                Some("ACP billing unavailable"),
                status,
            )]
        });
    usage_view(UsageViewInput {
        agent,
        provider: None,
        surface: UsageSurface::Grok,
        account_label: account,
        username: None,
        plan_label: grok_plan_label(auth),
        credential_origin,
        buckets,
        status,
        source: billing_usage
            .as_ref()
            .map_or(UsageSource::None, GrokBillingSnapshot::source),
        confidence: if billing_usage.is_some() {
            UsageConfidence::Authoritative
        } else if has_credentials {
            UsageConfidence::PresenceOnly
        } else {
            UsageConfidence::None
        },
        now,
        last_error: match status {
            UsageSnapshotStatus::NeedsLogin => Some(
                billing_error.unwrap_or_else(|| "Grok auth not available to Capsule".to_owned()),
            ),
            UsageSnapshotStatus::Error => {
                billing_error.or_else(|| Some("Grok billing unavailable to Capsule".to_owned()))
            }
            _ => None,
        },
    })
}

#[derive(Debug, Deserialize)]
pub(crate) struct GrokBillingResponse {
    #[serde(rename = "billingCycle")]
    pub(crate) billing_cycle: Option<GrokBillingCycle>,
    #[serde(rename = "monthlyLimit")]
    pub(crate) monthly_limit: Option<GrokCent>,
    #[serde(rename = "onDemandCap")]
    pub(crate) on_demand_cap: Option<GrokCent>,
    #[serde(rename = "on_demand_enabled")]
    pub(crate) on_demand_enabled: Option<bool>,
    pub(crate) usage: Option<GrokBillingUsage>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GrokBillingCycle {
    #[serde(rename = "billingPeriodStart")]
    pub(crate) billing_period_start: Option<String>,
    #[serde(rename = "billingPeriodEnd")]
    pub(crate) billing_period_end: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GrokBillingUsage {
    #[serde(rename = "includedUsed")]
    pub(crate) included_used: Option<GrokCent>,
    #[serde(rename = "onDemandUsed")]
    pub(crate) on_demand_used: Option<GrokCent>,
    #[serde(rename = "totalUsed")]
    pub(crate) total_used: Option<GrokCent>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GrokCent {
    pub(crate) val: Option<i64>,
}

#[derive(Debug)]
pub(crate) enum GrokBillingSnapshot {
    Rpc(GrokBillingResponse),
    Web(GrokWebBillingSnapshot),
}

#[derive(Debug)]
pub(crate) struct GrokWebBillingSnapshot {
    pub(crate) used_percent: f64,
    pub(crate) reset_at_epoch: Option<i64>,
}

impl GrokBillingSnapshot {
    pub(crate) fn buckets(&self, now: i64) -> Vec<QuotaBucketView> {
        match self {
            Self::Rpc(response) => response.buckets(now),
            Self::Web(snapshot) => snapshot.buckets(now),
        }
    }

    pub(crate) fn source(&self) -> UsageSource {
        match self {
            Self::Rpc(_) => UsageSource::Cli,
            Self::Web(_) => UsageSource::ProviderApi,
        }
    }
}

impl GrokWebBillingSnapshot {
    pub(crate) fn buckets(&self, now: i64) -> Vec<QuotaBucketView> {
        let label = self.reset_at_epoch.map_or("Credits", |reset_at| {
            grok_cycle_label_from_reset(reset_at, now)
        });
        // Grok exposes only a billing cycle (no session), so it fills the Weekly
        // headline slot.
        let mut view = timed_bucket(
            label,
            None,
            None,
            {
                #[expect(
                    clippy::cast_sign_loss,
                    reason = "provider used_percent rounded; saturating_sub bounds u8"
                )]
                {
                    Some(100u8.saturating_sub(self.used_percent.round() as u8))
                }
            },
            self.reset_at_epoch,
            now,
            None,
            UsageSnapshotStatus::Fresh,
        );
        view.status_slot = Some(StatusSlot::Weekly);
        vec![view]
    }
}

impl GrokBillingResponse {
    pub(crate) fn buckets(&self, now: i64) -> Vec<QuotaBucketView> {
        let mut buckets = Vec::new();
        if let Some(limit) = self.monthly_limit.as_ref().and_then(|amount| amount.val) {
            let reset_at = self.billing_period_end_epoch();
            let label = self
                .billing_period_minutes()
                .map_or("Credits", grok_cycle_label_from_minutes);
            let total_used = self
                .usage
                .as_ref()
                .and_then(|usage| usage.total_used.as_ref())
                .and_then(|amount| amount.val)
                .unwrap_or(0);
            let used_percent = if limit > 0 {
                Some(((total_used as f64 / limit as f64) * 100.0).clamp(0.0, 100.0))
            } else {
                None
            };
            // Billing cycle fills the Weekly slot; Grok has no session window.
            buckets.push(with_status_slot(
                timed_bucket(
                    label,
                    Some(format_cents(total_used)),
                    Some(format_cents(limit)),
                    used_percent.map(|used| {
                        #[expect(
                            clippy::cast_sign_loss,
                            reason = "used_percent clamped 0.0..=100.0 above"
                        )]
                        {
                            100u8.saturating_sub(used.round() as u8)
                        }
                    }),
                    reset_at,
                    now,
                    None,
                    UsageSnapshotStatus::Fresh,
                ),
                Some(StatusSlot::Weekly),
            ));
        }
        if let Some(usage) = &self.usage
            && let Some(included) = usage.included_used.as_ref().and_then(|amount| amount.val)
            && included > 0
        {
            buckets.push(bucket(
                "Included usage",
                Some(format_cents(included)),
                None,
                None,
                None,
                Some("used this cycle"),
                UsageSnapshotStatus::Fresh,
            ));
        }
        if let Some(usage) = &self.usage
            && let Some(on_demand) = usage.on_demand_used.as_ref().and_then(|amount| amount.val)
            && on_demand > 0
        {
            buckets.push(bucket(
                "On-demand usage",
                Some(format_cents(on_demand)),
                self.on_demand_cap
                    .as_ref()
                    .and_then(|amount| amount.val)
                    .map(format_cents),
                None,
                None,
                self.on_demand_enabled
                    .unwrap_or(false)
                    .then_some("enabled")
                    .or(Some("disabled")),
                UsageSnapshotStatus::Fresh,
            ));
        }
        buckets
    }

    pub(crate) fn billing_period_end_epoch(&self) -> Option<i64> {
        parse_iso_epoch(self.billing_cycle.as_ref()?.billing_period_end.as_deref()?)
    }

    pub(crate) fn billing_period_minutes(&self) -> Option<i64> {
        let cycle = self.billing_cycle.as_ref()?;
        let start = parse_iso_epoch(cycle.billing_period_start.as_deref()?)?;
        let end = parse_iso_epoch(cycle.billing_period_end.as_deref()?)?;
        (end > start).then_some((end - start) / 60)
    }
}

pub(crate) fn grok_cycle_label_from_minutes(minutes: i64) -> &'static str {
    let days = minutes / (24 * 60);
    if (6..=8).contains(&days) {
        "Weekly"
    } else if (28..=31).contains(&days) {
        "Monthly"
    } else {
        "Credits"
    }
}

pub(crate) fn grok_cycle_label_from_reset(reset_at: i64, now: i64) -> &'static str {
    let days = reset_at.saturating_sub(now) / 86_400;
    if days <= 8 {
        "Weekly"
    } else if days <= 35 {
        "Monthly"
    } else {
        "Credits"
    }
}

pub(crate) fn fetch_grok_billing(
    auth_path: &Path,
    now: i64,
    gate: &mut ManagedCliLaunchGate,
) -> Result<GrokBillingSnapshot, String> {
    match fetch_grok_rpc_billing(gate) {
        Ok(response) => Ok(GrokBillingSnapshot::Rpc(response)),
        Err(rpc_error) => match fetch_grok_web_billing(auth_path, now) {
            Ok(snapshot) => {
                gate.record_success();
                Ok(GrokBillingSnapshot::Web(snapshot))
            }
            Err(web_error) => Err(format!(
                "{rpc_error}; Grok bearer billing failed: {web_error}"
            )),
        },
    }
}

pub(crate) fn fetch_grok_rpc_billing(
    gate: &mut ManagedCliLaunchGate,
) -> Result<GrokBillingResponse, String> {
    gate.can_launch("Grok ACP billing", Instant::now())?;
    let executable = grok_binary_path();
    let mut child = match Command::new(&executable)
        .args(["agent", "stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            let message = format!(
                "{} agent stdio failed to start: {err}",
                executable.display()
            );
            gate.record_launch_failure(message.clone());
            return Err(message);
        }
    };

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "grok agent stdio stdin unavailable".to_owned())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "grok agent stdio stdout unavailable".to_owned())?;
    let (tx, rx) = mpsc::channel();
    let reader = thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });

    let result: Result<GrokBillingResponse, String> = (|| {
        drop(grok_rpc_request(
            &mut stdin,
            &rx,
            1,
            "initialize",
            serde_json::json!({
                "protocolVersion": "1",
                "clientCapabilities": {
                    "fs": {
                        "readTextFile": false,
                        "writeTextFile": false
                    },
                    "terminal": false
                }
            }),
            GROK_RPC_INIT_TIMEOUT,
        )?);
        let billing_value = grok_rpc_request(
            &mut stdin,
            &rx,
            2,
            "x.ai/billing",
            serde_json::json!({}),
            GROK_RPC_REQUEST_TIMEOUT,
        )?;
        serde_json::from_value::<GrokBillingResponse>(billing_value)
            .map_err(|err| format!("Grok billing decode failed: {err}"))
    })();

    drop(stdin);
    drop(child.kill());
    drop(child.wait());
    drop(reader.join());
    if result.is_ok() {
        gate.record_success();
    } else if let Err(message) = &result {
        gate.record_launch_failure(message.clone());
    }
    result
}

pub(crate) fn grok_binary_path() -> PathBuf {
    let home_bin = home_path(".grok/bin/grok");
    if home_bin.is_file() {
        home_bin
    } else {
        PathBuf::from("grok")
    }
}

pub(crate) fn fetch_grok_web_billing(
    auth_path: &Path,
    now: i64,
) -> Result<GrokWebBillingSnapshot, String> {
    let token = grok_bearer_token(auth_path, now)?;
    let client = provider_http_client()?;
    let response = client
        .post("https://grok.com/grok_api_v2.GrokBuildBilling/GetGrokCreditsConfig")
        .bearer_auth(token)
        .header(reqwest::header::ORIGIN, "https://grok.com")
        .header(reqwest::header::REFERER, "https://grok.com/?_s=usage")
        .header(reqwest::header::ACCEPT, "*/*")
        .header(reqwest::header::CONTENT_TYPE, "application/grpc-web+proto")
        .header("x-grpc-web", "1")
        .header("x-user-agent", "connect-es/2.1.1")
        .header(reqwest::header::USER_AGENT, "jackin-capsule")
        .body(vec![0, 0, 0, 0, 0])
        .send()
        .map_err(|err| format!("request failed: {err}"))?;
    let status = response.status();
    let grpc_status = response
        .headers()
        .get("grpc-status")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let grpc_message = response
        .headers()
        .get("grpc-message")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let body = response
        .bytes()
        .map_err(|err| format!("body read failed: {err}"))?;
    if !status.is_success() {
        return Err(format!("HTTP {status}"));
    }
    if let Some(grpc_status) = grpc_status
        && grpc_status != "0"
    {
        return Err(format!(
            "gRPC status {grpc_status}: {}",
            grpc_message.unwrap_or_else(|| "unknown".to_owned())
        ));
    }
    parse_grok_web_billing_response(&body, now)
}

pub(crate) fn grok_bearer_token(auth_path: &Path, now: i64) -> Result<String, String> {
    let text = fs::read_to_string(auth_path).map_err(|err| format!("auth read failed: {err}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|err| format!("auth decode failed: {err}"))?;
    let Some(entries) = value.as_object() else {
        return Err("auth.json root is not an object".to_owned());
    };
    let mut legacy: Option<(&str, &serde_json::Value)> = None;
    for (scope, entry) in entries {
        let is_oidc = scope.starts_with("https://auth.x.ai::");
        let is_legacy = scope == "https://accounts.x.ai/sign-in" || scope.contains("/sign-in");
        if is_legacy {
            legacy = Some((scope, entry));
        }
        if is_oidc && let Some(token) = grok_bearer_token_from_entry(entry, now)? {
            return Ok(token);
        }
    }
    if let Some((_, entry)) = legacy
        && let Some(token) = grok_bearer_token_from_entry(entry, now)?
    {
        return Ok(token);
    }
    Err("no fresh Grok bearer token in auth.json".to_owned())
}

pub(crate) fn grok_bearer_token_from_entry(
    entry: &serde_json::Value,
    now: i64,
) -> Result<Option<String>, String> {
    let Some(token) = entry.get("key").and_then(serde_json::Value::as_str) else {
        return Ok(None);
    };
    if token.is_empty() {
        return Ok(None);
    }
    if let Some(expires_at) = entry.get("expires_at").and_then(serde_json::Value::as_str)
        && let Some(epoch) = parse_iso_epoch(expires_at)
        && epoch <= now
    {
        return Err("Grok bearer token is expired".to_owned());
    }
    Ok(Some(token.to_owned()))
}

pub(crate) fn parse_grok_web_billing_response(
    data: &[u8],
    now: i64,
) -> Result<GrokWebBillingSnapshot, String> {
    let mut payloads = grpc_web_data_frames(data);
    if payloads.is_empty() && looks_like_protobuf_payload(data) {
        payloads.push(data.to_vec());
    }
    if payloads.is_empty() {
        return Err("empty gRPC-web payload".to_owned());
    }
    let mut scan = ProtobufScan::default();
    for payload in payloads {
        scan.merge(scan_protobuf(&payload, 0, Vec::new(), &mut 0));
    }
    let used_percent = scan
        .fixed32_fields
        .iter()
        .filter(|field| {
            field.path.last() == Some(&1)
                && field.value.is_finite()
                && field.value >= 0.0
                && field.value <= 100.0
        })
        .min_by(|left, right| {
            left.path
                .len()
                .cmp(&right.path.len())
                .then_with(|| left.order.cmp(&right.order))
        })
        .map(|field| f64::from(field.value))
        .ok_or_else(|| "usage percent not found in Grok billing protobuf".to_owned())?;
    let reset_at_epoch = scan
        .varint_fields
        .iter()
        .filter(|field| field.value >= 1_700_000_000 && field.value <= 2_100_000_000)
        .filter_map(|field| i64::try_from(field.value).ok().map(|epoch| (field, epoch)))
        .filter(|(_, epoch)| *epoch > now)
        .min_by_key(|(field, epoch)| {
            let preferred = i32::from(field.path != [1, 5, 1]);
            (preferred, *epoch)
        })
        .map(|(_, epoch)| epoch);
    Ok(GrokWebBillingSnapshot {
        used_percent,
        reset_at_epoch,
    })
}

pub(crate) fn grpc_web_data_frames(data: &[u8]) -> Vec<Vec<u8>> {
    let mut frames = Vec::new();
    let mut index = 0;
    while index < data.len() {
        if index + 5 > data.len() {
            break;
        }
        let flags = data[index];
        let length = (usize::from(data[index + 1]) << 24)
            | (usize::from(data[index + 2]) << 16)
            | (usize::from(data[index + 3]) << 8)
            | usize::from(data[index + 4]);
        let start = index + 5;
        let end = start.saturating_add(length);
        if end > data.len() {
            break;
        }
        if flags & 0x80 == 0 {
            frames.push(data[start..end].to_vec());
        }
        index = end;
    }
    frames
}

pub(crate) fn scan_protobuf(
    data: &[u8],
    depth: usize,
    path: Vec<u64>,
    order: &mut usize,
) -> ProtobufScan {
    let mut scan = ProtobufScan::default();
    let mut index = 0;
    while index < data.len() {
        let field_start = index;
        let Some(key) = read_varint(data, &mut index) else {
            index = field_start.saturating_add(1);
            continue;
        };
        if key == 0 {
            index = field_start.saturating_add(1);
            continue;
        }
        let field_number = key >> 3;
        let wire_type = key & 0x07;
        let field_path = {
            let mut next = path.clone();
            next.push(field_number);
            next
        };
        match wire_type {
            0 => {
                if let Some(value) = read_varint(data, &mut index) {
                    scan.varint_fields.push(VarintField {
                        path: field_path,
                        value,
                    });
                } else {
                    index = field_start.saturating_add(1);
                }
            }
            1 => {
                index = index.saturating_add(8).min(data.len());
            }
            2 => {
                let Some(length) =
                    read_varint(data, &mut index).and_then(|v| usize::try_from(v).ok())
                else {
                    index = field_start.saturating_add(1);
                    continue;
                };
                let start = index;
                let end = start.saturating_add(length);
                if end > data.len() {
                    break;
                }
                if depth < 4 {
                    scan.merge(scan_protobuf(
                        &data[start..end],
                        depth + 1,
                        field_path,
                        order,
                    ));
                }
                index = end;
            }
            5 => {
                if index + 4 > data.len() {
                    break;
                }
                let bytes = [
                    data[index],
                    data[index + 1],
                    data[index + 2],
                    data[index + 3],
                ];
                index += 4;
                let value = f32::from_le_bytes(bytes);
                let current_order = *order;
                *order = order.saturating_add(1);
                scan.fixed32_fields.push(Fixed32Field {
                    path: field_path,
                    value,
                    order: current_order,
                });
            }
            _ => {
                index = field_start.saturating_add(1);
            }
        }
    }
    scan
}

pub(crate) fn grok_rpc_request(
    stdin: &mut impl Write,
    rx: &mpsc::Receiver<String>,
    id: i64,
    method: &str,
    params: serde_json::Value,
    timeout: Duration,
) -> Result<serde_json::Value, String> {
    let payload = grok_rpc_request_payload(id, method, params);
    write_json_line(
        stdin,
        &payload,
        "Grok RPC request encode failed",
        "Grok RPC request write failed",
    )?;

    let started = Instant::now();
    loop {
        let remaining = timeout
            .checked_sub(started.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));
        if remaining.is_zero() {
            return Err(format!("Grok RPC timed out waiting for {method}"));
        }
        let line = rx
            .recv_timeout(remaining)
            .map_err(|_| format!("Grok RPC timed out waiting for {method}"))?;
        let value: serde_json::Value =
            serde_json::from_str(&line).map_err(|err| format!("Grok RPC decode failed: {err}"))?;
        if value.get("id").and_then(serde_json::Value::as_i64) != Some(id) {
            continue;
        }
        if let Some(error) = value.get("error") {
            let message = error
                .get("message")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown error");
            return Err(format!("Grok RPC {method} failed: {message}"));
        }
        return value
            .get("result")
            .cloned()
            .ok_or_else(|| format!("Grok RPC {method} response missing result"));
    }
}

pub(crate) fn grok_rpc_request_payload(
    id: i64,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    })
}

pub(crate) fn grok_account_label(path: &Path) -> Option<String> {
    let value = read_json_file(path)?;
    first_string_key(&value, "email")
        .or_else(|| first_string_key(&value, "user_id"))
        .or_else(|| first_string_key(&value, "team_id"))
}

pub(crate) fn grok_plan_label(path: &Path) -> Option<String> {
    let value = read_json_file(path)?;
    first_string_key(&value, "auth_mode").map(|mode| {
        if mode.eq_ignore_ascii_case("oidc") {
            "SuperGrok".to_owned()
        } else {
            mode
        }
    })
}

pub(crate) fn grok_account_label_or_presence(
    auth_path: &Path,
    has_auth: bool,
    has_xai_api_key: bool,
    has_deployment_key: bool,
) -> String {
    grok_account_label(auth_path).unwrap_or_else(|| {
        if has_auth {
            "local Grok auth".to_owned()
        } else if has_xai_api_key {
            "XAI_API_KEY present".to_owned()
        } else if has_deployment_key {
            "GROK_DEPLOYMENT_KEY present".to_owned()
        } else {
            "needs Grok login".to_owned()
        }
    })
}
