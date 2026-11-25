// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use rustls::crypto::aws_lc_rs::sign::any_supported_type;
use rustls::pki_types::{PrivateKeyDer, PrivatePkcs1KeyDer, PrivatePkcs8KeyDer, PrivateSec1KeyDer};
use rustls::sign::SigningKey;
use x509_parser::certificate::X509CertificateParser;
use x509_parser::nom::Parser as _;
use x509_parser::prelude::FromDer as _;
use x509_parser::x509::{AlgorithmIdentifier, SubjectPublicKeyInfo};
use zeroize::Zeroizing;

use super::{EngineCert, EngineCertProblem, EnginePairProblem, KEY_LABELS, pem_blocks};

fn fault(file: EngineCert, problem: EngineCertProblem) -> EnginePairProblem {
    EnginePairProblem { file, problem }
}

fn decode_secret(text: &str) -> Option<Zeroizing<Vec<u8>>> {
    let mut buffer = Zeroizing::new(vec![0_u8; base64::decoded_len_estimate(text.len())]);
    let written = STANDARD.decode_slice(text.as_bytes(), &mut buffer).ok()?;
    buffer.truncate(written);
    Some(buffer)
}

fn first_der(bytes: &[u8], labels: &[&str]) -> Result<Zeroizing<Vec<u8>>, EngineCertProblem> {
    let text = std::str::from_utf8(bytes).map_err(|_| EngineCertProblem::NotPem)?;
    let block = pem_blocks(text)?
        .into_iter()
        .find(|block| labels.contains(&block.label))
        .ok_or(EngineCertProblem::NotPem)?;
    let body = Zeroizing::new(block.body.concat());
    decode_secret(&body).ok_or(EngineCertProblem::NotPem)
}

fn signing_key(der: &[u8]) -> Option<Arc<dyn SigningKey>> {
    [
        PrivateKeyDer::from(PrivatePkcs1KeyDer::from(der)),
        PrivateKeyDer::from(PrivatePkcs8KeyDer::from(der)),
        PrivateKeyDer::from(PrivateSec1KeyDer::from(der)),
    ]
    .iter()
    .find_map(|candidate| any_supported_type(candidate).ok())
}

fn curve<'a>(algorithm: &'a AlgorithmIdentifier<'a>) -> &'a [u8] {
    match &algorithm.parameters {
        Some(parameters) => parameters.data,
        None => &[],
    }
}

pub(super) fn check(cert: &[u8], key: &[u8]) -> Result<(), EnginePairProblem> {
    EngineCert::Cert
        .check(cert)
        .map_err(|problem| fault(EngineCert::Cert, problem))?;
    EngineCert::Key
        .check(key)
        .map_err(|problem| fault(EngineCert::Key, problem))?;
    let leaf =
        first_der(cert, &["CERTIFICATE"]).map_err(|problem| fault(EngineCert::Cert, problem))?;
    let key_der = first_der(key, &KEY_LABELS).map_err(|problem| fault(EngineCert::Key, problem))?;
    let (rest, certificate) = X509CertificateParser::new()
        .with_deep_parse_extensions(false)
        .parse(&leaf)
        .map_err(|_| fault(EngineCert::Cert, EngineCertProblem::MalformedCertificate))?;
    if !rest.is_empty() {
        return Err(fault(
            EngineCert::Cert,
            EngineCertProblem::MalformedCertificate,
        ));
    }
    let malformed_key = || fault(EngineCert::Key, EngineCertProblem::MalformedKey);
    let signer = signing_key(&key_der).ok_or_else(malformed_key)?;
    let public = signer.public_key().ok_or_else(malformed_key)?;
    let (_, derived) =
        SubjectPublicKeyInfo::from_der(public.as_ref()).map_err(|_| malformed_key())?;
    let held = certificate.public_key();
    if derived.algorithm.algorithm == held.algorithm.algorithm
        && curve(&derived.algorithm) == curve(&held.algorithm)
        && derived.subject_public_key.data == held.subject_public_key.data
    {
        Ok(())
    } else {
        Err(fault(EngineCert::Key, EngineCertProblem::PairMismatch))
    }
}
