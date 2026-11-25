// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use jackin_core::EngineCert;
use rcgen::{
    BasicConstraints, CertificateParams, CustomExtension, DistinguishedName, DnType,
    ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair, KeyUsagePurpose, PKCS_ECDSA_P256_SHA256,
    PKCS_ECDSA_P384_SHA384, PKCS_ED25519, PKCS_RSA_SHA256, PublicKeyData, RsaKeySize,
};

use crate::fixtures::FixtureError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineKeyKind {
    Rsa,
    EcdsaP256,
    EcdsaP384,
    Ed25519,
}

impl EngineKeyKind {
    pub const ALL: [EngineKeyKind; 4] = [
        EngineKeyKind::Rsa,
        EngineKeyKind::EcdsaP256,
        EngineKeyKind::EcdsaP384,
        EngineKeyKind::Ed25519,
    ];

    fn generate(self) -> Result<KeyPair, FixtureError> {
        Ok(match self {
            EngineKeyKind::Rsa => KeyPair::generate_rsa_for(&PKCS_RSA_SHA256, RsaKeySize::_2048)?,
            EngineKeyKind::EcdsaP256 => KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256)?,
            EngineKeyKind::EcdsaP384 => KeyPair::generate_for(&PKCS_ECDSA_P384_SHA384)?,
            EngineKeyKind::Ed25519 => KeyPair::generate_for(&PKCS_ED25519)?,
        })
    }

    fn traditional_label(self) -> Option<&'static str> {
        match self {
            EngineKeyKind::Rsa => Some("RSA PRIVATE KEY"),
            EngineKeyKind::EcdsaP256 | EngineKeyKind::EcdsaP384 => Some("EC PRIVATE KEY"),
            EngineKeyKind::Ed25519 => None,
        }
    }
}

pub struct EngineCertificateSet {
    pub kind: EngineKeyKind,
    pub ca: String,
    pub cert: String,
    pub key: String,
}

impl std::fmt::Debug for EngineCertificateSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EngineCertificateSet(<redacted>)")
    }
}

fn named(common_name: &str) -> DistinguishedName {
    let mut name = DistinguishedName::new();
    name.push(DnType::CommonName, common_name);
    name
}

impl EngineCertificateSet {
    pub fn generate() -> Result<EngineCertificateSet, FixtureError> {
        EngineCertificateSet::generate_for(EngineKeyKind::EcdsaP256)
    }

    pub fn generate_for(kind: EngineKeyKind) -> Result<EngineCertificateSet, FixtureError> {
        let ca_key = EngineKeyKind::EcdsaP256.generate()?;
        let mut ca_params = CertificateParams::new(Vec::new())?;
        ca_params.distinguished_name = named("jackin test engine ca");
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign];
        let ca = ca_params.self_signed(&ca_key)?;
        let key = kind.generate()?;
        let mut params = CertificateParams::new(Vec::new())?;
        params.distinguished_name = named("jackin test engine client");
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
        let issuer = Issuer::from_params(&ca_params, &ca_key);
        let cert = params.signed_by(&key, &issuer)?;
        Ok(EngineCertificateSet {
            kind,
            ca: ca.pem(),
            cert: cert.pem(),
            key: key.serialize_pem(),
        })
    }

    pub fn certificate_for_key(key_pem: &str) -> Result<String, FixtureError> {
        let key = KeyPair::from_pem(key_pem)?;
        let mut params = CertificateParams::new(Vec::new())?;
        params.distinguished_name = named("jackin test engine rotated client");
        Ok(params.self_signed(&key)?.pem())
    }

    pub fn version_one_certificate_for_key(key_pem: &str) -> Result<String, FixtureError> {
        let key = KeyPair::from_pem(key_pem)?;
        let sha256_with_ecdsa = [0x06, 0x08, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x04, 0x03, 0x02];
        let algorithm = tlv(0x30, &sha256_with_ecdsa);
        let name = tlv(0x30, &[]);
        let validity = tlv(
            0x30,
            &[tlv(0x17, b"260101000000Z"), tlv(0x17, b"360101000000Z")].concat(),
        );
        let tbs = tlv(
            0x30,
            &[
                tlv(0x02, &[0x01]),
                algorithm.clone(),
                name.clone(),
                validity,
                name,
                key.subject_public_key_info(),
            ]
            .concat(),
        );
        let certificate = tlv(0x30, &[tbs, algorithm, tlv(0x03, &[0x00])].concat());
        Ok(pem_block("CERTIFICATE", &certificate))
    }

    pub fn certificate_with_unknown_extensions_for_key(
        key_pem: &str,
    ) -> Result<String, FixtureError> {
        let key = KeyPair::from_pem(key_pem)?;
        let mut params = CertificateParams::new(Vec::new())?;
        params.distinguished_name = named("jackin test engine extended client");
        for (arc, critical) in [(1_u64, false), (2, true)] {
            let mut extension = CustomExtension::from_oid_content(
                &[1, 3, 6, 1, 4, 1, 55_555, arc],
                vec![0x05, 0x00],
            );
            extension.set_criticality(critical);
            params.custom_extensions.push(extension);
        }
        Ok(params.self_signed(&key)?.pem())
    }

    pub fn bare_key(&self) -> Result<String, FixtureError> {
        let pkcs8 = der_of(&self.key)?;
        let private = pkcs8_private_key(&pkcs8).ok_or("the PKCS#8 key has no private key field")?;
        match self.kind {
            EngineKeyKind::Ed25519 => {
                let (_, seed, _) = element(private).ok_or("no Ed25519 seed")?;
                let algorithm = tlv(0x30, &tlv(0x06, &[0x2b, 0x65, 0x70]));
                let version_one = tlv(
                    0x30,
                    &[tlv(0x02, &[0x00]), algorithm, tlv(0x04, &tlv(0x04, seed))].concat(),
                );
                Ok(pem_block("PRIVATE KEY", &version_one))
            }
            EngineKeyKind::EcdsaP256 | EngineKeyKind::EcdsaP384 => {
                let (_, sec1, _) = element(private).ok_or("no SEC1 key")?;
                let (_, _version, rest) = element(sec1).ok_or("no SEC1 version")?;
                let (tag, scalar, _) = element(rest).ok_or("no SEC1 scalar")?;
                if tag != 0x04 {
                    return Err("the SEC1 key has no private scalar".into());
                }
                let bare = tlv(0x30, &[tlv(0x02, &[0x01]), tlv(0x04, scalar)].concat());
                Ok(pem_block("EC PRIVATE KEY", &bare))
            }
            EngineKeyKind::Rsa => Err("an RSA key has no bare form".into()),
        }
    }

    pub fn key_claiming_the_public_key_of(
        &self,
        other: &EngineCertificateSet,
    ) -> Result<String, FixtureError> {
        let own = pkcs8_private_key(&der_of(&self.key)?)
            .and_then(element)
            .map(|(_, sec1, _)| sec1.to_vec())
            .ok_or("no SEC1 key inside the PKCS#8 key")?;
        let other_pkcs8 = der_of(&other.key)?;
        let other_sec1 = pkcs8_private_key(&other_pkcs8)
            .and_then(element)
            .map(|(_, sec1, _)| sec1)
            .ok_or("no SEC1 key inside the other PKCS#8 key")?;
        let (_, _, rest) = element(&own).ok_or("no SEC1 version")?;
        let (_, scalar, _) = element(rest).ok_or("no SEC1 scalar")?;
        let mut rest = element(other_sec1)
            .and_then(|(_, _, rest)| element(rest))
            .map(|(_, _, rest)| rest)
            .filter(|rest| !rest.is_empty());
        let mut public = None;
        while let Some(more) = rest {
            let (tag, content, after) = element(more).ok_or("unreadable SEC1 field")?;
            if tag == 0xa1 {
                public = Some(content.to_vec());
            }
            rest = (!after.is_empty()).then_some(after);
        }
        let public = public.ok_or("the other key carries no public key")?;
        let lying = tlv(
            0x30,
            &[tlv(0x02, &[0x01]), tlv(0x04, scalar), tlv(0xa1, &public)].concat(),
        );
        Ok(pem_block("EC PRIVATE KEY", &lying))
    }

    pub fn unsupported_key() -> String {
        let algorithm = tlv(0x30, &tlv(0x06, &[0x2b, 0x65, 0x6e]));
        let key = tlv(
            0x30,
            &[
                tlv(0x02, &[0x00]),
                algorithm,
                tlv(0x04, &tlv(0x04, &[7_u8; 32])),
            ]
            .concat(),
        );
        pem_block("PRIVATE KEY", &key)
    }

    pub fn pem(&self, cert: EngineCert) -> &str {
        match cert {
            EngineCert::Ca => &self.ca,
            EngineCert::Cert => &self.cert,
            EngineCert::Key => &self.key,
        }
    }

    pub fn traditional_key(&self) -> Result<String, FixtureError> {
        let label = self
            .kind
            .traditional_label()
            .ok_or("this key type has no traditional encoding")?;
        let pkcs8 = der_of(&self.key)?;
        let inner = pkcs8_private_key(&pkcs8).ok_or("the PKCS#8 key has no private key field")?;
        Ok(pem_block(label, inner))
    }
}

pub fn pem_body(pem: &str) -> String {
    pem.lines()
        .filter(|line| !line.starts_with("-----"))
        .collect()
}

fn der_of(pem: &str) -> Result<Vec<u8>, FixtureError> {
    Ok(STANDARD.decode(pem_body(pem))?)
}

fn pem_block(label: &str, der: &[u8]) -> String {
    let encoded = STANDARD.encode(der);
    let mut text = format!("-----BEGIN {label}-----\n");
    for line in encoded.as_bytes().chunks(64) {
        text.push_str(&String::from_utf8_lossy(line));
        text.push('\n');
    }
    text.push_str(&format!("-----END {label}-----\n"));
    text
}

fn tlv(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    match u8::try_from(content.len()) {
        Ok(length) if length < 0x80 => out.push(length),
        _ => {
            let digits: Vec<u8> = content
                .len()
                .to_be_bytes()
                .into_iter()
                .skip_while(|digit| *digit == 0)
                .collect();
            out.push(0x80 | u8::try_from(digits.len()).unwrap_or(0));
            out.extend(digits);
        }
    }
    out.extend(content);
    out
}

fn element(input: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    let (&tag, rest) = input.split_first()?;
    let (&first, rest) = rest.split_first()?;
    let (length, rest) = if first < 0x80 {
        (usize::from(first), rest)
    } else {
        let count = usize::from(first & 0x7f);
        let (digits, rest) = rest.split_at_checked(count)?;
        let length = digits.iter().try_fold(0_usize, |acc, digit| {
            acc.checked_mul(256)?.checked_add(usize::from(*digit))
        })?;
        (length, rest)
    };
    let (content, rest) = rest.split_at_checked(length)?;
    Some((tag, content, rest))
}

fn pkcs8_private_key(der: &[u8]) -> Option<&[u8]> {
    let (_, info, _) = element(der)?;
    let (_, _version, rest) = element(info)?;
    let (_, _algorithm, rest) = element(rest)?;
    let (tag, private_key, _) = element(rest)?;
    (tag == 0x04).then_some(private_key)
}
