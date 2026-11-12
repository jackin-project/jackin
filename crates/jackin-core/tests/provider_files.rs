// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Provider;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn the_opencode_provider_reads_its_credential_store_and_names_no_login_file() -> TestResult {
    assert_eq!(Provider::Opencode.login_file(), None);
    assert_eq!(Provider::Opencode.credential_store(), Some("opencode.db"));
    Ok(())
}

#[test]
fn every_other_provider_names_no_credential_store() -> TestResult {
    for provider in Provider::ALL {
        if provider == Provider::Opencode {
            continue;
        }
        assert_eq!(provider.credential_store(), None, "{provider:?}");
    }
    Ok(())
}
