use super::Metadata;

#[test]
fn metadata_distinguishes_binary_and_library_crates() {
    let metadata: Metadata = serde_json::from_str(
        r#"{
          "packages": [
            {"name":"binary-only","targets":[{"doctest":false}]},
            {"name":"library","targets":[{"doctest":true},{"doctest":false}]}
          ]
        }"#,
    )
    .unwrap();

    let binary = metadata
        .packages
        .iter()
        .find(|package| package.name == "binary-only")
        .unwrap();
    let library = metadata
        .packages
        .iter()
        .find(|package| package.name == "library")
        .unwrap();
    assert!(!binary.targets.iter().any(|target| target.doctest));
    assert!(library.targets.iter().any(|target| target.doctest));
}
