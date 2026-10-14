// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::auth::AuthForwardMode;

macro_rules! bounded_fallback_curl {
    ($url:literal, $pipe:literal) => {
        concat!(
            "curl -fsSL --connect-timeout 15 --max-time 120 --retry 2 --retry-delay 2 --retry-connrefused ",
            $url,
            $pipe
        )
    };
}

pub(crate) use bounded_fallback_curl;

pub(crate) fn looks_like_version(token: &str) -> bool {
    token.split('.').count() >= 2 && token.starts_with(|c: char| c.is_ascii_digit())
}

pub(crate) fn render_fallback_install_block(
    path_prefix: &str,
    install_command: &str,
    version_check_bin: &str,
) -> String {
    format!(
        "\
USER agent
ARG JACKIN_CACHE_BUST=0
ENV PATH=\"{path_prefix}:${{PATH}}\"
ENV XDG_CACHE_HOME=\"/home/agent/.cache\"
RUN --mount=type=cache,id=jackin-agent-fallback-{version_check_bin},target=/home/agent/.cache,uid=1000,gid=1000,sharing=locked \\
    set -euxo pipefail && \\
    : \"${{JACKIN_CACHE_BUST}}\" && \\
    {{ {install_command}; }} || \\
    {{ sleep 5; {install_command}; }} || \\
    {{ sleep 10; {install_command}; }} && \\
    {version_check_bin} --version
"
    )
}

pub(crate) mod private {
    pub(crate) trait Sealed {}
}

#[expect(
    private_bounds,
    reason = "sealed trait uses a private supertrait to block external implementations"
)]
pub trait AgentRuntime: Send + Sync + 'static + private::Sealed {
    fn slug(&self) -> &'static str;

    fn label(&self) -> &'static str;

    fn install_block(&self, source: &str) -> String;

    fn container_binary_paths(&self) -> &'static [&'static str];

    fn fallback_install_block(&self) -> String;

    fn fallback_install_command(&self) -> &'static str;

    fn required_env_var(&self, mode: AuthForwardMode) -> Option<&'static str>;

    fn supported_modes(&self) -> &'static [AuthForwardMode];

    fn state_paths(&self) -> AgentStatePaths;

    fn default_home_exclude_paths(&self) -> &'static [&'static str] {
        &[]
    }

    fn parse_version<'a>(&self, raw: &'a str) -> Option<&'a str>;
}

#[derive(Debug, Clone)]
pub struct AgentStatePaths {
    pub credential_dir: &'static str,

    pub config_dir: Option<&'static str>,

    pub credential_file: Option<&'static str>,

    pub folder_env_var: Option<FolderVar>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FolderVar {
    pub name: &'static str,

    pub kind: FolderVarKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderVarKind {
    Dir,

    Parent,

    XdgRoot,
}

impl AgentStatePaths {
    pub fn home_dirs(&self) -> impl Iterator<Item = &'static str> {
        std::iter::once(self.credential_dir).chain(self.config_dir)
    }
}

pub(crate) use super::adapters;
