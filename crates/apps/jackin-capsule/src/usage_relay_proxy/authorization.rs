// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Capability authorization for the usage relay proxy.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use jackin_protocol::CapsuleConfig;
use jackin_protocol::usage_broker::{UsageAccountCapability, UsageBrokerOperation};

use super::identity::{PeerIdentity, SupervisorIdentity};

/// Immutable capability binding loaded from the host-validated Capsule config.
/// Session peers get exactly one capability through their kernel UID/GID; the
/// root Capsule supervisor may use the launch-wide set for daemon refreshes.
#[derive(Debug, Clone, Default)]
pub(crate) struct UsageRelayAuthorization {
    by_peer: BTreeMap<(u32, u32), UsageAccountCapability>,
    launch_capabilities: BTreeSet<UsageAccountCapability>,
}

impl UsageRelayAuthorization {
    pub(crate) fn from_config(config: &CapsuleConfig) -> Result<Self> {
        let mut authorization = Self::default();
        for (instance, capability) in &config.usage_capabilities {
            anyhow::ensure!(
                config
                    .instances
                    .iter()
                    .any(|candidate| candidate == instance),
                "usage capability names an instance outside the configured allowlist"
            );
            anyhow::ensure!(
                !capability.account_id.is_empty() && !capability.surface_id.is_empty(),
                "usage capability for instance {instance:?} is empty"
            );
            let identity = config.identity_for_instance(instance).ok_or_else(|| {
                anyhow::anyhow!("usage instance {instance:?} has no Unix identity")
            })?;
            let peer = PeerIdentity::from(identity);
            anyhow::ensure!(
                authorization
                    .by_peer
                    .insert((peer.uid, peer.gid), capability.clone())
                    .is_none(),
                "multiple usage instances share Unix identity {peer:?}"
            );
            authorization.launch_capabilities.insert(capability.clone());
        }
        Ok(authorization)
    }

    pub(crate) fn authorizes(
        &self,
        supervisor: SupervisorIdentity,
        peer: Option<PeerIdentity>,
        operation: &UsageBrokerOperation,
    ) -> bool {
        let Some(capability) = operation_capability(operation) else {
            return false;
        };
        let Some(peer) = peer else {
            return false;
        };
        if supervisor.matches(peer) {
            return self.launch_capabilities.contains(capability);
        }
        self.by_peer.get(&(peer.uid, peer.gid)) == Some(capability)
    }

    #[cfg(test)]
    pub(crate) fn for_peer(peer: PeerIdentity, capability: UsageAccountCapability) -> Self {
        Self {
            by_peer: BTreeMap::from([((peer.uid, peer.gid), capability.clone())]),
            launch_capabilities: BTreeSet::from([capability]),
        }
    }
}

fn operation_capability(operation: &UsageBrokerOperation) -> Option<&UsageAccountCapability> {
    match operation {
        UsageBrokerOperation::CurrentForCapability { capability }
        | UsageBrokerOperation::RefreshForCapability { capability, .. }
        | UsageBrokerOperation::JoinForCapability { capability, .. }
        | UsageBrokerOperation::Current { capability }
        | UsageBrokerOperation::Refresh { capability, .. }
        | UsageBrokerOperation::Join { capability, .. } => Some(capability),
        UsageBrokerOperation::CurrentProjection
        | UsageBrokerOperation::RequestRefresh { .. }
        | UsageBrokerOperation::JoinPublication { .. }
        | UsageBrokerOperation::ReconcileCatalog { .. }
        | UsageBrokerOperation::CurrentProjectionForSurface
        | UsageBrokerOperation::RequestRefreshForSurface { .. }
        | UsageBrokerOperation::JoinPublicationForSurface { .. } => None,
    }
}
