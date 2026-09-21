// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn valid_document(archive: &Path) -> Value {
    json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.7",
        "serialNumber": "urn:uuid:12345678-1234-1234-1234-123456789abc",
        "metadata": {
            "component": {
                "type": "file",
                "name": archive.file_name().unwrap().to_str().unwrap(),
                "properties": [
                    {
                        "name": ARCHIVE_DIGEST_PROPERTY,
                        "value": archive_sha256(archive).unwrap(),
                    },
                    {
                        "name": ARCHIVE_NAME_PROPERTY,
                        "value": archive.file_name().unwrap().to_str().unwrap(),
                    },
                ],
            },
        },
        "components": [{"type": "library", "name": "example"}],
    })
}

#[test]
fn rejects_empty_shape() {
    let temp = tempfile::tempdir().unwrap();
    let archive = temp.path().join("artifact.tar.gz");
    let sbom = temp.path().join("artifact.tar.gz.sbom.json");
    fs::write(&archive, b"archive").unwrap();
    fs::write(&sbom, b"{}").unwrap();

    let error = verify(&archive, &sbom).expect_err("empty SBOM shape must fail");
    assert!(error.to_string().contains("bomFormat"));
}

#[test]
fn accepts_bound_cyclonedx_document() {
    let temp = tempfile::tempdir().unwrap();
    let archive = temp.path().join("artifact.tar.gz");
    let sbom = temp.path().join("artifact.tar.gz.sbom.json");
    fs::write(&archive, b"archive").unwrap();
    fs::write(
        &sbom,
        serde_json::to_vec(&valid_document(&archive)).unwrap(),
    )
    .unwrap();

    verify(&archive, &sbom).unwrap();
}

#[test]
fn rejects_archive_digest_mismatch() {
    let temp = tempfile::tempdir().unwrap();
    let archive = temp.path().join("artifact.tar.gz");
    let sbom = temp.path().join("artifact.tar.gz.sbom.json");
    fs::write(&archive, b"archive").unwrap();
    let mut document = valid_document(&archive);
    document["metadata"]["component"]["properties"][0]["value"] = Value::String("0".repeat(64));
    fs::write(&sbom, serde_json::to_vec(&document).unwrap()).unwrap();

    let error = verify(&archive, &sbom).expect_err("mismatched archive digest must fail");
    assert!(error.to_string().contains("digest binding"));
}

#[test]
fn rejects_archive_name_mismatch() {
    let temp = tempfile::tempdir().unwrap();
    let archive = temp.path().join("artifact.tar.gz");
    let sbom = temp.path().join("artifact.tar.gz.sbom.json");
    fs::write(&archive, b"archive").unwrap();
    let mut document = valid_document(&archive);
    document["metadata"]["component"]["name"] = Value::String("other.tar.gz".to_owned());
    fs::write(&sbom, serde_json::to_vec(&document).unwrap()).unwrap();

    let error = verify(&archive, &sbom).expect_err("mismatched archive name must fail");
    assert!(error.to_string().contains("component.name"));
}
