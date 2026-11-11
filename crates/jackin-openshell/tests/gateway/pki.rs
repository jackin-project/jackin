// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::net::{IpAddr, Ipv4Addr};

use rcgen::{
    BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose, SanType,
};

pub(crate) const KEY_SENTINEL: &str = "PRIVATE-SENTINEL-Zq7#Xv9!Lm2@Hk4w";

#[derive(Clone, Debug)]
pub(crate) struct Pki {
    pub(crate) ca_pem: String,
    pub(crate) server_cert_pem: String,
    pub(crate) server_key_pem: String,
    pub(crate) client_cert_pem: String,
    pub(crate) client_key_pem: String,
}

fn signed(params: CertificateParams, issuer: &Issuer<'_, &KeyPair>) -> (String, String) {
    let key = KeyPair::generate().expect("key pair");
    let cert = params.signed_by(&key, issuer).expect("certificate");
    (cert.pem(), key.serialize_pem())
}

impl Pki {
    pub(crate) fn new() -> Pki {
        let ca_key = KeyPair::generate().expect("ca key");
        let mut ca_params = CertificateParams::new(Vec::<String>::new()).expect("ca params");
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        let ca_cert = ca_params.self_signed(&ca_key).expect("ca cert");
        let issuer = Issuer::from_params(&ca_params, &ca_key);

        let mut server_params =
            CertificateParams::new(vec!["localhost".to_owned()]).expect("server params");
        server_params
            .subject_alt_names
            .push(SanType::IpAddress(IpAddr::V4(Ipv4Addr::LOCALHOST)));
        server_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        let (server_cert_pem, server_key_pem) = signed(server_params, &issuer);

        let mut client_params =
            CertificateParams::new(vec!["jackin-test-client".to_owned()]).expect("client params");
        client_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
        let (client_cert_pem, client_key_pem) = signed(client_params, &issuer);

        Pki {
            ca_pem: ca_cert.pem(),
            server_cert_pem,
            server_key_pem,
            client_cert_pem,
            client_key_pem,
        }
    }

    pub(crate) fn key_file(&self) -> String {
        format!("{}\n{KEY_SENTINEL}\n", self.client_key_pem)
    }
}
