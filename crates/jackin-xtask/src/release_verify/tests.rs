use super::*;

#[test]
fn accepts_matching_sha256_sidecar() {
    let temp = tempfile::tempdir().unwrap();
    let archive = temp.path().join("artifact.tar.gz");
    let sidecar = sibling_with_suffix(&archive, "sha256");
    fs::write(&archive, b"release artifact").unwrap();
    fs::write(
        &sidecar,
        "133cfccb5b503cf4040c95f3dfad56d07c1574283a1e39066b594f6ee33711ba\n",
    )
    .unwrap();

    verify_sha256_file(&archive, &sidecar).unwrap();
}

#[test]
fn rejects_tampered_archive() {
    let temp = tempfile::tempdir().unwrap();
    let archive = temp.path().join("artifact.tar.gz");
    let sidecar = sibling_with_suffix(&archive, "sha256");
    fs::write(&archive, b"release artifact").unwrap();
    fs::write(
        &sidecar,
        "133cfccb5b503cf4040c95f3dfad56d07c1574283a1e39066b594f6ee33711ba\n",
    )
    .unwrap();
    fs::write(&archive, b"release artifact!").unwrap();

    let err = verify_sha256_file(&archive, &sidecar)
        .expect_err("tampered archive should fail the digest check");
    assert!(err.to_string().contains("sha256 mismatch"));
}

#[test]
fn parses_sha256_sidecar_with_filename() {
    let temp = tempfile::tempdir().unwrap();
    let sidecar = temp.path().join("artifact.tar.gz.sha256");
    fs::write(
        &sidecar,
        "3de369ca6af574307c46108c1eb59e7a77a2e3e8f84d94993076504ba48f4760  artifact.tar.gz\n",
    )
    .unwrap();

    assert_eq!(
        read_expected_sha256(&sidecar).unwrap(),
        "3de369ca6af574307c46108c1eb59e7a77a2e3e8f84d94993076504ba48f4760"
    );
}

#[test]
fn sibling_suffix_preserves_archive_extension() {
    assert_eq!(
        sibling_with_suffix(Path::new("jackin.tar.gz"), "bundle"),
        PathBuf::from("jackin.tar.gz.bundle")
    );
}
