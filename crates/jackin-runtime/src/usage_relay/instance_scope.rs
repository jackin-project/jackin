// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Select one immutable launch instance's proof after route authorization.

use std::collections::BTreeMap;

use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCoordinationError, UsageCoordinationErrorKind,
    UsageCredentialScope,
};

pub(super) fn for_instance(
    instance_id: &str,
    capability: &UsageAccountCapability,
    instance_capabilities: &BTreeMap<String, UsageAccountCapability>,
    scope: &UsageCredentialScope,
) -> Result<UsageCredentialScope, UsageCoordinationError> {
    if instance_id.is_empty() || instance_capabilities.get(instance_id) != Some(capability) {
        return Err(UsageCoordinationError {
            kind: UsageCoordinationErrorKind::Unauthorized,
            message: "usage instance route is not authorized".to_owned(),
        });
    }
    Ok(UsageCredentialScope {
        sources: scope
            .sources
            .iter()
            .filter(|proof| {
                proof.instance_id == instance_id && proof.surface_id == capability.surface_id
            })
            .cloned()
            .collect(),
        profiles: scope
            .profiles
            .iter()
            .filter(|proof| {
                proof.instance_id == instance_id && proof.surface_id == capability.surface_id
            })
            .cloned()
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use jackin_protocol::usage_broker::{
        UsageCredentialSourceIdentity, UsageCredentialSourceProof, UsageProfileSourceProof,
    };

    fn route() -> UsageAccountCapability {
        UsageAccountCapability {
            account_id: "same-canonical-route".to_owned(),
            surface_id: "openrouter".to_owned(),
        }
    }

    fn proof(instance: &str, material: &str) -> UsageCredentialSourceProof {
        UsageCredentialSourceProof {
            instance_id: instance.to_owned(),
            account_id: "same-configured-account".to_owned(),
            surface_id: "openrouter".to_owned(),
            key: "OPENROUTER_API_KEY".to_owned(),
            source: UsageCredentialSourceIdentity::HostEnv {
                name: "FIXTURE_SOURCE".to_owned(),
            },
            material_fingerprint: material.to_owned(),
        }
    }

    #[test]
    fn same_account_instances_keep_their_own_captured_environment_material() {
        let capability = route();
        let map = BTreeMap::from([
            ("first".to_owned(), capability.clone()),
            ("second".to_owned(), capability.clone()),
        ]);
        let first = proof("first", "captured-revision-a");
        let second = proof("second", "captured-revision-b");
        let scope = UsageCredentialScope {
            sources: [first.clone(), second.clone()].into_iter().collect(),
            profiles: Default::default(),
        };
        assert_eq!(
            for_instance("first", &capability, &map, &scope)
                .unwrap()
                .sources,
            [first].into_iter().collect()
        );
        assert_eq!(
            for_instance("second", &capability, &map, &scope)
                .unwrap()
                .sources,
            [second].into_iter().collect()
        );
        assert!(for_instance("foreign", &capability, &map, &scope).is_err());
        let mut foreign_route = capability;
        foreign_route.account_id = "foreign-route".to_owned();
        assert!(for_instance("first", &foreign_route, &map, &scope).is_err());
    }

    #[test]
    fn same_account_instances_keep_their_own_captured_profile_revision() {
        let capability = route();
        let map = BTreeMap::from([
            ("first".to_owned(), capability.clone()),
            ("second".to_owned(), capability.clone()),
        ]);
        let first = UsageProfileSourceProof {
            instance_id: "first".to_owned(),
            account_id: "same-configured-account".to_owned(),
            surface_id: "openrouter".to_owned(),
            source: jackin_core::ProfileCredentialSourceIdentity {
                agent: jackin_core::Agent::Codex,
                descriptor_fingerprint: "same-profile-source".to_owned(),
            },
            material_revision: "captured-revision-a".to_owned(),
        };
        let second = UsageProfileSourceProof {
            instance_id: "second".to_owned(),
            material_revision: "captured-revision-b".to_owned(),
            ..first.clone()
        };
        let scope = UsageCredentialScope {
            sources: Default::default(),
            profiles: [first.clone(), second.clone()].into_iter().collect(),
        };
        assert_eq!(
            for_instance("first", &capability, &map, &scope)
                .unwrap()
                .profiles,
            [first].into_iter().collect()
        );
        assert_eq!(
            for_instance("second", &capability, &map, &scope)
                .unwrap()
                .profiles,
            [second].into_iter().collect()
        );
    }
}
