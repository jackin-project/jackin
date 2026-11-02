// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::{
    OpenShellWorkspace, OutputStream, Phase, ProcessStatus, ProfileId, ProviderName, SandboxName,
    TerminalText,
};
use jackin_openshell::{
    ClosedPolicyError, ClosureViolation, DiscoveryError, GatewayError, GatewayOp, HostPattern,
    Missing, PolicyDocument, RuleKey, TransferOp, resolve_gateway,
};
use tokio_util::sync::CancellationToken;

use super::fakes::{RegistryFs, ScriptedRunner, exited};
use super::harness::{GATEWAY, host, name};

fn missing_rows() -> Vec<(Missing, &'static str)> {
    vec![
        (
            Missing::Provider(ProviderName::parse("jackin-work-codex").unwrap()),
            "provider jackin-work-codex",
        ),
        (
            Missing::Sandbox(SandboxName::parse("jackin-work-0001").unwrap()),
            "sandbox jackin-work-0001",
        ),
        (
            Missing::Profile(ProfileId::parse("codex-chatgpt").unwrap()),
            "profile codex-chatgpt",
        ),
        (
            Missing::Workspace(OpenShellWorkspace::parse("default").unwrap()),
            "workspace default",
        ),
    ]
}

#[test]
fn a_missing_thing_is_named_as_a_word_and_its_name() {
    for (what, expected) in missing_rows() {
        let not_found = GatewayError::NotFound { what: what.clone() };
        assert_eq!(not_found.to_string(), format!("not found: {expected}"));
        let exists = GatewayError::AlreadyExists { what };
        assert_eq!(exists.to_string(), format!("already exists: {expected}"));
    }
}

#[test]
fn operations_phases_and_durations_print_as_words() {
    let rows = [
        (
            GatewayError::Timeout {
                op: GatewayOp::Health,
                after: Duration::from_secs(30),
            },
            "health timed out after 30s",
        ),
        (
            GatewayError::Timeout {
                op: GatewayOp::ReadFiles,
                after: Duration::from_millis(250),
            },
            "read files timed out after 250ms",
        ),
        (
            GatewayError::Cancelled {
                op: GatewayOp::UploadTree,
            },
            "upload tree was cancelled",
        ),
        (
            GatewayError::NotReady {
                phase: Phase::Completed { exit_code: 0 },
            },
            "the sandbox is not ready: completed with exit code 0",
        ),
        (
            GatewayError::Transfer {
                op: TransferOp::UploadFiltered,
                status: ProcessStatus::Exited(3),
                tail: TerminalText::sanitize(b"boom"),
            },
            "upload filtered failed with exit code 3: boom",
        ),
        (
            GatewayError::OutputTooLarge {
                op: GatewayOp::Program,
                stream: OutputStream::Stderr,
                limit: 10,
            },
            "the stderr output of program is larger than 10 bytes",
        ),
    ];
    for (error, expected) in rows {
        assert_eq!(error.to_string(), expected);
    }
}

fn rule(text: &str) -> RuleKey {
    PolicyDocument::parse(&format!(
        "version: 1\nnetwork_policies:\n  {text}:\n    name: {text}\n    endpoints:\n      - host: example.org\n        port: 443\n"
    ))
    .unwrap()
    .network_rules()
    .remove(0)
    .key
}

#[test]
fn registry_and_policy_errors_name_gateways_paths_and_violations_plainly() {
    let overlap = ClosureViolation::Overlap {
        rule: rule("r1"),
        host: HostPattern::parse("**").unwrap(),
        provider_host: HostPattern::parse("api.openai.com").unwrap(),
    };
    let unparsable = ClosureViolation::UnparsableHost {
        rule: rule("r2"),
        host: "bad host!".to_owned(),
    };
    let port = ClosureViolation::InvalidPort {
        rule: rule("r3"),
        port: 0,
    };
    let rows = [
        (
            DiscoveryError::UnknownGateway(name("absent")).to_string(),
            "gateway absent is not registered".to_owned(),
        ),
        (
            DiscoveryError::MissingCertificate {
                path: host("/home/me/my certs/ca.crt"),
            }
            .to_string(),
            "missing certificate /home/me/my certs/ca.crt".to_owned(),
        ),
        (
            DiscoveryError::ListFailed {
                status: ProcessStatus::Signaled(9),
                tail: TerminalText::sanitize(b"killed"),
            }
            .to_string(),
            "gateway list failed with signal 9: killed".to_owned(),
        ),
        (
            DiscoveryError::InsecureEndpoint {
                name: name(GATEWAY),
                scheme: Some("http".to_owned()),
            }
            .to_string(),
            format!("gateway {GATEWAY} has the endpoint scheme http; only https is accepted"),
        ),
        (
            DiscoveryError::InsecureEndpoint {
                name: name(GATEWAY),
                scheme: None,
            }
            .to_string(),
            format!("gateway {GATEWAY} has no endpoint scheme; only https is accepted"),
        ),
        (
            DiscoveryError::InsecureEndpoint {
                name: name(GATEWAY),
                scheme: Some("ht\u{1b}tp".to_owned()),
            }
            .to_string(),
            format!(
                "gateway {GATEWAY} has the endpoint scheme ht\\u{{1b}}tp; only https is accepted"
            ),
        ),
        (
            ClosedPolicyError::Violations(vec![overlap, unparsable, port]).to_string(),
            "the policy overlaps provider hosts: rule r1: ** overlaps the provider host api.openai.com, rule r2: the host bad host! cannot be read, rule r3: the port 0 is invalid".to_owned(),
        ),
    ];
    for (shown, expected) in rows {
        assert_eq!(shown, expected);
    }
}

#[tokio::test]
async fn an_authentication_read_from_the_registry_is_shown_without_control_characters() {
    let cancel = CancellationToken::new();
    let home = host("/home/test/.config/openshell");
    let fs = RegistryFs::default();
    let resolve = |auth: &str| {
        let listing = format!(
            r#"[{{"name": "{GATEWAY}", "endpoint": "https://localhost:17670", "auth": "{auth}"}}]"#
        );
        let runner = ScriptedRunner::always(exited(0, listing.as_bytes(), b""));
        let (fs, home, cancel) = (&fs, &home, &cancel);
        async move {
            resolve_gateway(&runner, fs, home, &name(GATEWAY), cancel)
                .await
                .err()
                .map(|error| error.to_string())
                .unwrap_or_default()
        }
    };
    assert_eq!(
        resolve("oidc").await,
        format!("gateway {GATEWAY} uses the authentication oidc; only mtls is accepted")
    );
    assert_eq!(
        resolve("oidc\\u001b[2J").await,
        format!(
            "gateway {GATEWAY} uses the authentication oidc\\u{{1b}}[2J; only mtls is accepted"
        )
    );
    assert_eq!(
        resolve("a\\u202eb").await,
        format!("gateway {GATEWAY} uses the authentication a\\u{{202e}}b; only mtls is accepted")
    );
}
