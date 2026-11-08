// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt;
use std::num::NonZeroU16;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::boundary::FileMode;
use crate::ids::EnvName;
use crate::paths::{HostPath, SandboxPath, sandbox_layout};
use crate::platform::Platform;

pub const ENGINE_HOST: &str = "host.openshell.internal";
pub const HOST_GATEWAY_ALIASES: [&str; 3] = [
    ENGINE_HOST,
    "host.containers.internal",
    "host.docker.internal",
];
pub const PLAINTEXT_DOCKER_PORT: u16 = 2375;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineConfig {
    pub port: NonZeroU16,
    pub client_certs: HostPath,
}

impl EngineConfig {
    pub fn port_refusal(port: NonZeroU16) -> Option<String> {
        (port.get() == PLAINTEXT_DOCKER_PORT).then(|| {
            format!("port {PLAINTEXT_DOCKER_PORT} is the plaintext Docker port; use the TLS port")
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DockerSource {
    Role,
    Workspace,
    Both,
}

impl DockerSource {
    pub fn from_flags(role_enabled: bool, workspace_enabled: bool) -> Option<DockerSource> {
        match (role_enabled, workspace_enabled) {
            (false, false) => None,
            (true, false) => Some(DockerSource::Role),
            (false, true) => Some(DockerSource::Workspace),
            (true, true) => Some(DockerSource::Both),
        }
    }
}

impl fmt::Display for DockerSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            DockerSource::Role => "the role",
            DockerSource::Workspace => "the workspace",
            DockerSource::Both => "both the role and the workspace",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineAccess {
    pub engine: EngineConfig,
    pub enabled_by: DockerSource,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DockerCapability {
    Disabled,
    Enabled(EngineAccess),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DockerLayer {
    Absent,
    Client,
}

impl DockerLayer {
    pub fn from_flags(role_enabled: bool, workspace_enabled: bool) -> DockerLayer {
        match DockerSource::from_flags(role_enabled, workspace_enabled) {
            Some(_) => DockerLayer::Client,
            None => DockerLayer::Absent,
        }
    }
}

const CA_FINGERPRINT_BYTES: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CaFingerprint([u8; CA_FINGERPRINT_BYTES]);

impl CaFingerprint {
    pub fn of_ca(pem: &[u8]) -> Result<CaFingerprint, EngineCertProblem> {
        EngineCert::Ca.check(pem)?;
        let text = std::str::from_utf8(pem).map_err(|_| EngineCertProblem::NotPem)?;
        let mut certificates = Vec::new();
        for block in pem_blocks(text)? {
            if block.label == "CERTIFICATE" {
                let body = block.body.concat();
                certificates.push(
                    STANDARD
                        .decode(body)
                        .map_err(|_| EngineCertProblem::NotPem)?,
                );
            }
        }
        certificates.sort();
        certificates.dedup();
        let mut hasher = Sha256::new();
        for certificate in &certificates {
            hasher.update(certificate);
        }
        let digest = hasher.finalize();
        let mut bytes = [0_u8; CA_FINGERPRINT_BYTES];
        bytes.copy_from_slice(&digest[..CA_FINGERPRINT_BYTES]);
        Ok(CaFingerprint(bytes))
    }

    pub fn parse(text: &str) -> Option<CaFingerprint> {
        let lower_hex = text
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'));
        if text.len() != CA_FINGERPRINT_BYTES * 2 || !lower_hex {
            return None;
        }
        let mut bytes = [0_u8; CA_FINGERPRINT_BYTES];
        for (index, slot) in bytes.iter_mut().enumerate() {
            *slot = u8::from_str_radix(text.get(index * 2..index * 2 + 2)?, 16).ok()?;
        }
        Some(CaFingerprint(bytes))
    }
}

impl fmt::Display for CaFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|byte| write!(f, "{byte:02x}"))
    }
}

impl Serialize for CaFingerprint {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for CaFingerprint {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<CaFingerprint, D::Error> {
        let text = String::deserialize(deserializer)?;
        CaFingerprint::parse(&text)
            .ok_or_else(|| D::Error::custom("a CA fingerprint is 16 lowercase hex digits"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DockerLabel {
    Off,
    Engine { port: NonZeroU16, ca: CaFingerprint },
}

impl fmt::Display for DockerLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DockerLabel::Off => f.write_str("off"),
            DockerLabel::Engine { port, ca } => write!(f, "engine port {port} with CA {ca}"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DockerRisk {
    ClientKeyReadable,
    ContainersBypassPolicy,
    BindMountsNameEngineHost,
    EngineOutlivesSandbox,
}

impl DockerRisk {
    pub const ALL: [DockerRisk; 4] = [
        DockerRisk::ClientKeyReadable,
        DockerRisk::ContainersBypassPolicy,
        DockerRisk::BindMountsNameEngineHost,
        DockerRisk::EngineOutlivesSandbox,
    ];

    pub fn label(self) -> &'static str {
        match self {
            DockerRisk::ClientKeyReadable => "Client key",
            DockerRisk::ContainersBypassPolicy => "Network",
            DockerRisk::BindMountsNameEngineHost => "Bind mounts",
            DockerRisk::EngineOutlivesSandbox => "Lifetime",
        }
    }

    pub fn text(self) -> &'static str {
        match self {
            DockerRisk::ClientKeyReadable => "The agent can read the engine client key.",
            DockerRisk::ContainersBypassPolicy => "Containers bypass the sandbox network policy.",
            DockerRisk::BindMountsNameEngineHost => {
                "A bind mount names a path on the engine host, not in the sandbox."
            }
            DockerRisk::EngineOutlivesSandbox => {
                "The engine outlives the sandbox. Sandboxes that share one engine see each other's containers, and you remove them."
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum DockerError {
    #[error(
        "the Docker capability is enabled by {enabled_by} but the gateway has no Docker engine"
    )]
    EngineMissing { enabled_by: DockerSource },
}

impl DockerCapability {
    pub fn resolve(
        role_enabled: bool,
        workspace_enabled: bool,
        engine: Option<&EngineConfig>,
    ) -> Result<DockerCapability, DockerError> {
        let Some(enabled_by) = DockerSource::from_flags(role_enabled, workspace_enabled) else {
            return Ok(DockerCapability::Disabled);
        };
        engine
            .map(|engine| {
                DockerCapability::Enabled(EngineAccess {
                    engine: engine.clone(),
                    enabled_by,
                })
            })
            .ok_or(DockerError::EngineMissing { enabled_by })
    }

    pub fn layer(&self) -> DockerLayer {
        match self {
            DockerCapability::Disabled => DockerLayer::Absent,
            DockerCapability::Enabled(_) => DockerLayer::Client,
        }
    }

    pub fn environment(&self) -> BTreeMap<EnvName, String> {
        let DockerCapability::Enabled(access) = self else {
            return BTreeMap::new();
        };
        [
            (
                "DOCKER_HOST",
                format!("tcp://{ENGINE_HOST}:{}", access.engine.port),
            ),
            ("DOCKER_TLS_VERIFY", "1".to_owned()),
            (
                "DOCKER_CERT_PATH",
                sandbox_layout::DOCKER_DIR.as_str().to_owned(),
            ),
        ]
        .into_iter()
        .map(|(name, value)| (EnvName::from_static(name), value))
        .collect()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DockerTarget {
    pub platform: Platform,
    pub triple: &'static str,
    pub sha256: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct DockerPart {
    pub version: &'static str,
    pub url_template: &'static str,
    pub targets: &'static [DockerTarget],
}

#[derive(Clone, Copy, Debug)]
pub struct DockerClientSpec {
    pub cli: DockerPart,
    pub cli_members: &'static [&'static str],
    pub buildx: DockerPart,
    pub compose: DockerPart,
}

impl DockerPart {
    pub fn target(&self, platform: Platform) -> Option<&'static DockerTarget> {
        self.targets
            .iter()
            .find(|target| target.platform == platform)
    }
}

pub static DOCKER_CLIENT: DockerClientSpec = DockerClientSpec {
    cli: DockerPart {
        version: "29.9.3",
        url_template: "https://download.docker.com/linux/static/stable/{triple}/docker-{version}.tgz",
        targets: &[
            DockerTarget {
                platform: Platform::LinuxArm64,
                triple: "aarch64",
                sha256: "1b27fd739378ef938401e81ce5afb73b7ad7432d4f598814a9ff5f826bb464d2",
            },
            DockerTarget {
                platform: Platform::LinuxAmd64,
                triple: "x86_64",
                sha256: "3fb5d1f13014c18cc54945bc177c58a9056a482dada031e17d12cf2e6922c973",
            },
        ],
    },
    cli_members: &["docker/docker"],
    buildx: DockerPart {
        version: "0.38.2",
        url_template: "https://github.com/docker/buildx/releases/download/v{version}/buildx-v{version}.linux-{triple}",
        targets: &[
            DockerTarget {
                platform: Platform::LinuxArm64,
                triple: "arm64",
                sha256: "46c0f334772de3cb0b341cc415caf7339fb5cfc8b8438ebcb5a10e7f23800d36",
            },
            DockerTarget {
                platform: Platform::LinuxAmd64,
                triple: "amd64",
                sha256: "7992ec09413b80ff57ba4e081f56fc833d7bac1d5e71143c6c809e7e6065f6d6",
            },
        ],
    },
    compose: DockerPart {
        version: "5.7.0",
        url_template: "https://github.com/docker/compose/releases/download/v{version}/docker-compose-linux-{triple}",
        targets: &[
            DockerTarget {
                platform: Platform::LinuxArm64,
                triple: "aarch64",
                sha256: "15861535d14e26555736b49376c95e14bfd94285366c8446bfac7156a972e76f",
            },
            DockerTarget {
                platform: Platform::LinuxAmd64,
                triple: "x86_64",
                sha256: "7758048ad38e580f22559603312874fec8bb638d021a9e64182385fc17cc12a2",
            },
        ],
    },
};

pub const ENGINE_CERT_MAX_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EngineCert {
    Ca,
    Cert,
    Key,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Error)]
pub enum EngineCertProblem {
    #[error("it does not exist")]
    Missing,
    #[error("it is a symlink")]
    Symlink,
    #[error("it is not a regular file")]
    NotARegularFile,
    #[error("it is empty")]
    Empty,
    #[error("it is larger than 64 KiB")]
    TooLarge,
    #[error("it is not PEM text")]
    NotPem,
    #[error("it holds a PEM block of the wrong kind")]
    WrongKind,
    #[error("the private key is encrypted")]
    EncryptedKey,
    #[error("the client certificate and private key do not form a matching pair")]
    PairMismatch,
    #[error("it changed after the launch bound its fingerprint")]
    Changed,
}

impl EngineCert {
    pub const ALL: [EngineCert; 3] = [EngineCert::Ca, EngineCert::Cert, EngineCert::Key];

    pub fn file_name(self) -> &'static str {
        match self {
            EngineCert::Ca => "ca.pem",
            EngineCert::Cert => "cert.pem",
            EngineCert::Key => "key.pem",
        }
    }

    pub fn mode(self) -> FileMode {
        match self {
            EngineCert::Ca | EngineCert::Cert => FileMode(0o644),
            EngineCert::Key => FileMode(0o600),
        }
    }

    pub fn sandbox_path(self) -> SandboxPath {
        sandbox_layout::docker_file(self.file_name())
    }

    pub fn check(self, bytes: &[u8]) -> Result<(), EngineCertProblem> {
        if bytes.is_empty() {
            return Err(EngineCertProblem::Empty);
        }
        if bytes.len() > ENGINE_CERT_MAX_BYTES {
            return Err(EngineCertProblem::TooLarge);
        }
        let text = std::str::from_utf8(bytes).map_err(|_| EngineCertProblem::NotPem)?;
        let mut keys = 0_usize;
        for block in pem_blocks(text)? {
            match (self, block.label) {
                (EngineCert::Ca | EngineCert::Cert, "CERTIFICATE") => {}
                (EngineCert::Key, "PRIVATE KEY" | "RSA PRIVATE KEY" | "EC PRIVATE KEY") => {
                    keys += 1;
                }
                (EngineCert::Key, "EC PARAMETERS" | "DH PARAMETERS") => {}
                (EngineCert::Key, "ENCRYPTED PRIVATE KEY") => {
                    return Err(EngineCertProblem::EncryptedKey);
                }
                _ => return Err(EngineCertProblem::WrongKind),
            }
        }
        if matches!(self, EngineCert::Key) && keys == 0 {
            return Err(EngineCertProblem::WrongKind);
        }
        Ok(())
    }

    pub fn check_pair(cert: &[u8], key: &[u8]) -> Result<(), EngineCertProblem> {
        EngineCert::Cert.check(cert)?;
        EngineCert::Key.check(key)?;
        let cert_text = std::str::from_utf8(cert).map_err(|_| EngineCertProblem::NotPem)?;
        let cert_chain = pem_blocks(cert_text)?
            .into_iter()
            .filter(|block| block.label == "CERTIFICATE")
            .map(|block| {
                STANDARD
                    .decode(block.body.join(""))
                    .map(rustls::pki_types::CertificateDer::from)
                    .map_err(|_| EngineCertProblem::NotPem)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let key_text = std::str::from_utf8(key).map_err(|_| EngineCertProblem::NotPem)?;
        let mut private_keys = pem_blocks(key_text)?.into_iter().filter(|block| {
            matches!(
                block.label,
                "PRIVATE KEY" | "RSA PRIVATE KEY" | "EC PRIVATE KEY"
            )
        });
        let private_key = private_keys.next().ok_or(EngineCertProblem::PairMismatch)?;
        if private_keys.next().is_some() {
            return Err(EngineCertProblem::PairMismatch);
        }
        let key_der = STANDARD
            .decode(private_key.body.join(""))
            .map_err(|_| EngineCertProblem::PairMismatch)?;
        let key_der = rustls::pki_types::PrivateKeyDer::try_from(key_der)
            .map_err(|_| EngineCertProblem::PairMismatch)?;
        let signing_key = rustls::crypto::aws_lc_rs::default_provider()
            .key_provider
            .load_private_key(key_der)
            .map_err(|_| EngineCertProblem::PairMismatch)?;
        rustls::sign::CertifiedKey::new(cert_chain, signing_key)
            .keys_match()
            .map_err(|_| EngineCertProblem::PairMismatch)
    }
}

fn armor<'a>(line: &'a str, word: &str) -> Option<&'a str> {
    line.strip_prefix("-----")?
        .strip_prefix(word)?
        .strip_prefix(' ')?
        .strip_suffix("-----")
}

fn base64_line(line: &str) -> bool {
    !line.is_empty()
        && line
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'='))
}

struct PemBlock<'a> {
    label: &'a str,
    body: Vec<&'a str>,
}

fn pem_blocks(text: &str) -> Result<Vec<PemBlock<'_>>, EngineCertProblem> {
    let mut blocks = Vec::new();
    let mut open: Option<PemBlock<'_>> = None;
    for line in text.lines().map(str::trim_end) {
        match open.take() {
            None => {
                open = armor(line, "BEGIN").map(|label| PemBlock {
                    label,
                    body: Vec::new(),
                });
            }
            Some(mut block) => {
                if let Some(found) = armor(line, "END") {
                    if found != block.label || block.body.is_empty() {
                        return Err(EngineCertProblem::NotPem);
                    }
                    blocks.push(block);
                } else if armor(line, "BEGIN").is_some() || !base64_line(line) {
                    return Err(EngineCertProblem::NotPem);
                } else {
                    block.body.push(line);
                    open = Some(block);
                }
            }
        }
    }
    if open.is_some() || blocks.is_empty() {
        return Err(EngineCertProblem::NotPem);
    }
    Ok(blocks)
}
