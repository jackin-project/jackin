// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

struct MockTransport {
    requests: std::sync::Mutex<Vec<String>>,
    receipt: Vec<u8>,
}
impl Transport for MockTransport {
    #[expect(
        clippy::unused_async_trait_impl,
        reason = "in-memory HTTP mock implements async transport without I/O"
    )]
    async fn get(&self, url: &str, _limit: usize) -> Result<Vec<u8>> {
        self.requests.lock().unwrap().push(url.into());
        if url.ends_with("/release.json") {
            Ok(self.receipt.clone())
        } else if url.ends_with("/release.json.bundle") {
            Ok(COSIGN_BUNDLE.to_vec())
        } else {
            anyhow::bail!("unverified receipt must block all downstream downloads")
        }
    }
}

#[tokio::test]
async fn mocked_http_authenticates_receipt_before_payload_download() {
    let mock = MockTransport {
        requests: std::sync::Mutex::new(Vec::new()),
        receipt: RECEIPT.to_vec(),
    };
    verify_with_transport(
        &mock,
        &verifier(),
        &receipt().version.value,
        "x86_64-unknown-linux-gnu",
        Some(&receipt().source.commit_sha),
    )
    .await
    .expect_err("invalid evidence must be rejected");
    let requests = mock.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(
        requests[0].ends_with("/release.json") && requests[1].ends_with("/release.json.bundle")
    );
    let immutable = format!(
        "/releases/download/preview-{}/",
        receipt().source.commit_sha
    );
    assert!(requests.iter().all(|url| url.contains(&immutable)));
}

#[tokio::test]
async fn unsupported_remote_versions_never_request_http() {
    let mock = MockTransport {
        requests: std::sync::Mutex::new(Vec::new()),
        receipt: Vec::new(),
    };
    for version in ["dev", "preview", "0.6.4-dev+abcdef0", "00.6.4"] {
        verify_with_transport(
            &mock,
            &verifier(),
            version,
            "x86_64-unknown-linux-gnu",
            Some(&receipt().source.commit_sha),
        )
        .await
        .expect_err("invalid evidence must be rejected");
    }
    assert!(mock.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn bare_local_or_overridden_versions_without_native_origin_never_request_http() {
    let mock = MockTransport {
        requests: std::sync::Mutex::new(Vec::new()),
        receipt: Vec::new(),
    };
    for version in ["0.6.4", "0.6.4-preview.1+abcdef0", "0.6.4+local"] {
        let error = verify_with_transport(
            &mock,
            &verifier(),
            version,
            "x86_64-unknown-linux-gnu",
            None,
        )
        .await
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("native release origin is required")
        );
    }
    assert!(mock.requests.lock().unwrap().is_empty());
}
