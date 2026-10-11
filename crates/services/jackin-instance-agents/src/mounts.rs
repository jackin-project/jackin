// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Auth mount admission for provisioned credential paths.

use jackin_core::Agent;
use std::path::PathBuf;

use crate::ProvisionedAuth;
use jackin_instance_credentials::{AuthMountLease, auth_directory};

pub fn admit_auth_mounts(
    auth: &ProvisionedAuth,
) -> anyhow::Result<(std::collections::BTreeSet<PathBuf>, Vec<AuthMountLease>)> {
    let mut requests = Vec::<(PathBuf, bool, Agent)>::new();
    for slot in auth.slots.values() {
        if !slot.forward_auth {
            continue;
        }
        let directory = matches!(slot.agent, Agent::Kimi | Agent::Hermes);
        for path in &slot.credential_paths {
            requests.push((path.clone(), directory, slot.agent));
            if !directory && let Some(parent) = path.parent() {
                requests.push((parent.to_path_buf(), true, slot.agent));
            }
        }
    }
    requests
        .sort_by(|left, right| (left.0.as_os_str(), left.1).cmp(&(right.0.as_os_str(), right.1)));
    requests.dedup_by(|left, right| left.0 == right.0 && left.1 == right.1);

    let mut paths = std::collections::BTreeSet::new();
    let mut leases = Vec::new();
    for (path, directory, agent) in requests {
        let lease = if directory {
            auth_directory::lock_mount_directory(&path)?
        } else {
            auth_directory::lock_mount_file(&path)?
        };
        let Some(lease) = lease else {
            if agent != Agent::Claude {
                anyhow::bail!(
                    "{agent} auth mount path disappeared before launch: {}",
                    path.display()
                );
            }
            continue;
        };
        paths.insert(path);
        leases.push(lease);
    }
    Ok((paths, leases))
}
