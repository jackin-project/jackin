// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Runtime projection methods.

use jackin_core::account_key_hash;

use jackin_protocol::usage_broker::UsageProjectionV1;

use super::super::HostUsageRuntime;
use jackin_usage_destination::ProjectionMetadata;
use jackin_usage_projection::build_canonical_projection;

impl HostUsageRuntime {
    /// Build the immutable surface-neutral V1 publication from current discovery.
    pub fn canonical_projection(&mut self, locale: &str) -> Result<UsageProjectionV1, String> {
        self.require_open()?;
        let aliases = self
            .discovery
            .as_ref()
            .ok_or_else(|| "usage discovery has not completed".to_owned())?
            .canonical_aliases()
            .map(|(capability_id, identity)| (capability_id.to_owned(), identity.clone()))
            .collect::<Vec<_>>();
        for (capability_id, identity) in aliases {
            let _canonical_id = self
                .canonical_identity_graph
                .resolve_alias(&capability_id, &identity)?;
        }
        let catalog = self.materialize_account_catalog()?;
        let discovery = self
            .discovery
            .as_ref()
            .ok_or_else(|| "usage discovery has not completed".to_owned())?;
        let draft = build_canonical_projection(
            &catalog,
            discovery,
            &self.broker_generations,
            ProjectionMetadata {
                projection_id: "draft",
                generated_at_epoch: 0,
                broker_instance_id: &self.canonical_instance_id,
                broker_generation: 0,
                refreshing: self.broker_refresh_in_progress(),
                locale,
            },
        )?;
        let content = serde_json::to_string(&draft)
            .map_err(|error| format!("canonical usage projection failed: {error}"))?;
        let content_id = account_key_hash("usage-projection-content-v1", &content);
        if self.canonical_content_id.as_deref() == Some(content_id.as_str()) {
            return self
                .canonical_projection_cache
                .clone()
                .ok_or_else(|| "canonical usage projection cache missing".to_owned());
        }
        let generation = self
            .canonical_projection_cache
            .as_ref()
            .map_or(1, |projection| {
                projection.broker_generation.saturating_add(1)
            });
        let projection_id = format!("{}:{generation:020}", self.canonical_instance_id);
        let projection = build_canonical_projection(
            &catalog,
            discovery,
            &self.broker_generations,
            ProjectionMetadata {
                projection_id: &projection_id,
                generated_at_epoch: chrono::Utc::now().timestamp(),
                broker_instance_id: &self.canonical_instance_id,
                broker_generation: generation,
                refreshing: self.broker_refresh_in_progress(),
                locale,
            },
        )?;
        self.canonical_content_id = Some(content_id);
        self.canonical_projection_cache = Some(projection.clone());
        Ok(projection)
    }
}
