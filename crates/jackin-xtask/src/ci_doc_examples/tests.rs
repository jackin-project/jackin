// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use syn::visit::Visit;

use super::{Documentation, Metadata, has_runnable_doc_fence};

#[test]
fn metadata_resolves_package_manifest() {
    let metadata: Metadata = serde_json::from_str(
        r#"{
          "packages": [
            {"name":"library","manifest_path":"/workspace/crates/library/Cargo.toml"}
          ]
        }"#,
    )
    .unwrap();

    let library = metadata
        .packages
        .iter()
        .find(|package| package.name == "library")
        .unwrap();
    assert_eq!(
        library.manifest_path.to_string_lossy(),
        "/workspace/crates/library/Cargo.toml"
    );
}

#[test]
fn runnable_fences_are_rejected_but_non_runnable_fences_are_allowed() {
    let source = r#"
//! ```compile_fail
//! let _: u8 = "wrong";
//! ```
/// ```rust
/// assert!(true);
/// ```
pub fn runnable() {}
/// ```text
/// diagram only
/// ```
/// ```rust,ignore
/// intentionally not executable
/// ```
pub fn ignored() {}
/**
```text
block documentation
```
*/
pub fn block() {}
"#;
    let parsed = syn::parse_file(source).unwrap();
    let mut docs = Documentation::default();
    docs.visit_file(&parsed);
    assert!(has_runnable_doc_fence(&docs.markdown));

    let safe =
        syn::parse_file("/** ```text\\nnot executable\\n``` */ pub fn example() {}").unwrap();
    let mut docs = Documentation::default();
    docs.visit_file(&safe);
    assert!(!has_runnable_doc_fence(&docs.markdown));
}
