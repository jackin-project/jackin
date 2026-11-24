// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::{fmt, path::Path, sync::Arc};

use jackin_core::{EnvName, EnvSnapshot, FileSystem, HostPath, RepoPath, SecretBytes};

use super::LoginError;
use crate::gemini_login::DesktopClient;

#[derive(Clone)]
pub struct GoogleRegistrationFile {
    path: Option<HostPath>,
    fs: Arc<dyn FileSystem>,
}

impl fmt::Debug for GoogleRegistrationFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GoogleRegistrationFile")
            .finish_non_exhaustive()
    }
}

impl GoogleRegistrationFile {
    pub fn from_environment(
        env: &EnvSnapshot,
        cwd: &HostPath,
        fs: Arc<dyn FileSystem>,
    ) -> Option<Self> {
        let name = EnvName::parse("JACKIN_GOOGLE_OAUTH_CLIENT_FILE").ok()?;
        let file = env.get(&name)?;
        Some(Self {
            path: HostPath::resolve(Path::new(file), cwd).ok(),
            fs,
        })
    }

    pub(super) async fn client(&self) -> Result<DesktopClient, LoginError> {
        let invalid = || LoginError::GoogleRegistration;
        let file = self
            .fs
            .canonicalize(self.path.as_ref().ok_or_else(invalid)?)
            .await
            .map_err(|_| invalid())?
            .ok_or_else(invalid)?;
        let path = file.as_path();
        let parent = HostPath::parse(path.parent().ok_or_else(invalid)?).map_err(|_| invalid())?;
        let name = RepoPath::parse(
            path.file_name()
                .and_then(|s| s.to_str())
                .ok_or_else(invalid)?,
        )
        .map_err(|_| invalid())?;
        let regular = self
            .fs
            .read_regular(&parent, &name, 64 * 1024)
            .await
            .map_err(|_| invalid())?
            .map_err(|_| invalid())?;
        DesktopClient::from_installed_json(&SecretBytes::new(regular.bytes)).ok_or_else(invalid)
    }
}
