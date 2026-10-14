// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::protocol::control::frame;
use anyhow::{Context as _, Result, bail};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

pub(crate) async fn read_framed(stream: &mut UnixStream, max: usize) -> Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    anyhow::ensure!(
        len <= max,
        "framed reply too large: {len} bytes (max {max})"
    );
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).await?;
    Ok(body)
}

pub async fn resolve_credentials(
    host_sock_path: &str,
    refs: Vec<jackin_protocol::ExecBinding>,
) -> Result<std::collections::BTreeMap<String, String>> {
    use jackin_protocol::{CredReply, CredRequest};

    if refs.is_empty() {
        return Ok(std::collections::BTreeMap::default());
    }

    const METHOD: &str = "jackin.host.Credentials/Resolve";
    let attrs = [
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_SYSTEM_NAME,
            value: jackin_telemetry::Value::Str("jackin"),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_METHOD,
            value: jackin_telemetry::Value::Str(METHOD),
        },
    ];
    let operation =
        jackin_telemetry::operation(&jackin_telemetry::operation::RPC_CLIENT, &attrs).ok();
    let result = async {
        let mut stream = jackin_diagnostics::operation::connection_attempt(
            jackin_telemetry::schema::enums::ConnectionPeerType::HostDaemon,
            UnixStream::connect(host_sock_path),
        )
        .await
        .with_context(|| format!("connecting to host credential resolver at {host_sock_path}"))?;

        let mut ctx = jackin_protocol::TelemetryContext::v1();
        if let Some(operation) = operation.as_ref() {
            operation
                .span()
                .in_scope(|| jackin_telemetry::propagation::inject(&mut ctx));
        } else {
            jackin_telemetry::propagation::inject(&mut ctx);
        }
        stream.write_all(&frame(&CredRequest { ctx, refs })).await?;

        const MAX_REPLY: usize = 1024 * 1024;
        let reply_body = read_framed(&mut stream, MAX_REPLY)
            .await
            .context("reading host.sock reply")?;

        match serde_json::from_slice::<CredReply>(&reply_body).context("parsing host.sock reply")? {
            CredReply::Ok { values } => Ok(values),
            CredReply::Error { error } => bail!("{error}"),
        }
    }
    .await;
    if let Some(operation) = operation {
        operation.complete(
            if result.is_ok() {
                jackin_telemetry::schema::enums::OutcomeValue::Success
            } else {
                jackin_telemetry::schema::enums::OutcomeValue::Failure
            },
            result
                .as_ref()
                .err()
                .map(|_| jackin_telemetry::schema::enums::ErrorType::RpcError),
        );
    }
    result
}

pub async fn execute_command(
    command: &str,
    args: &[String],
    extra_env: &std::collections::BTreeMap<String, String>,
    secrets_for_redaction: &[&str],
) -> Result<(i32, String, String, u32)> {
    let mut request = jackin_process::ExecRequest::new(command, args).no_timeout();
    request = request.envs(extra_env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    let output = crate::process_telemetry::exec_async_as(
        &request,
        jackin_telemetry::schema::enums::ProcessExecutableName::ConfiguredCommand,
    )
    .await
    .context("running configured command")?;

    let exit_code = output.code.unwrap_or(-1);

    let mut stdout = into_utf8(output.stdout);
    let mut stderr = into_utf8(output.stderr);

    let mut redacted_count = 0u32;
    for &secret in secrets_for_redaction {
        if secret.is_empty() {
            continue;
        }

        let out_hits = stdout.matches(secret).count();
        let err_hits = stderr.matches(secret).count();
        if out_hits > 0 {
            stdout = stdout.replace(secret, "[redacted by jackin']");
        }
        if err_hits > 0 {
            stderr = stderr.replace(secret, "[redacted by jackin']");
        }
        redacted_count = redacted_count
            .saturating_add(u32::try_from(out_hits.saturating_add(err_hits)).unwrap_or(u32::MAX));
    }

    if secrets_for_redaction
        .iter()
        .any(|s| s.contains("BEGIN") && s.contains("PRIVATE KEY"))
    {
        redact_pem(&mut stdout, &mut redacted_count);
        redact_pem(&mut stderr, &mut redacted_count);
    }

    const MAX_OUTPUT: usize = 1024 * 1024;
    cap_output(&mut stdout, MAX_OUTPUT);
    cap_output(&mut stderr, MAX_OUTPUT);

    Ok((exit_code, stdout, stderr, redacted_count))
}

pub(crate) fn into_utf8(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

pub(crate) fn cap_output(s: &mut String, max: usize) {
    if s.len() <= max {
        return;
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s.truncate(end);
    s.push_str("\n[output truncated at 1 MiB]");
}

pub(crate) fn redact_pem(s: &mut String, count: &mut u32) {
    let begin = "-----BEGIN";
    let end = "-----";
    while let Some(start) = s.find(begin) {
        if let Some(end_idx) = s[start..].find("-----END")
            && let Some(close) = s[start + end_idx + 8..].find(end)
        {
            let remove_end = start + end_idx + 8 + close + end.len();
            s.replace_range(start..remove_end, "[key material redacted by jackin']");
            *count += 1;
            continue;
        }
        break;
    }
}
