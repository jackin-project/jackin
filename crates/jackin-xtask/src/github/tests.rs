// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use super::{PublishPreviewArgs, retired_publish_preview};

#[test]
fn publish_preview_is_retired_without_mutation() {
    let error = retired_publish_preview(PublishPreviewArgs {
        repository: "jackin-project/jackin".to_owned(),
        tag: "preview".to_owned(),
        version: "0.6.4-preview.1+0123456".to_owned(),
        sha: "0123456789012345678901234567890123456789".to_owned(),
        assets: PathBuf::from("artifacts"),
    })
    .expect_err("retired publisher must fail closed");
    let message = error.to_string();
    assert!(message.contains("retired"));
    assert!(message.contains("release-preview-package"));
}
