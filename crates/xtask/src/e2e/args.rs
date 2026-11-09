// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::num::NonZeroU16;
use std::path::PathBuf;

use clap::Args;

#[derive(Args, Clone, Debug, Default)]
pub struct E2eArgs {
    #[arg(
        long,
        value_name = "NAME",
        help = "Registered OpenShell gateway; passed to every jackin call"
    )]
    pub gateway: String,
    #[arg(
        long,
        value_name = "DIR",
        required_unless_present = "sweep",
        help = "The sentinel role repository"
    )]
    pub role: Option<PathBuf>,
    #[arg(
        long,
        value_name = "PATH",
        help = "A built jackin binary; default: build the workspace binary"
    )]
    pub jackin: Option<PathBuf>,
    #[arg(
        long,
        value_name = "DIR",
        help = "Codex home with the A1 ChatGPT login; default: $HOME/.codex"
    )]
    pub codex_home: Option<PathBuf>,
    #[arg(
        long,
        value_name = "AGENT",
        help = "Agent tuple to prove; default: codex"
    )]
    pub agent: Option<String>,
    #[arg(
        long,
        value_name = "PROVIDER",
        help = "Provider tuple to prove; default: the agent's native provider"
    )]
    pub provider: Option<String>,
    #[arg(
        long,
        value_name = "MODEL",
        help = "Model to prove; default: the tuple default"
    )]
    pub model: Option<String>,
    #[arg(
        long,
        value_name = "SOURCE",
        help = "Reference-only credential source for a non-T01 tuple"
    )]
    pub credential: Option<String>,
    #[arg(
        long,
        value_name = "NAME",
        help = "Docker buildx builder, passed to jackin gateway add"
    )]
    pub builder: Option<String>,
    #[arg(
        long,
        value_name = "PLATFORM",
        value_parser = ["linux/arm64", "linux/amd64"],
        help = "Image platform, passed to jackin gateway add"
    )]
    pub platform: Option<String>,
    #[arg(
        long,
        value_name = "PORT",
        requires = "engine_certs",
        help = "Port of the operator's D24 test engine on host.openshell.internal (J15, J16)"
    )]
    pub engine_port: Option<NonZeroU16>,
    #[arg(
        long,
        value_name = "DIR",
        requires = "engine_port",
        help = "Directory with the D24 test engine client ca.pem, cert.pem, and key.pem"
    )]
    pub engine_certs: Option<PathBuf>,
    #[arg(
        long,
        value_name = "DIR",
        help = "Directory for the redacted per-step logs; default: target/xtask-e2e/<run>"
    )]
    pub log_dir: Option<PathBuf>,
    #[arg(
        long,
        help = "Delete the sandboxes and providers that crashed e2e runs left behind, then exit"
    )]
    pub sweep: bool,
}

impl E2eArgs {
    #[must_use]
    pub fn engine(&self) -> Option<(NonZeroU16, &PathBuf)> {
        self.engine_port.zip(self.engine_certs.as_ref())
    }
}
