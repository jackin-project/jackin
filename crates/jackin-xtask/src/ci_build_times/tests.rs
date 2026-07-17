use super::package_sources_from_json;

#[test]
fn resolves_library_sources_from_cargo_metadata() {
    let metadata = br#"{"packages":[{"name":"jackin-core","targets":[{"kind":["lib"],"src_path":"/repo/crates/jackin-core/src/lib.rs"}]}]}"#;

    let sources = package_sources_from_json(metadata).unwrap();

    assert_eq!(
        sources["jackin-core"].to_string_lossy(),
        "/repo/crates/jackin-core/src/lib.rs"
    );
}
