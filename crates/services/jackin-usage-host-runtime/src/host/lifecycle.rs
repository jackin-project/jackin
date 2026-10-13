// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `HostUsageRuntime` open and shutdown lifecycle.

use super::{
    HOST_USAGE_STATE_REL, HostProbePolicy, HostRuntimeConfig, HostSurfaceId, HostUsageEvent,
    HostUsageRuntime, MAX_EVENT_LOG, ProviderCredentialEnvResolver, ValidatedUsageDiscovery,
    accounts, canonical_instance_id, discover_usage_sources, enabled_surface_ids,
    host_accounts_path, validate_usage_sources,
};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use jackin_core::Agent;

use jackin_usage_provider_core::{UsageCache, UsageFormatPrefs};

impl HostUsageRuntime {
    /// Construct a closed runtime (call [`Self::open`] before use).
    #[must_use]
    pub fn new() -> Self {
        Self {
            cache: UsageCache::default(),
            enabled: HashSet::new(),
            events: VecDeque::new(),
            next_seq: 0,
            refresh_floor_secs: 300,
            last_refresh: None,
            format_prefs: UsageFormatPrefs::default(),
            open: false,
            data_dir: None,
            selected_accounts: HashMap::new(),
            probe_policy: HostProbePolicy::Live,
            desktop_detected_surfaces: HashSet::new(),
            discovery: None,
            discovery_generation: 0,
            discovered_views: BTreeMap::new(),
            discovered_provider_views: BTreeMap::new(),
            discovery_scope: None,
            broker_phases: BTreeMap::new(),
            broker_generations: BTreeMap::new(),
            canonical_instance_id: canonical_instance_id(),
            canonical_content_id: None,
            canonical_projection_cache: None,
            canonical_identity_graph: accounts::CanonicalIdentityGraph::default(),
        }
    }

    /// Open with host paths; enables all surfaces when config list empty.
    pub fn open(&mut self, config: HostRuntimeConfig) -> Result<(), String> {
        self.open_prepared(config, None)
    }

    /// Open after Rust-owned config/env discovery completes.
    pub fn open_with_discovery(
        &mut self,
        config: HostRuntimeConfig,
        resolver: &dyn ProviderCredentialEnvResolver,
    ) -> Result<(), String> {
        let discovered = validate_usage_sources(
            discover_usage_sources(&config.discovery_scope, resolver)?,
            resolver,
        );
        self.open_with_validated_discovery(config, discovered)
    }

    /// Open after a caller has completed a fresh, validated discovery scan.
    /// The typed discovery result is committed only after all config checks
    /// pass, so broker activation can publish against the same generation.
    pub fn open_with_validated_discovery(
        &mut self,
        config: HostRuntimeConfig,
        discovery: ValidatedUsageDiscovery,
    ) -> Result<(), String> {
        self.open_prepared(config, Some(discovery))
    }

    pub(crate) fn open_prepared(
        &mut self,
        config: HostRuntimeConfig,
        discovery: Option<ValidatedUsageDiscovery>,
    ) -> Result<(), String> {
        let enabled = enabled_surface_ids(&config)?;
        let data_dir_changed = self
            .data_dir
            .as_ref()
            .is_some_and(|current| current != &config.data_dir);
        if data_dir_changed {
            self.cache = UsageCache::default();
            self.events.clear();
            self.next_seq = 0;
            self.selected_accounts.clear();
            self.desktop_detected_surfaces.clear();
            self.discovery = None;
            self.discovered_views.clear();
            self.discovered_provider_views.clear();
            self.broker_phases.clear();
            self.broker_generations.clear();
        }
        let accounts_path = host_accounts_path(&config.data_dir);
        self.cache.set_accounts_materialize_path(accounts_path);
        self.refresh_floor_secs = config.refresh_floor_secs.max(60);
        self.last_refresh = None;
        self.enabled = enabled;
        // Prove Agent::ALL is covered by primary surfaces.
        for agent in Agent::ALL {
            let surface = HostSurfaceId::from_agent(*agent);
            debug_assert!(
                HostSurfaceId::ALL.contains(&surface),
                "agent {} missing host surface",
                agent.slug()
            );
        }
        let selected_path =
            accounts::selected_accounts_path(&config.data_dir, HOST_USAGE_STATE_REL);
        self.selected_accounts = accounts::load_selected_accounts(&selected_path);
        self.probe_policy = config.probe_policy;
        self.discovery_scope = Some(config.discovery_scope);
        self.discovery = discovery;
        self.discovery_generation = self.discovery_generation.saturating_add(1);
        self.discovered_views.clear();
        self.discovered_provider_views.clear();
        self.desktop_detected_surfaces.clear();
        self.broker_phases.clear();
        self.broker_generations.clear();
        self.data_dir = Some(config.data_dir);
        self.open = true;
        self.push_event("runtime_ready", None, None);
        Ok(())
    }

    /// Shutdown; idempotent.
    pub fn shutdown(&mut self) {
        self.open = false;
        self.last_refresh = None;
        self.events.clear();
        self.discovery = None;
        self.discovery_scope = None;
        self.discovered_views.clear();
        self.discovered_provider_views.clear();
        self.broker_phases.clear();
        self.broker_generations.clear();
        self.canonical_content_id = None;
        self.canonical_projection_cache = None;
    }

    pub(crate) fn require_open(&self) -> Result<(), String> {
        if self.open {
            Ok(())
        } else {
            Err("runtime not open".to_owned())
        }
    }

    pub(crate) fn push_event(
        &mut self,
        kind: &str,
        surface_id: Option<&str>,
        detail: Option<String>,
    ) {
        self.next_seq = self.next_seq.saturating_add(1);
        self.events.push_back(HostUsageEvent {
            sequence: self.next_seq,
            kind: kind.to_owned(),
            surface_id: surface_id.map(str::to_owned),
            detail,
        });
        while self.events.len() > MAX_EVENT_LOG {
            self.events.pop_front();
        }
    }
}
