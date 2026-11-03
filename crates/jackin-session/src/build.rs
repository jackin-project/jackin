// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Config, ConfigError, GatewayName, HostPath, RoleName};
use jackin_image::{BuiltImage, ImageEngine, ImageError, ProgressSink, Rebuild, ensure_image};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::plan::{PlanError, plan_image};
use crate::roles::{RoleCatalog, RoleError};
use crate::workspace::lookup_workspace;

#[derive(Clone, Copy, Debug)]
pub struct BuildRequest<'a> {
    pub gateway: Option<&'a GatewayName>,
    pub role: Option<&'a RoleName>,
    pub path: &'a HostPath,
    pub rebuild: Rebuild,
}

#[derive(Debug, Error)]
pub enum BuildError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Role(#[from] RoleError),
    #[error(transparent)]
    Plan(#[from] PlanError),
    #[error(transparent)]
    Image(#[from] ImageError),
}

pub async fn build_for_workspace(
    engine: &dyn ImageEngine,
    catalog: &RoleCatalog,
    config: &Config,
    request: &BuildRequest<'_>,
    progress: &ProgressSink,
    cancel: &CancellationToken,
) -> Result<BuiltImage, BuildError> {
    let (gateway, _) = config.resolve_gateway(request.gateway)?;
    let workspace = lookup_workspace(config, request.path).workspace();
    let role = catalog.load(config, request.role, &workspace).await?;
    let image = plan_image(config, &role, gateway, None, request.rebuild)?;
    Ok(ensure_image(engine, &image, progress, cancel).await?)
}
