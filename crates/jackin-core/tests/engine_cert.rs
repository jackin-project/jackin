// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{ENGINE_CERT_MAX_BYTES, EngineCert, EngineCertProblem, FileMode};

const MARKER: &str = "MARKERQkZ0VTVC9zZWNyZXQ";

fn block(label: &str) -> String {
    format!("-----BEGIN {label}-----\n{MARKER}\nAAAA\n-----END {label}-----\n")
}

fn padded(label: &str, total: usize) -> Vec<u8> {
    let head = format!("-----BEGIN {label}-----\n");
    let tail = format!("-----END {label}-----\n");
    let body = total - head.len() - tail.len();
    let mut text = head;
    let mut left = body;
    while left > 0 {
        let line = left.min(65);
        text.push_str(&"A".repeat(line - 1));
        text.push('\n');
        left -= line;
    }
    text.push_str(&tail);
    text.into_bytes()
}

#[test]
fn the_table_names_three_files_with_the_documented_modes_under_the_docker_directory() {
    let names: Vec<&str> = EngineCert::ALL.map(EngineCert::file_name).to_vec();
    assert_eq!(names, ["ca.pem", "cert.pem", "key.pem"]);
    let modes: Vec<FileMode> = EngineCert::ALL.map(EngineCert::mode).to_vec();
    assert_eq!(modes, [FileMode(0o644), FileMode(0o644), FileMode(0o600)]);
    for cert in EngineCert::ALL {
        let path = cert.sandbox_path();
        assert_eq!(
            path.as_str(),
            format!("/sandbox/.jackin/docker/{}", cert.file_name())
        );
    }
}

#[test]
fn pem_check_accepts_the_right_kind_in_each_slot_up_to_64_kib() {
    for slot in [EngineCert::Ca, EngineCert::Cert] {
        assert_eq!(slot.check(block("CERTIFICATE").as_bytes()), Ok(()));
        let chain = format!("{}{}", block("CERTIFICATE"), block("CERTIFICATE"));
        assert_eq!(slot.check(chain.as_bytes()), Ok(()));
    }
    for label in ["PRIVATE KEY", "RSA PRIVATE KEY", "EC PRIVATE KEY"] {
        assert_eq!(
            EngineCert::Key.check(block(label).as_bytes()),
            Ok(()),
            "{label}"
        );
    }
    let exact = padded("CERTIFICATE", ENGINE_CERT_MAX_BYTES);
    assert_eq!(exact.len(), 65536);
    assert_eq!(EngineCert::Cert.check(&exact), Ok(()));
}

#[test]
fn pem_check_refuses_everything_else_without_echoing_the_content() {
    let cert = block("CERTIFICATE");
    let key = block("PRIVATE KEY");
    let unterminated = format!("-----BEGIN PRIVATE KEY-----\n{MARKER}\n");
    let non_utf8 = [0xff, 0xfe, 0x00];
    let big = padded("CERTIFICATE", ENGINE_CERT_MAX_BYTES + 1);
    assert_eq!(big.len(), 65537);
    let cases: Vec<(EngineCert, Vec<u8>, EngineCertProblem)> = vec![
        (EngineCert::Cert, vec![], EngineCertProblem::Empty),
        (EngineCert::Cert, big, EngineCertProblem::TooLarge),
        (
            EngineCert::Cert,
            non_utf8.to_vec(),
            EngineCertProblem::NotPem,
        ),
        (
            EngineCert::Cert,
            MARKER.as_bytes().to_vec(),
            EngineCertProblem::NotPem,
        ),
        (
            EngineCert::Key,
            unterminated.into_bytes(),
            EngineCertProblem::NotPem,
        ),
        (
            EngineCert::Key,
            cert.clone().into_bytes(),
            EngineCertProblem::WrongKind,
        ),
        (
            EngineCert::Cert,
            key.clone().into_bytes(),
            EngineCertProblem::WrongKind,
        ),
        (
            EngineCert::Cert,
            format!("{cert}{key}").into_bytes(),
            EngineCertProblem::WrongKind,
        ),
        (
            EngineCert::Key,
            format!("{key}{cert}").into_bytes(),
            EngineCertProblem::WrongKind,
        ),
        (
            EngineCert::Cert,
            format!("{cert}-----BEGIN PRIVATE KEY-----\n{MARKER}\n").into_bytes(),
            EngineCertProblem::NotPem,
        ),
        (
            EngineCert::Key,
            block("ENCRYPTED PRIVATE KEY").into_bytes(),
            EngineCertProblem::EncryptedKey,
        ),
    ];
    for (slot, bytes, expected) in cases {
        let result = slot.check(&bytes);
        assert_eq!(result, Err(expected), "{slot:?}");
        let problem = expected;
        assert!(!format!("{problem} {problem:?}").contains(MARKER));
    }
}
