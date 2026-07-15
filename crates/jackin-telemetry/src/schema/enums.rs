// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
// GENERATED from registry/ — do not hand-edit. Regenerate: cargo xtask telemetry-registry --generate.

macro_rules! bounded_values {
    ($name:ident { $($variant:ident => $value:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $name { $($variant),+ }
        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $value),+ }
            }
        }
    };
}

bounded_values!(AppMode { OneShot => "one_shot", Interactive => "interactive", Daemon => "daemon", Capsule => "capsule" });
bounded_values!(OutcomeValue { Success => "success", Failure => "failure", Error => "error", Timeout => "timeout", Skip => "skip", Cancellation => "cancellation" });
bounded_values!(TransitionReason { Action => "action", Launch => "launch", Attach => "attach", Detach => "detach", Back => "back", Cancel => "cancel", Completion => "completion", Failure => "failure", Shutdown => "shutdown" });
bounded_values!(JobType { ImagePrewarm => "image_prewarm", SidecarPrewarm => "sidecar_prewarm" });
bounded_values!(LaunchStageName { Identity => "identity", Role => "role", Credentials => "credentials", Construct => "construct", AgentBinaries => "agent_binaries", DerivedImage => "derived_image", Workspace => "workspace", Network => "network", Sidecar => "sidecar", Capsule => "capsule", Hardline => "hardline" });
bounded_values!(LaunchTargetKind { Workspace => "workspace", Directory => "directory" });
bounded_values!(BackgroundCycleName { BranchContext => "branch_context", PrContext => "pr_context", UsageAccount => "usage_account", ProviderProbe => "provider_probe", InstanceRefresh => "instance_refresh", AgentStatus => "agent_status" });
bounded_values!(ConnectionPeerType { HostDaemon => "host_daemon", CapsuleControl => "capsule_control", CapsuleAttach => "capsule_attach", Docker => "docker", Provider => "provider", Parallax => "parallax" });
bounded_values!(AgentState { Working => "working", Blocked => "blocked", Done => "done", Idle => "idle", Unknown => "unknown" });
bounded_values!(AgentStatusSource { None => "none", VisibleScreen => "visible_screen", ShellIntegration => "shell_integration", ForegroundProcess => "foreground_process", Reported => "reported" });
bounded_values!(AgentStatusConfidence { Unknown => "unknown", Weak => "weak", Strong => "strong", Authoritative => "authoritative" });
bounded_values!(AuthMode { Sync => "sync", ApiKey => "api_key", OauthToken => "oauth_token", Ignore => "ignore" });
bounded_values!(CredentialSourceType { Environment => "environment", AgentHome => "agent_home", Onepassword => "onepassword", GithubCli => "github_cli", OauthStore => "oauth_store", None => "none" });
bounded_values!(WorkspaceIsolationMode { Shared => "shared", Worktree => "worktree", Clone => "clone" });
bounded_values!(NetworkMode { None => "none", Allowlist => "allowlist", Open => "open" });
bounded_values!(DindMode { None => "none", Rootless => "rootless", Privileged => "privileged" });
bounded_values!(ConfigScope { Global => "global", Workspace => "workspace" });
bounded_values!(ConfigOperation { Load => "load", Validate => "validate", Migrate => "migrate", Save => "save" });
bounded_values!(ConfigSchemaVersion { Legacy => "legacy", V1Alpha1 => "v1alpha1", V1Alpha2 => "v1alpha2", V1Alpha3 => "v1alpha3", V1Alpha4 => "v1alpha4", V1Alpha5 => "v1alpha5", V1Alpha6 => "v1alpha6", V1Alpha7 => "v1alpha7", V1Alpha8 => "v1alpha8", V1Alpha9 => "v1alpha9" });
bounded_values!(TrustDecision { Granted => "granted", Revoked => "revoked", Rejected => "rejected" });
bounded_values!(TrustSourceType { Builtin => "builtin", External => "external" });
bounded_values!(CacheName { RoleRepository => "role_repository", AgentBinary => "agent_binary", CapsuleBinary => "capsule_binary", DerivedImage => "derived_image", UsageSnapshot => "usage_snapshot" });
bounded_values!(CacheResult { Hit => "hit", Miss => "miss", Stale => "stale", Reuse => "reuse", Bypass => "bypass" });
bounded_values!(PtyExitReason { Clean => "clean", Signal => "signal", NonzeroExit => "nonzero_exit", WaitFailed => "wait_failed", Cancelled => "cancelled" });
bounded_values!(StreamDirection { Input => "input", Output => "output" });
bounded_values!(TelemetrySignal { Log => "log", Trace => "trace", Metric => "metric" });
bounded_values!(TelemetryRejectionReason { UnknownName => "unknown_name", UnknownAttribute => "unknown_attribute", InvalidValue => "invalid_value", Privacy => "privacy", Cardinality => "cardinality", SizeLimit => "size_limit" });
bounded_values!(AgentName { Claude => "claude", Codex => "codex", Amp => "amp", Kimi => "kimi", Opencode => "opencode", Grok => "grok" });
bounded_values!(ProviderName { Anthropic => "anthropic", Openai => "openai", Amp => "amp", Xai => "xai", Zai => "zai", Minimax => "minimax", Kimi => "kimi" });
bounded_values!(ScreenId { WorkspaceList => "workspace.list", WorkspaceEditor => "workspace.editor", Settings => "settings", WorkspaceCreate => "workspace.create", LaunchProgress => "launch.progress", Capsule => "capsule" });
bounded_values!(CliCommandName { Load => "load", Hardline => "hardline", Eject => "eject", Exile => "exile", Purge => "purge", Prewarm => "prewarm", Prune => "prune", Console => "console", Role => "role", Workspace => "workspace", Config => "config", Daemon => "daemon", Doctor => "doctor", Diagnostics => "diagnostics", Status => "status", Usage => "usage", Help => "help" });
bounded_values!(ErrorType { DockerDaemonUnreachable => "docker_daemon_unreachable", DockerVersionTooOld => "docker_version_too_old", OutOfDiskSpace => "out_of_disk_space", RoleManifestInvalid => "role_manifest_invalid", RoleManifestVersionUnsupported => "role_manifest_version_unsupported", RoleSourceNotTrusted => "role_source_not_trusted", WorkspaceNotFound => "workspace_not_found", WorkspaceConfigVersionUnsupported => "workspace_config_version_unsupported", ContainerNameConflict => "container_name_conflict", DindHealthCheckFailed => "dind_health_check_failed", DindPortConflict => "dind_port_conflict", GhAuthFailed => "gh_auth_failed", OpNotSignedIn => "op_not_signed_in", CapsuleDownloadFailed => "capsule_download_failed", WorktreeConflict => "worktree_conflict", UnsupportedOtlpProtocol => "unsupported_otlp_protocol", Timeout => "timeout", ConnectionRefused => "connection_refused", Panic => "panic" });
