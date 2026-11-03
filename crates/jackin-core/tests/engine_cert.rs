// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{CaFingerprint, ENGINE_CERT_MAX_BYTES, EngineCert, EngineCertProblem, FileMode};

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

const EC_PARAMETERS: &str =
    "-----BEGIN EC PARAMETERS-----\nBggqhkjOPQMBBw==\n-----END EC PARAMETERS-----\n";
const EC_KEY_BODY: &str = "MHcCAQEEIBEsxiHjujJq+KSj8xsMQ++A5xUgSZyfVcEUpXJDxgxkoAoGCCqGSM49\nAwEHoUQDQgAEb3SWStGuM0pTTnG9Xw/xNBqz6ZFWBqXYs9+IKIM+IZ7SrRCtwtUT\nBZdbV9oZSMVCKmWnYEN8f/BX2STCTwJJXQ==\n";
const KEY_WORDS: &str = "PRIVATE KEY";

fn ec_key() -> String {
    format!("-----BEGIN EC {KEY_WORDS}-----\n{EC_KEY_BODY}-----END EC {KEY_WORDS}-----\n")
}

fn ecparam_genkey_output() -> String {
    format!("{EC_PARAMETERS}{}", ec_key())
}

#[test]
fn key_check_accepts_parameter_blocks_before_or_after_a_key_as_the_docker_cli_does() {
    let key = ec_key();
    let dh = block("DH PARAMETERS");
    let other_key = block("PRIVATE KEY");
    let accepted = [
        ("openssl ecparam -genkey output", ecparam_genkey_output()),
        ("parameters after the key", format!("{key}{EC_PARAMETERS}")),
        (
            "parameters on both sides",
            format!("{EC_PARAMETERS}{key}{dh}"),
        ),
        ("dh parameters before the key", format!("{dh}{key}")),
        ("two parameter blocks", format!("{EC_PARAMETERS}{dh}{key}")),
        ("two key blocks", format!("{EC_PARAMETERS}{key}{other_key}")),
    ];
    for (label, text) in accepted {
        assert_eq!(EngineCert::Key.check(text.as_bytes()), Ok(()), "{label}");
    }
}

#[test]
fn key_check_still_refuses_parameters_without_a_usable_key_and_foreign_blocks() {
    let key = ec_key();
    let cert = block("CERTIFICATE");
    let refused = [
        (
            "ec parameters only",
            EngineCert::Key,
            EC_PARAMETERS.to_owned(),
            EngineCertProblem::WrongKind,
        ),
        (
            "dh parameters only",
            EngineCert::Key,
            block("DH PARAMETERS"),
            EngineCertProblem::WrongKind,
        ),
        (
            "parameters then an encrypted key",
            EngineCert::Key,
            format!("{EC_PARAMETERS}{}", block("ENCRYPTED PRIVATE KEY")),
            EngineCertProblem::EncryptedKey,
        ),
        (
            "parameters, a key and a certificate",
            EngineCert::Key,
            format!("{EC_PARAMETERS}{key}{cert}"),
            EngineCertProblem::WrongKind,
        ),
        (
            "parameters and a certificate",
            EngineCert::Key,
            format!("{EC_PARAMETERS}{cert}"),
            EngineCertProblem::WrongKind,
        ),
        (
            "a key file in the certificate slot",
            EngineCert::Cert,
            ecparam_genkey_output(),
            EngineCertProblem::WrongKind,
        ),
        (
            "parameters before a certificate in the ca slot",
            EngineCert::Ca,
            format!("{EC_PARAMETERS}{cert}"),
            EngineCertProblem::WrongKind,
        ),
        (
            "parameters only in the certificate slot",
            EngineCert::Cert,
            EC_PARAMETERS.to_owned(),
            EngineCertProblem::WrongKind,
        ),
    ];
    for (label, slot, text, expected) in refused {
        let result = slot.check(text.as_bytes());
        assert_eq!(result, Err(expected), "{label}");
        let problem = expected;
        assert!(!format!("{problem} {problem:?}").contains(EC_KEY_BODY.trim_end()));
    }
}

const CA_A: &str = "-----BEGIN CERTIFICATE-----
MIIBajCCARCgAwIBAgIUEt2JRLpXbgq1YRt3L4vCkNqEQn0wCgYIKoZIzj0EAwIw
GzEZMBcGA1UEAwwQamFja2luIHRlc3QgY2EgYTAeFw0yNjExMDMwNDI4MzFaFw0z
NjEwMzEwNDI4MzFaMBsxGTAXBgNVBAMMEGphY2tpbiB0ZXN0IGNhIGEwWTATBgcq
hkjOPQIBBggqhkjOPQMBBwNCAARWjd/jb7QJeTfcSBqcRv8UvydmT5HtZAlWNscC
xPOru7G2hnuQGhTb2vaYiHFgCVSG+gMJSCemdd+g6kFnyzq9ozIwMDAdBgNVHQ4E
FgQU4qrxVTiR7hEU6/eBABkH+xrXfHIwDwYDVR0TAQH/BAUwAwEB/zAKBggqhkjO
PQQDAgNIADBFAiEA7BlzixFqRkgq11svPEuObt9GNqVm2jjHq1S+A2mu4uMCIH8C
VbmFn/JXj3GjVI13F8s3zR0iUg1gplRLpdNqIgOj
-----END CERTIFICATE-----
";
const CA_B: &str = "-----BEGIN CERTIFICATE-----
MIIBaTCCARCgAwIBAgIURQxZor2lki0TTRCYjSLh2fzm5AMwCgYIKoZIzj0EAwIw
GzEZMBcGA1UEAwwQamFja2luIHRlc3QgY2EgYjAeFw0yNjExMDMwNDI4MzFaFw0z
NjEwMzEwNDI4MzFaMBsxGTAXBgNVBAMMEGphY2tpbiB0ZXN0IGNhIGIwWTATBgcq
hkjOPQIBBggqhkjOPQMBBwNCAAS2oxUawVSjXMLH+J3qIMdz4s5Ogpl+hdVUJGEy
RQjREr23nl+WDVSJl734DukOqKh2gVaDqudfffgsYk1ecy1PozIwMDAdBgNVHQ4E
FgQUbFFMMJxx7TZG4HvjQuuioiUkz2cwDwYDVR0TAQH/BAUwAwEB/zAKBggqhkjO
PQQDAgNHADBEAiBkNeH9G/JgV1VKGgYLpmL5ISkClCWvGM5nRQgDxWBcKQIgfhT1
mrQsW+DQ6gJbuvoLVih9iM1WIZ5NC6I6rknfNcU=
-----END CERTIFICATE-----
";

fn fingerprint(text: &str) -> Result<String, EngineCertProblem> {
    CaFingerprint::of_ca(text.as_bytes()).map(|found| found.to_string())
}

#[test]
fn the_ca_fingerprint_is_the_first_sixteen_hex_digits_of_sha256_over_the_sorted_der_certificates() {
    assert_eq!(fingerprint(CA_A).as_deref(), Ok("9e3e927f7dacec2d"));
    assert_eq!(fingerprint(CA_B).as_deref(), Ok("193844f866f10c22"));
    let bundle = fingerprint(&format!("{CA_A}{CA_B}"));
    assert_eq!(bundle.as_deref(), Ok("181a3ddb73719372"));
    assert_eq!(fingerprint(&format!("{CA_B}{CA_A}")), bundle);
    assert_eq!(fingerprint(&format!("{CA_A}{CA_A}")), fingerprint(CA_A));
}

#[test]
fn the_ca_fingerprint_ignores_line_endings_blank_lines_and_text_between_blocks() {
    let crlf = format!("{CA_A}\n\n").replace('\n', "\r\n");
    assert_eq!(fingerprint(&crlf), fingerprint(CA_A));
    let padded = format!("\n{CA_A}\n\n{CA_A}");
    assert_eq!(fingerprint(&padded), fingerprint(CA_A));
    let moved = format!("{CA_A}{CA_A}").replace(
        "-----END CERTIFICATE-----\n",
        "-----END CERTIFICATE-----   \n",
    );
    assert_eq!(fingerprint(&moved), fingerprint(CA_A));
}

#[test]
fn the_ca_fingerprint_refuses_what_is_not_a_decodable_certificate_bundle() {
    let truncated = "-----BEGIN CERTIFICATE-----\nAAA\n-----END CERTIFICATE-----\n";
    let cases = [
        ("empty", String::new(), EngineCertProblem::Empty),
        (
            "not base64 length",
            truncated.to_owned(),
            EngineCertProblem::NotPem,
        ),
        ("a key", ec_key(), EngineCertProblem::WrongKind),
        ("text", "hello".to_owned(), EngineCertProblem::NotPem),
    ];
    for (label, text, expected) in cases {
        assert_eq!(fingerprint(&text), Err(expected), "{label}");
    }
}

#[test]
fn a_ca_fingerprint_round_trips_through_text_and_json_and_refuses_other_spellings() {
    let found = CaFingerprint::of_ca(CA_A.as_bytes()).unwrap();
    assert_eq!(CaFingerprint::parse(&found.to_string()), Some(found));
    assert_eq!(
        serde_json::to_string(&found).unwrap(),
        "\"9e3e927f7dacec2d\""
    );
    for bad in [
        "",
        "9E3E927F7DACEC2D",
        "9e3e927f7dacec2",
        "9e3e927f7dacec2d0",
        "9e3e927f7dacec2g",
        "+e3e927f7dacec2d",
    ] {
        assert_eq!(CaFingerprint::parse(bad), None, "{bad}");
    }
}
