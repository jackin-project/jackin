// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use jackin_core::{ENGINE_CERT_MAX_BYTES, EngineCert, EngineCertProblem};
use jackin_testkit::{EngineCertificateSet, EngineKeyKind, pem_body};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const EC_PARAMETERS: &str =
    "-----BEGIN EC PARAMETERS-----\nBggqhkjOPQMBBw==\n-----END EC PARAMETERS-----\n";

fn located(cert: &str, key: &str) -> Result<(), (EngineCert, EngineCertProblem)> {
    EngineCert::check_pair(cert.as_bytes(), key.as_bytes())
        .map_err(|found| (found.file, found.problem))
}

fn pair(cert: &str, key: &str) -> Result<(), EngineCertProblem> {
    located(cert, key).map_err(|(_, problem)| problem)
}

fn armored(label: &str, der: &[u8]) -> String {
    format!(
        "-----BEGIN {label}-----\n{}\n-----END {label}-----\n",
        STANDARD.encode(der)
    )
}

fn der_of(pem: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(STANDARD.decode(pem_body(pem))?)
}

fn generate(kind: EngineKeyKind) -> Result<EngineCertificateSet, Box<dyn std::error::Error>> {
    EngineCertificateSet::generate_for(kind)
}

#[test]
fn a_certificate_and_its_own_key_pair_up_for_every_supported_key_type() -> TestResult {
    for kind in EngineKeyKind::ALL {
        let set = generate(kind)?;
        assert_eq!(pair(&set.cert, &set.key), Ok(()), "{kind:?}");
    }
    Ok(())
}

#[test]
fn the_traditional_rsa_and_ec_key_encodings_pair_up_too() -> TestResult {
    for kind in [
        EngineKeyKind::Rsa,
        EngineKeyKind::EcdsaP256,
        EngineKeyKind::EcdsaP384,
    ] {
        let set = generate(kind)?;
        let traditional = set.traditional_key()?;
        assert_ne!(traditional, set.key);
        assert_eq!(pair(&set.cert, &traditional), Ok(()), "{kind:?}");
    }
    Ok(())
}

#[test]
fn the_pem_label_never_decides_which_key_encoding_is_read() -> TestResult {
    let ec = generate(EngineKeyKind::EcdsaP256)?;
    let pkcs8 = der_of(&ec.key)?;
    let sec1 = der_of(&ec.traditional_key()?)?;
    for (label, der) in [
        ("RSA PRIVATE KEY", &pkcs8),
        ("EC PRIVATE KEY", &pkcs8),
        ("PRIVATE KEY", &sec1),
        ("RSA PRIVATE KEY", &sec1),
    ] {
        assert_eq!(pair(&ec.cert, &armored(label, der)), Ok(()), "{label}");
    }
    let rsa = generate(EngineKeyKind::Rsa)?;
    let pkcs8 = der_of(&rsa.key)?;
    let pkcs1 = der_of(&rsa.traditional_key()?)?;
    for (label, der) in [("EC PRIVATE KEY", &pkcs8), ("PRIVATE KEY", &pkcs1)] {
        assert_eq!(pair(&rsa.cert, &armored(label, der)), Ok(()), "{label}");
    }
    Ok(())
}

#[test]
fn a_key_of_another_certificate_never_pairs_up_whatever_the_key_types() -> TestResult {
    let sets: Vec<(EngineKeyKind, EngineCertificateSet)> = EngineKeyKind::ALL
        .into_iter()
        .flat_map(|kind| [kind, kind])
        .map(|kind| Ok((kind, generate(kind)?)))
        .collect::<Result<_, Box<dyn std::error::Error>>>()?;
    for (index, (cert_kind, owner)) in sets.iter().enumerate() {
        for (other_index, (key_kind, stranger)) in sets.iter().enumerate() {
            if index == other_index {
                continue;
            }
            assert_eq!(
                located(&owner.cert, &stranger.key),
                Err((EngineCert::Key, EngineCertProblem::PairMismatch)),
                "{cert_kind:?} certificate with a {key_kind:?} key"
            );
        }
    }
    Ok(())
}

#[test]
fn a_key_that_claims_the_public_key_of_another_certificate_never_pairs_up() -> TestResult {
    let mine = generate(EngineKeyKind::EcdsaP256)?;
    let theirs = generate(EngineKeyKind::EcdsaP256)?;
    let lying = mine.key_claiming_the_public_key_of(&theirs)?;
    assert!(
        pair(&theirs.cert, &lying).is_err(),
        "the embedded public key was trusted"
    );
    Ok(())
}

#[test]
fn a_certificate_that_names_another_curve_around_the_same_point_never_pairs_up() -> TestResult {
    let set = generate(EngineKeyKind::EcdsaP256)?;
    let mut der = der_of(&set.cert)?;
    let p256 = [0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07];
    let at = der
        .windows(p256.len())
        .position(|window| window == p256)
        .ok_or("no P-256 curve identifier")?;
    der[at + p256.len() - 1] = 0x01;
    assert_eq!(
        pair(&armored("CERTIFICATE", &der), &set.key),
        Err(EngineCertProblem::PairMismatch)
    );
    assert_eq!(pair(&set.cert, &set.key), Ok(()));
    Ok(())
}

#[test]
fn only_the_first_certificate_is_the_client_certificate() -> TestResult {
    let set = EngineCertificateSet::generate()?;
    assert_eq!(pair(&format!("{}{}", set.cert, set.ca), &set.key), Ok(()));
    assert_eq!(
        pair(&format!("{}{}", set.ca, set.cert), &set.key),
        Err(EngineCertProblem::PairMismatch)
    );
    Ok(())
}

#[test]
fn only_the_first_private_key_block_is_the_key() -> TestResult {
    let mine = EngineCertificateSet::generate()?;
    let other = EngineCertificateSet::generate()?;
    assert_eq!(
        pair(&mine.cert, &format!("{}{}", mine.key, other.key)),
        Ok(())
    );
    assert_eq!(
        pair(&mine.cert, &format!("{}{}", other.key, mine.key)),
        Err(EngineCertProblem::PairMismatch)
    );
    assert_eq!(
        pair(&mine.cert, &format!("{EC_PARAMETERS}{}", mine.key)),
        Ok(())
    );
    Ok(())
}

#[test]
fn blocks_after_the_first_are_never_decoded() -> TestResult {
    let mine = EngineCertificateSet::generate()?;
    let junk_certificate = armored("CERTIFICATE", b"decodes as base64 but is not a certificate");
    let junk_key = armored("PRIVATE KEY", b"decodes as base64 but is not a key");
    assert_eq!(
        pair(&format!("{}{junk_certificate}", mine.cert), &mine.key),
        Ok(())
    );
    assert_eq!(pair(&mine.cert, &format!("{}{junk_key}", mine.key)), Ok(()));
    Ok(())
}

#[test]
fn a_junk_first_block_is_refused_even_when_a_good_one_follows() -> TestResult {
    let mine = EngineCertificateSet::generate()?;
    let junk_certificate = armored("CERTIFICATE", b"decodes as base64 but is not a certificate");
    let junk_key = armored("PRIVATE KEY", b"decodes as base64 but is not a key");
    assert_eq!(
        located(&format!("{junk_certificate}{}", mine.cert), &mine.key),
        Err((EngineCert::Cert, EngineCertProblem::MalformedCertificate))
    );
    assert_eq!(
        located(&mine.cert, &format!("{junk_key}{}", mine.key)),
        Err((EngineCert::Key, EngineCertProblem::MalformedKey))
    );
    Ok(())
}

#[test]
fn an_ec_parameters_block_before_a_sec1_key_pairs_up() -> TestResult {
    let set = EngineCertificateSet::generate()?;
    let sec1 = set.traditional_key()?;
    assert_eq!(pair(&set.cert, &format!("{EC_PARAMETERS}{sec1}")), Ok(()));
    Ok(())
}

#[test]
fn line_endings_wrapping_and_text_around_the_blocks_do_not_matter() -> TestResult {
    let set = EngineCertificateSet::generate()?;
    let rewrapped = |pem: &str, width: usize| -> String {
        let label = pem
            .lines()
            .next()
            .unwrap_or_default()
            .trim_start_matches("-----BEGIN ")
            .trim_end_matches("-----")
            .to_owned();
        let body = pem_body(pem);
        let mut text = format!("-----BEGIN {label}-----\n");
        for chunk in body.as_bytes().chunks(width) {
            text.push_str(&String::from_utf8_lossy(chunk));
            text.push('\n');
        }
        text.push_str(&format!("-----END {label}-----\n"));
        text
    };
    let cert = format!("subject=/CN=client\n{}\n", rewrapped(&set.cert, 76));
    let key = format!("Bag Attributes\n{}\n", rewrapped(&set.key, 76));
    assert_eq!(pair(&cert, &key), Ok(()));
    let crlf = |text: &str| text.replace('\n', "\r\n");
    assert_eq!(pair(&crlf(&set.cert), &crlf(&set.key)), Ok(()));
    assert_eq!(
        pair(&rewrapped(&set.cert, 20), &rewrapped(&set.key, 100)),
        Ok(())
    );
    Ok(())
}

#[test]
fn a_version_one_certificate_pairs_up_by_its_public_key_alone() -> TestResult {
    let set = EngineCertificateSet::generate()?;
    let v1 = EngineCertificateSet::version_one_certificate_for_key(&set.key)?;
    assert_eq!(pair(&v1, &set.key), Ok(()));
    let other = EngineCertificateSet::generate()?;
    assert_eq!(pair(&v1, &other.key), Err(EngineCertProblem::PairMismatch));
    Ok(())
}

#[test]
fn certificate_extensions_never_decide_the_pairing() -> TestResult {
    let set = EngineCertificateSet::generate()?;
    let extended = EngineCertificateSet::certificate_with_unknown_extensions_for_key(&set.key)?;
    assert_eq!(pair(&extended, &set.key), Ok(()));
    let other = EngineCertificateSet::generate()?;
    assert_eq!(
        pair(&extended, &other.key),
        Err(EngineCertProblem::PairMismatch)
    );
    Ok(())
}

#[test]
fn keys_that_leave_out_the_optional_public_key_still_pair_by_the_derived_public_key() -> TestResult
{
    for kind in [
        EngineKeyKind::Ed25519,
        EngineKeyKind::EcdsaP256,
        EngineKeyKind::EcdsaP384,
    ] {
        let set = generate(kind)?;
        let bare = set.bare_key()?;
        assert_ne!(bare, set.key, "{kind:?}");
        assert_eq!(pair(&set.cert, &bare), Ok(()), "{kind:?}");
        let other = generate(kind)?;
        assert_eq!(
            pair(&other.cert, &bare),
            Err(EngineCertProblem::PairMismatch),
            "{kind:?}"
        );
    }
    Ok(())
}

#[test]
fn well_formed_pem_around_broken_der_names_the_file_that_holds_it() -> TestResult {
    let set = EngineCertificateSet::generate()?;
    let cert_der = der_of(&set.cert)?;
    let key_der = der_of(&set.key)?;
    let truncated_cert = armored("CERTIFICATE", &cert_der[..cert_der.len() / 2]);
    let trailing = armored("CERTIFICATE", &[cert_der.as_slice(), &[0, 0, 0]].concat());
    let garbage_cert = armored("CERTIFICATE", b"not a certificate at all, just text");
    let truncated_key = armored("PRIVATE KEY", &key_der[..key_der.len() / 2]);
    let garbage_key = armored("PRIVATE KEY", b"not a private key at all, just text");
    let unsupported = EngineCertificateSet::unsupported_key();
    let cert_fault = (EngineCert::Cert, EngineCertProblem::MalformedCertificate);
    let key_fault = (EngineCert::Key, EngineCertProblem::MalformedKey);

    for broken in [&truncated_cert, &garbage_cert, &trailing] {
        assert_eq!(located(broken, &set.key), Err(cert_fault));
    }
    for broken in [&truncated_key, &garbage_key, &unsupported] {
        assert_eq!(located(&set.cert, broken), Err(key_fault));
    }
    Ok(())
}

#[test]
fn a_block_that_is_not_base64_names_the_file_that_holds_it() -> TestResult {
    let set = EngineCertificateSet::generate()?;
    let bad_padding =
        |label: &str| format!("-----BEGIN {label}-----\nQUJD=\n-----END {label}-----\n");
    assert_eq!(
        located(&bad_padding("CERTIFICATE"), &set.key),
        Err((EngineCert::Cert, EngineCertProblem::NotPem))
    );
    assert_eq!(
        located(&set.cert, &bad_padding("PRIVATE KEY")),
        Err((EngineCert::Key, EngineCertProblem::NotPem))
    );
    Ok(())
}

#[test]
fn the_structural_refusals_come_first_and_name_the_file_that_holds_them() -> TestResult {
    let set = EngineCertificateSet::generate()?;
    let encrypted = armored("ENCRYPTED PRIVATE KEY", b"anything");
    let oversized = format!("{}{}", set.cert, " ".repeat(ENGINE_CERT_MAX_BYTES));
    let cases = [
        (
            set.cert.clone(),
            encrypted,
            (EngineCert::Key, EngineCertProblem::EncryptedKey),
        ),
        (
            set.key.clone(),
            set.key.clone(),
            (EngineCert::Cert, EngineCertProblem::WrongKind),
        ),
        (
            set.cert.clone(),
            set.cert.clone(),
            (EngineCert::Key, EngineCertProblem::WrongKind),
        ),
        (
            String::new(),
            set.key.clone(),
            (EngineCert::Cert, EngineCertProblem::Empty),
        ),
        (
            set.cert.clone(),
            String::new(),
            (EngineCert::Key, EngineCertProblem::Empty),
        ),
        (
            oversized,
            set.key.clone(),
            (EngineCert::Cert, EngineCertProblem::TooLarge),
        ),
        (
            "hello".to_owned(),
            set.key.clone(),
            (EngineCert::Cert, EngineCertProblem::NotPem),
        ),
    ];
    for (cert, key, expected) in cases {
        assert_eq!(located(&cert, &key), Err(expected));
    }
    Ok(())
}
