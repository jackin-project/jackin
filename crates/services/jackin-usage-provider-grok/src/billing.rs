// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Grok` billing fetch and tier parsing.

use jackin_usage_provider_core::ProviderError;
use jackin_usage_provider_core::{
    ChildOperation, GROK_RPC_INIT_TIMEOUT, GROK_RPC_REQUEST_TIMEOUT, ManagedCliLaunchGate,
    get_json_bearer, provider_http_client,
};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Instant;

use super::{
    GrokBillingConfig, GrokBillingResponse, GrokBillingSnapshot, grok_bearer_token,
    grok_binary_path, grok_rpc_request,
};

pub fn fetch_grok_billing(
    auth_path: &Path,
    now: i64,
    gate: &mut ManagedCliLaunchGate,
) -> Result<GrokBillingSnapshot, ProviderError> {
    match fetch_grok_rest_billing(auth_path, now) {
        Ok(response) => Ok(GrokBillingSnapshot::Rest(Box::new(response))),
        Err(rest_error) => match fetch_grok_rpc_billing(gate) {
            Ok(response) => Ok(GrokBillingSnapshot::Rpc(Box::new(response))),
            Err(rpc_error) => Err(rest_error.with_message(format!(
                "{}; Grok ACP billing failed: {rpc_error}",
                rest_error.message()
            ))),
        },
    }
}

/// Fetch the supported Grok CLI-proxy REST contract. The ACP adapter remains
/// a fallback facade, but the direct grpc-web wire scan is not a production
/// source anymore.
pub fn fetch_grok_rest_billing(
    auth_path: &Path,
    now: i64,
) -> Result<GrokBillingResponse, ProviderError> {
    let token = grok_bearer_token(auth_path, now).map_err(ProviderError::from)?;
    let extra_headers = [
        (reqwest::header::USER_AGENT, "jackin-capsule"),
        (
            reqwest::header::HeaderName::from_static("x-xai-token-auth"),
            "xai-grok-cli",
        ),
    ];
    let billing_value = get_json_bearer::<serde_json::Value>(
        jackin_telemetry::schema::enums::ProviderName::Xai,
        "/v1/billing",
        "Grok billing",
        "https://cli-chat-proxy.grok.com/v1/billing?format=credits",
        &token,
        &extra_headers,
    )
    .map_err(ProviderError::from)?;
    let mut response =
        parse_grok_rest_billing_response(&billing_value).map_err(ProviderError::from)?;
    if let Ok(settings) = fetch_grok_rest_settings(&token)
        && let Some(tier) = grok_tier_from_settings(&settings)
    {
        response.subscription_tier = Some(tier);
    }
    Ok(response)
}

/// Server-resolved plan label from the `{proxy}/settings` payload. The display
/// form wins, then the machine form, in either case convention. Pure so the
/// lookup chain is unit-testable without provider I/O.
pub(crate) fn grok_tier_from_settings(settings: &serde_json::Value) -> Option<String> {
    settings
        .get("subscriptionTierDisplay")
        .or_else(|| settings.get("subscription_tier_display"))
        .or_else(|| settings.get("subscriptionTier"))
        .or_else(|| settings.get("subscription_tier"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|tier| !tier.is_empty())
        .map(str::to_owned)
}

pub(crate) fn fetch_grok_rest_settings(token: &str) -> Result<serde_json::Value, String> {
    let client = provider_http_client()?;
    let response = client
        .get("https://cli-chat-proxy.grok.com/v1/settings")
        .bearer_auth(token)
        .header(reqwest::header::ACCEPT, "application/json")
        .header("X-XAI-Token-Auth", "xai-grok-cli")
        .send()
        .map_err(|error| format!("Grok settings request failed: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("Grok settings HTTP {}", response.status()));
    }
    response
        .json::<serde_json::Value>()
        .map_err(|error| format!("Grok settings decode failed: {error}"))
}

pub fn parse_grok_rest_billing_response(
    value: &serde_json::Value,
) -> Result<GrokBillingResponse, String> {
    let payload = value.get("data").unwrap_or(value);
    if let Ok(response) = serde_json::from_value::<GrokBillingResponse>(payload.clone())
        && response.config.is_some()
    {
        return Ok(response);
    }
    let config = serde_json::from_value::<GrokBillingConfig>(payload.clone())
        .map_err(|error| format!("Grok billing shape unsupported: {error}"))?;
    Ok(GrokBillingResponse {
        config: Some(config),
        on_demand_enabled: None,
        subscription_tier: None,
    })
}

pub fn fetch_grok_rpc_billing(
    gate: &mut ManagedCliLaunchGate,
) -> Result<GrokBillingResponse, String> {
    gate.can_launch("Grok ACP billing", Instant::now())?;
    let executable = grok_binary_path();
    let process = ChildOperation::begin(executable.to_string_lossy().as_ref());
    let mut child = match Command::new(&executable)
        .args(["agent", "stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            process.spawn_failed();
            let message = format!(
                "{} agent stdio failed to start: {err}",
                executable.display()
            );
            gate.record_launch_failure(message.clone());
            return Err(message);
        }
    };

    let Some(mut stdin) = child.stdin.take() else {
        process.fail_managed_io(&mut child);
        return Err("grok agent stdio stdin unavailable".to_owned());
    };
    let Some(stdout) = child.stdout.take() else {
        process.fail_managed_io(&mut child);
        return Err("grok agent stdio stdout unavailable".to_owned());
    };
    let (tx, rx) = mpsc::channel();
    let reader = jackin_telemetry::spawn::thread_stream("grok.stdout", move || {
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
    let reaped = ChildOperation::reap_managed(&mut child);
    let reader_joined = reader.join().is_ok();
    process.finish_managed(reaped && reader_joined);
    if result.is_ok() {
        gate.record_success();
    } else if let Err(message) = &result {
        gate.record_launch_failure(message.clone());
    }
    result
}
