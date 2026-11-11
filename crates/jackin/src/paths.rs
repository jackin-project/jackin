// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::{Component, Path, PathBuf};

use jackin_core::HostPath;

use crate::app::App;
use crate::error::CommandError;

pub(crate) fn absolutize(path: &Path, cwd: &HostPath) -> Result<HostPath, CommandError> {
    let mut joined = cwd.as_path().to_path_buf();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => {
                joined = PathBuf::from(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                joined.pop();
            }
            Component::Normal(name) => joined.push(name),
        }
    }
    Ok(HostPath::parse(&joined)?)
}

pub(crate) async fn existing_dir(app: &App, path: &Path) -> Result<HostPath, CommandError> {
    let absolute = absolutize(path, &app.cwd)?;
    Ok(HostPath::canonical_dir(app.deps.host.fs.as_ref(), &absolute).await?)
}
