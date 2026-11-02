// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{GatewayName, OpenShellWorkspace, SandboxName, SandboxPath};

pub(crate) struct Scope<'a> {
    pub(crate) gateway: &'a GatewayName,
    pub(crate) workspace: &'a OpenShellWorkspace,
}

impl Scope<'_> {
    fn base(&self, verb: &str) -> Vec<String> {
        vec![
            "-g".to_owned(),
            self.gateway.to_string(),
            "--workspace".to_owned(),
            self.workspace.to_string(),
            "sandbox".to_owned(),
            verb.to_owned(),
        ]
    }

    pub(crate) fn upload(
        &self,
        sandbox: &SandboxName,
        source: &str,
        dest: &SandboxPath,
        no_git_ignore: bool,
    ) -> Vec<String> {
        let mut args = self.base("upload");
        args.extend([
            sandbox.to_string(),
            source.to_owned(),
            dest.as_str().to_owned(),
        ]);
        if no_git_ignore {
            args.push("--no-git-ignore".to_owned());
        }
        args
    }

    pub(crate) fn download(
        &self,
        sandbox: &SandboxName,
        remote: &SandboxPath,
        local: &str,
    ) -> Vec<String> {
        let mut args = self.base("download");
        args.extend([
            sandbox.to_string(),
            remote.as_str().to_owned(),
            local.to_owned(),
        ]);
        args
    }
}
