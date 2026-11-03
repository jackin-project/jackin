// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt;
use std::num::NonZeroU16;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::boundary::FileMode;
use crate::ids::EnvName;
use crate::paths::{HostPath, SandboxPath, sandbox_layout};
use crate::platform::Platform;

pub const ENGINE_HOST: &str = "host.openshell.internal";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineConfig {
    pub port: NonZeroU16,
    pub client_certs: HostPath,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DockerLabel {
    Off,
    Engine(NonZeroU16),
}

impl fmt::Display for DockerLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DockerLabel::Off => f.write_str("off"),
            DockerLabel::Engine(port) => write!(f, "engine port {port}"),
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

    pub fn label(&self) -> DockerLabel {
        match self {
            DockerCapability::Disabled => DockerLabel::Off,
            DockerCapability::Enabled(access) => DockerLabel::Engine(access.engine.port),
        }
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
        version: "29.9.2",
        url_template: "https://download.docker.com/linux/static/stable/{triple}/docker-{version}.tgz",
        targets: &[
            DockerTarget {
                platform: Platform::LinuxArm64,
                triple: "aarch64",
                sha256: "873b3e0207857c3b2f28392fb922f3ea6efe37620c16d4d4aaf2fef409c90b73",
            },
            DockerTarget {
                platform: Platform::LinuxAmd64,
                triple: "x86_64",
                sha256: "9ac225999fd5e863f7694c728ef23d480616b1e5639328fa309befd4243d69eb",
            },
        ],
    },
    cli_members: &["docker/docker"],
    buildx: DockerPart {
        version: "0.38.1",
        url_template: "https://github.com/docker/buildx/releases/download/v{version}/buildx-v{version}.linux-{triple}",
        targets: &[
            DockerTarget {
                platform: Platform::LinuxArm64,
                triple: "arm64",
                sha256: "3b83b3731916ba433feccaaa689fe5bcdc81c641d92c8e6c2ea865dfa141c97a",
            },
            DockerTarget {
                platform: Platform::LinuxAmd64,
                triple: "amd64",
                sha256: "e25d5cd675092bb5d1d03632fdc46cdf3fede7b514dc5f023a105e00d3ce5ad4",
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
        let labels = pem_labels(text)?;
        for label in labels {
            match (self, label) {
                (EngineCert::Ca | EngineCert::Cert, "CERTIFICATE")
                | (EngineCert::Key, "PRIVATE KEY" | "RSA PRIVATE KEY" | "EC PRIVATE KEY") => {}
                (EngineCert::Key, "ENCRYPTED PRIVATE KEY") => {
                    return Err(EngineCertProblem::EncryptedKey);
                }
                _ => return Err(EngineCertProblem::WrongKind),
            }
        }
        Ok(())
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

fn pem_labels(text: &str) -> Result<Vec<&str>, EngineCertProblem> {
    let mut labels = Vec::new();
    let mut open: Option<(&str, usize)> = None;
    for line in text.lines().map(str::trim_end) {
        match open {
            None => open = armor(line, "BEGIN").map(|label| (label, 0)),
            Some((label, body)) => {
                if let Some(found) = armor(line, "END") {
                    if found != label || body == 0 {
                        return Err(EngineCertProblem::NotPem);
                    }
                    labels.push(label);
                    open = None;
                } else if armor(line, "BEGIN").is_some() || !base64_line(line) {
                    return Err(EngineCertProblem::NotPem);
                } else {
                    open = Some((label, body + 1));
                }
            }
        }
    }
    if open.is_some() || labels.is_empty() {
        return Err(EngineCertProblem::NotPem);
    }
    Ok(labels)
}
