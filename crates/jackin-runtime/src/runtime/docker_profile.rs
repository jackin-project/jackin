//! Docker security profile resolution and Docker flag emission.
//!
//! Shared serde schema types live in `jackin-core`; this module owns runtime
//! behavior such as grant validation, effective grants, and launch flags.

pub use jackin_core::docker_security::{
    DindGrant, DockerGrants, DockerSecurityProfile, NetworkGrant, ParseProfileError,
};

// ── Valid Linux capability names ─────────────────────────────────────────────

/// All recognized Linux capability names (without the `CAP_` prefix,
/// uppercase). Used to validate `capabilities_add` entries at launch time.
pub const VALID_CAPABILITIES: &[&str] = &[
    "AUDIT_CONTROL",
    "AUDIT_READ",
    "AUDIT_WRITE",
    "BPF",
    "BLOCK_SUSPEND",
    "CHECKPOINT_RESTORE",
    "CHOWN",
    "DAC_OVERRIDE",
    "DAC_READ_SEARCH",
    "FOWNER",
    "FSETID",
    "IPC_LOCK",
    "IPC_OWNER",
    "KILL",
    "LEASE",
    "LINUX_IMMUTABLE",
    "MAC_ADMIN",
    "MAC_OVERRIDE",
    "MKNOD",
    "NET_ADMIN",
    "NET_BIND_SERVICE",
    "NET_BROADCAST",
    "NET_RAW",
    "PERFMON",
    "SETFCAP",
    "SETGID",
    "SETPCAP",
    "SETUID",
    "SYS_ADMIN",
    "SYS_BOOT",
    "SYS_CHROOT",
    "SYS_MODULE",
    "SYS_NICE",
    "SYS_PACCT",
    "SYS_PTRACE",
    "SYS_RAWIO",
    "SYS_RESOURCE",
    "SYS_TIME",
    "SYS_TTY_CONFIG",
    "WAKE_ALARM",
];

/// The 8-cap minimum set applied under `hardened` and `locked`.
///
/// Applied regardless of `DinD` status (with `DinD` active the caps can be
/// circumvented via `docker run --privileged` against the sidecar, but they are
/// still emitted for defense in depth). Derived from common role workflows
/// (package managers, build tools, process supervisors). Everything else is
/// dropped from Docker's 14-cap default.
pub const MINIMUM_CAPABILITIES: &[&str] = &[
    "CHOWN",
    "DAC_OVERRIDE",
    "FOWNER",
    "FSETID",
    "SETUID",
    "SETGID",
    "SETFCAP",
    "KILL",
];

/// Docker's 14-cap default. Applied under `standard` and `compat` (no explicit
/// `--cap-drop` / `--cap-add` flags are emitted for these profiles).
pub const DEFAULT_CAPABILITIES: &[&str] = &[
    "CHOWN",
    "DAC_OVERRIDE",
    "FSETID",
    "FOWNER",
    "MKNOD",
    "NET_RAW",
    "SETGID",
    "SETUID",
    "SETFCAP",
    "SETPCAP",
    "NET_BIND_SERVICE",
    "SYS_CHROOT",
    "KILL",
    "AUDIT_WRITE",
];

// ── Validation ───────────────────────────────────────────────────────────────

/// Errors produced by [`validate_grants`] before any container is started.
#[derive(Debug)]
pub enum GrantValidationError {
    /// `user = "root"` and `sudo = true` are mutually exclusive.
    RootAndSudo,
    /// An entry in `capabilities_add` is not a recognized Linux capability.
    UnknownCapability(String),
    /// `memory_reservation` exceeds `memory` (both provided).
    MemoryReservationExceedsMemory { reservation: u64, memory: u64 },
    /// A size string could not be parsed.
    UnparsableSize { field: &'static str, value: String },
    /// A numeric field is outside its valid range (e.g. `pids <= 0`, memory > `i64::MAX`).
    ValueOutOfRange {
        field: &'static str,
        reason: &'static str,
    },
}

impl std::fmt::Display for GrantValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RootAndSudo => write!(
                f,
                "grants.user = \"root\" and grants.sudo = true are mutually exclusive: \
                 root does not need sudo escalation; remove one of the two grants"
            ),
            Self::UnknownCapability(cap) => {
                write!(
                    f,
                    "unknown Linux capability {cap:?} in grants.capabilities_add — \
                     valid values: {}",
                    VALID_CAPABILITIES.join(", ")
                )
            }
            Self::MemoryReservationExceedsMemory {
                reservation,
                memory,
            } => write!(
                f,
                "grants.memory_reservation ({}) must be ≤ grants.memory ({})",
                format_bytes(*reservation),
                format_bytes(*memory),
            ),
            Self::UnparsableSize { field, value } => write!(
                f,
                "cannot parse {value:?} as a size for grants.{field} — \
                 use format \"512M\", \"4G\", \"32G\""
            ),
            Self::ValueOutOfRange { field, reason } => {
                write!(f, "grants.{field} is out of range: {reason}")
            }
        }
    }
}

impl std::error::Error for GrantValidationError {}

/// Parse a human-readable byte size into a byte count. Case-insensitive suffix.
///
/// Accepts K/M/G/T (with or without trailing B) and bare numeric bytes.
/// Examples: `"512M"`, `"4G"`, `"16G"`, `"2048K"`, `"2T"`.
///
/// Returns `None` if the string is empty, the numeric part cannot be parsed,
/// or the suffix is unrecognized.
pub fn parse_memory_bytes(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    // Find where the numeric part ends.
    let split = s.find(|c: char| c.is_alphabetic()).unwrap_or(s.len());
    let number: u64 = s[..split].trim().parse().ok()?;
    let suffix = s[split..].trim().to_ascii_uppercase();
    let multiplier = match suffix.as_str() {
        "K" | "KB" => 1_024,
        "M" | "MB" => 1_024 * 1_024,
        "G" | "GB" => 1_024 * 1_024 * 1_024,
        "T" | "TB" => 1_024 * 1_024 * 1_024 * 1_024,
        "" => 1,
        _ => return None,
    };
    number.checked_mul(multiplier)
}

fn format_bytes(bytes: u64) -> String {
    const GB: u64 = 1_024 * 1_024 * 1_024;
    const MB: u64 = 1_024 * 1_024;
    const KB: u64 = 1_024;
    if bytes.is_multiple_of(GB) {
        format!("{}G", bytes / GB)
    } else if bytes.is_multiple_of(MB) {
        format!("{}M", bytes / MB)
    } else if bytes.is_multiple_of(KB) {
        format!("{}K", bytes / KB)
    } else {
        format!("{bytes}B")
    }
}

/// Validate explicit grants, returning all errors found (not just the first).
///
/// Called at launch time before any container is started. A non-empty error
/// list aborts the launch with clear, actionable messages.
pub fn validate_grants(grants: &DockerGrants) -> Vec<GrantValidationError> {
    let mut errors = Vec::new();

    // user = "root" + sudo = true is mutually exclusive.
    if grants.user.as_deref() == Some("root") && grants.sudo == Some(true) {
        errors.push(GrantValidationError::RootAndSudo);
    }

    // Validate capability names — reuse normalize_cap to strip CAP_ prefix.
    for cap in &grants.capabilities_add {
        let normalized = normalize_cap(cap);
        if !VALID_CAPABILITIES.contains(&normalized.as_str()) {
            errors.push(GrantValidationError::UnknownCapability(cap.clone()));
        }
    }

    // Parse and validate memory sizes — call parse_memory_bytes once per field.
    let memory_bytes = grants.memory.as_deref().and_then(|s| {
        let v = parse_memory_bytes(s);
        if v.is_none() {
            errors.push(GrantValidationError::UnparsableSize {
                field: "memory",
                value: s.to_owned(),
            });
        }
        v
    });

    let reservation_bytes = grants.memory_reservation.as_deref().and_then(|s| {
        let v = parse_memory_bytes(s);
        if v.is_none() {
            errors.push(GrantValidationError::UnparsableSize {
                field: "memory_reservation",
                value: s.to_owned(),
            });
        }
        v
    });

    if let (Some(res), Some(mem)) = (reservation_bytes, memory_bytes)
        && res > mem
    {
        errors.push(GrantValidationError::MemoryReservationExceedsMemory {
            reservation: res,
            memory: mem,
        });
    }

    // Memory values must fit in i64 for the Bollard/Docker API boundary.
    if let Some(bytes) = memory_bytes
        && bytes > i64::MAX as u64
    {
        errors.push(GrantValidationError::ValueOutOfRange {
            field: "memory",
            reason: "exceeds i64::MAX (≈ 8 EiB); use a value ≤ 8 EiB",
        });
    }
    if let Some(bytes) = reservation_bytes
        && bytes > i64::MAX as u64
    {
        errors.push(GrantValidationError::ValueOutOfRange {
            field: "memory_reservation",
            reason: "exceeds i64::MAX (≈ 8 EiB); use a value ≤ 8 EiB",
        });
    }

    // pids must be positive. Docker uses -1 as "unlimited", but that would
    // disable the limit that hardened/locked profiles are designed to enforce.
    if let Some(pids) = grants.pids
        && pids <= 0
    {
        errors.push(GrantValidationError::ValueOutOfRange {
            field: "pids",
            reason: "must be > 0; omit the field to remove the limit",
        });
    }

    errors
}

/// Normalize a capability name to uppercase without `CAP_` prefix.
fn normalize_cap(cap: &str) -> String {
    let upper = cap.to_ascii_uppercase();
    upper.strip_prefix("CAP_").unwrap_or(&upper).to_owned()
}

// ── Effective grants ─────────────────────────────────────────────────────────

/// Fully resolved grants for a launch — every dimension has a concrete value,
/// produced by merging the profile's defaults with explicit overrides.
#[derive(Debug, Clone)]
pub struct EffectiveGrants {
    pub network: NetworkGrant,
    /// Merged list of allowed hosts for the `allowlist` network tier.
    pub allowed_hosts: Vec<String>,
    pub dind: DindGrant,
    /// Username passed to `--user` on `docker run`. `"agent"` means no
    /// explicit `--user` flag (the image's `USER` directive governs).
    pub user: String,
    pub sudo: bool,
    pub system_writes: bool,
    /// Parsed hard memory limit in bytes. `None` = no limit.
    pub memory_bytes: Option<u64>,
    /// Parsed soft memory limit in bytes. `None` = no soft limit.
    pub memory_reservation_bytes: Option<u64>,
    pub cpus: Option<f64>,
    pub pids: Option<i64>,
    pub nofile: Option<u64>,
    /// Additional capabilities beyond the profile's base set.
    pub capabilities_add: Vec<String>,
    /// Whether `--security-opt no-new-privileges` is applied to the container.
    /// `true` for `hardened` and `locked`; resolved to `true` for `standard`
    /// (WP-SUDO: sudo off by default, so `no_new_privileges` on) unless an explicit
    /// `sudo = true` grant is active. `false` for `compat` (sudo always on).
    pub no_new_privileges: bool,
}

/// Per-profile base grants. Explicit [`DockerGrants`] are layered on top via
/// [`apply_grants`].
pub fn profile_base_grants(profile: DockerSecurityProfile) -> EffectiveGrants {
    const GB: u64 = 1_024 * 1_024 * 1_024;
    match profile {
        DockerSecurityProfile::Locked => EffectiveGrants {
            network: NetworkGrant::Allowlist,
            allowed_hosts: Vec::new(),
            dind: DindGrant::None,
            user: "agent".to_owned(),
            sudo: false,
            system_writes: false,
            memory_bytes: Some(4 * GB),
            memory_reservation_bytes: Some(3 * GB),
            cpus: Some(2.0),
            pids: Some(512),
            nofile: Some(2048),
            capabilities_add: Vec::new(),
            no_new_privileges: true,
        },
        DockerSecurityProfile::Hardened => EffectiveGrants {
            network: NetworkGrant::Allowlist,
            allowed_hosts: Vec::new(),
            dind: DindGrant::None,
            user: "agent".to_owned(),
            sudo: false,
            system_writes: false,
            memory_bytes: Some(16 * GB),
            memory_reservation_bytes: Some(12 * GB),
            cpus: Some(4.0),
            pids: Some(2048),
            nofile: Some(8192),
            capabilities_add: Vec::new(),
            no_new_privileges: true,
        },
        DockerSecurityProfile::Standard => EffectiveGrants {
            network: NetworkGrant::Open,
            allowed_hosts: Vec::new(),
            // WP4: DinD off by default outside `compat` (Decision 12).
            // Enable via explicit `dind = "rootless"` or `dind = "privileged"` grant.
            dind: DindGrant::None,
            user: "agent".to_owned(),
            // WP-SUDO: sudo is off by default outside `compat` (Decision 11).
            // Enable via explicit `sudo = true` grant.
            sudo: false,
            system_writes: true,
            memory_bytes: Some(16 * GB),
            memory_reservation_bytes: Some(12 * GB),
            cpus: Some(4.0),
            pids: Some(2048),
            nofile: Some(8192),
            capabilities_add: Vec::new(),
            // Resolved to `true` by apply_implicit_grants when sudo is false.
            no_new_privileges: false,
        },
        DockerSecurityProfile::Compat => EffectiveGrants {
            network: NetworkGrant::Open,
            allowed_hosts: Vec::new(),
            dind: DindGrant::Privileged,
            user: "agent".to_owned(),
            sudo: true,
            system_writes: true,
            memory_bytes: None,
            memory_reservation_bytes: None,
            cpus: None,
            pids: None,
            nofile: None,
            capabilities_add: Vec::new(),
            no_new_privileges: false,
        },
    }
}

/// Apply explicit grants on top of profile defaults. Each dimension takes the
/// more capable of the profile default and the explicit override.
///
/// Grants must already have been validated by [`validate_grants`].
pub fn apply_grants(mut base: EffectiveGrants, grants: &DockerGrants) -> EffectiveGrants {
    if let Some(network) = grants.network
        && network > base.network
    {
        base.network = network;
    }
    if !grants.allowed_hosts.is_empty() {
        base.allowed_hosts
            .extend(grants.allowed_hosts.iter().cloned());
        base.allowed_hosts.sort_unstable();
        base.allowed_hosts.dedup();
    }
    if let Some(dind) = grants.dind
        && dind > base.dind
    {
        base.dind = dind;
    }
    if let Some(ref user) = grants.user {
        base.user.clone_from(user);
    }
    if let Some(sudo) = grants.sudo {
        base.sudo = base.sudo || sudo;
    }
    if let Some(sw) = grants.system_writes {
        base.system_writes = base.system_writes || sw;
    }
    if let Some(ref mem) = grants.memory
        && let Some(bytes) = parse_memory_bytes(mem)
    {
        base.memory_bytes = Some(base.memory_bytes.map_or(bytes, |b| b.max(bytes)));
    }
    if let Some(ref res) = grants.memory_reservation
        && let Some(bytes) = parse_memory_bytes(res)
    {
        base.memory_reservation_bytes = Some(
            base.memory_reservation_bytes
                .map_or(bytes, |b| b.max(bytes)),
        );
    }
    if let Some(cpus) = grants.cpus {
        base.cpus = Some(base.cpus.map_or(cpus, |c: f64| c.max(cpus)));
    }
    if let Some(pids) = grants.pids {
        base.pids = Some(base.pids.map_or(pids, |p| p.max(pids)));
    }
    if let Some(nofile) = grants.nofile {
        base.nofile = Some(base.nofile.map_or(nofile, |n| n.max(nofile)));
    }
    for cap in &grants.capabilities_add {
        let normalized = normalize_cap(cap);
        if !base.capabilities_add.contains(&normalized) {
            base.capabilities_add.push(normalized);
        }
    }
    base
    // No implicit cap injection here: apply_grants() is a pure layering function.
    // Injecting caps based on the merged network value would fire on every source
    // layer, producing duplicates. apply_implicit_grants() fires once post-merge.
}

// ── Profile resolution ───────────────────────────────────────────────────────

/// Source that produced the active Docker security profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileSource {
    Cli,
    Workspace,
    Config,
    Default,
}

impl std::fmt::Display for ProfileSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cli => write!(f, "cli"),
            Self::Workspace => write!(f, "workspace"),
            Self::Config => write!(f, "config"),
            Self::Default => write!(f, "default"),
        }
    }
}

/// Resolve the effective Docker security profile and its source.
///
/// Precedence (highest to lowest):
/// 1. CLI `--docker-profile` override
/// 2. Workspace `[docker] profile` override
/// 3. Global `[docker] profile` from `config.toml`
/// 4. Compiled-in default (`Compat` until sudo audit resolves)
pub fn resolve_profile(
    cli_override: Option<DockerSecurityProfile>,
    workspace_profile: Option<DockerSecurityProfile>,
    config_default: Option<DockerSecurityProfile>,
) -> (DockerSecurityProfile, ProfileSource) {
    if let Some(p) = cli_override {
        return (p, ProfileSource::Cli);
    }
    if let Some(p) = workspace_profile {
        return (p, ProfileSource::Workspace);
    }
    if let Some(p) = config_default {
        return (p, ProfileSource::Config);
    }
    (DockerSecurityProfile::default(), ProfileSource::Default)
}

/// Validate a fully-resolved [`EffectiveGrants`] for cross-source invariants
/// that per-source [`validate_grants`] cannot catch.
///
/// Returns a list of all violations. An empty list means the grants are valid.
pub fn validate_effective_grants(grants: &EffectiveGrants) -> Vec<GrantValidationError> {
    let mut errors = Vec::new();
    // user="root" + sudo=true can emerge from cross-source merging (e.g.
    // config sets sudo=true, workspace sets user="root") even though per-source
    // validation on each DockerGrants would not catch the combination.
    if grants.user == "root" && grants.sudo {
        errors.push(GrantValidationError::RootAndSudo);
    }
    // memory_reservation > memory can emerge from cross-source merging: each
    // source passes per-source validation independently, but after apply_grants
    // raises each field to its maximum the merged result may violate the constraint.
    if let (Some(res), Some(mem)) = (grants.memory_reservation_bytes, grants.memory_bytes)
        && res > mem
    {
        errors.push(GrantValidationError::MemoryReservationExceedsMemory {
            reservation: res,
            memory: mem,
        });
    }
    errors
}

/// Resolve the effective profile and apply grants, returning the merged
/// [`EffectiveGrants`] for a launch.
///
/// Precedence: config-level grants are applied first, then workspace-level
/// grants layer on top (workspace wins). Both use the same profile base.
pub fn resolve_effective_grants(
    profile: DockerSecurityProfile,
    config_grants: Option<&DockerGrants>,
    workspace_grants: Option<&DockerGrants>,
) -> EffectiveGrants {
    let base = profile_base_grants(profile);
    let after_config = match config_grants {
        Some(g) => apply_grants(base, g),
        None => base,
    };
    let merged = match workspace_grants {
        Some(g) => apply_grants(after_config, g),
        None => after_config,
    };
    // Apply implicit caps that depend on the MERGED network tier. This must
    // run after all source layers are applied so a workspace that raises the
    // network to Allowlist also picks up the required NET_ADMIN + NET_RAW.
    // (profile_base_grants starts with Allowlist for locked/hardened; when
    // no explicit grants are provided apply_grants is never called, so the
    // injection in apply_grants is never triggered — hence this finalization.)
    apply_implicit_grants(merged)
}

/// Apply grants that depend on the fully-resolved state rather than any
/// single source. Called once at the end of `resolve_effective_grants`.
fn apply_implicit_grants(mut grants: EffectiveGrants) -> EffectiveGrants {
    if grants.network == NetworkGrant::Allowlist {
        for cap in ["NET_ADMIN", "NET_RAW"] {
            if !grants.capabilities_add.iter().any(|c| c == cap) {
                grants.capabilities_add.push(cap.to_owned());
            }
        }
    }
    // WP-SUDO: no_new_privileges on whenever sudo is off. Keeps standard
    // sudo-free by default while allowing an explicit `sudo = true` grant to
    // disable it (to avoid the silent-sudo-failure trap with no-new-privileges).
    if !grants.sudo {
        grants.no_new_privileges = true;
    }
    grants
}

// ── Docker flag emission ─────────────────────────────────────────────────────

/// Returns the network enforcement quality label.
///
/// Used for session contract output and `JACKIN_NETWORK_ENFORCEMENT`. Shared
/// between `format_session_contract` and `launch_role_runtime` so both surfaces
/// stay in sync.
pub fn network_enforcement_label(grants: &EffectiveGrants) -> &'static str {
    if !matches!(grants.network, NetworkGrant::Allowlist) {
        return "n/a";
    }
    if grants.sudo || grants.user == "root" {
        "partial (sudo grants iptables access)"
    } else if grants.dind != DindGrant::None {
        "partial (DinD inner containers bypass host iptables)"
    } else {
        "full"
    }
}

/// Format a human-readable session contract table for the active grants.
///
/// Emitted via `crate::debug_log!` at launch; surfaced to the operator in
/// `--debug` mode as a factual summary of what the container can do.
// Eight contract dimensions are one flat argument list by design; bundling them
// into a struct would just move the same fields without aiding any caller.
#[allow(clippy::too_many_arguments)]
pub fn format_session_contract(
    profile: DockerSecurityProfile,
    profile_source: &str,
    grants: &EffectiveGrants,
    apparmor_available: bool,
    apparmor_layer: &str,
    cgroup_version: &str,
    agent_auth_mode: &str,
    gh_auth_forwarded: bool,
) -> String {
    let extra_caps = if grants.capabilities_add.is_empty() {
        String::new()
    } else {
        format!(" + {}", grants.capabilities_add.join(","))
    };
    let caps_line = if drops_all_caps(profile) {
        format!("drop-all + {}{extra_caps}", MINIMUM_CAPABILITIES.join(","))
    } else {
        format!("docker-default (14 caps){extra_caps}")
    };
    let network_mode = match grants.network {
        NetworkGrant::None => "none (--network none)".to_owned(),
        // `allowed_hosts` is only the operator/role-configured set; the launch
        // path also injects the agent's API endpoint(s) and (when forwarded)
        // GitHub into JACKIN_ALLOWED_HOSTS. Report the configured count honestly
        // rather than guessing the injected total with a fixed `+1`.
        NetworkGrant::Allowlist => format!(
            "allowlist ({} configured hosts + agent/GitHub endpoints)",
            grants.allowed_hosts.len()
        ),
        NetworkGrant::Open => "open".to_owned(),
    };
    let network_enforcement = network_enforcement_label(grants);
    let memory_line = grants
        .memory_bytes
        .map_or_else(|| "unlimited".to_owned(), format_bytes);
    let cpus_line = grants
        .cpus
        .map_or_else(|| "unlimited".to_owned(), |c| c.to_string());
    let pids_line = grants
        .pids
        .map_or_else(|| "unlimited".to_owned(), |p| p.to_string());
    let gh_line = if gh_auth_forwarded {
        "forwarded"
    } else {
        "not forwarded"
    };
    let residual_base = "shared host kernel; writable workspace mounts can still be changed";
    let residual = if grants.dind != DindGrant::None {
        format!("{residual_base}; DinD sidecar has kernel access")
    } else if grants.system_writes {
        format!("{residual_base}; writable container root")
    } else {
        residual_base.to_owned()
    };

    format!(
        "Docker profile: {} (source: {})\n\
         Role container:\n  \
           seccomp: docker-default\n  \
           apparmor: {} (layer: {})\n  \
           no-new-privileges: {}\n  \
           capabilities: {}\n  \
           root filesystem: {}\n  \
           writable tmpfs: {}\n\
         DinD:\n  status: {}\n\
         Network:\n  mode: {}\n  enforcement: {}\n\
         cgroup: {}\n\
         Resources:\n  memory: {}\n  cpus: {}\n  pids: {}\n\
         Credentials:\n  agent: {}\n  GitHub CLI: {}\n\
         Residual risk:\n  {}",
        profile,
        profile_source,
        if apparmor_available {
            "docker-default"
        } else {
            "unavailable"
        },
        apparmor_layer,
        if grants.no_new_privileges {
            "enforced"
        } else {
            "not applied"
        },
        caps_line,
        if grants.system_writes {
            "writable"
        } else {
            "read-only"
        },
        if grants.system_writes {
            "none (writable root)".to_owned()
        } else {
            tmpfs_paths(profile).join(",")
        },
        grants.dind, // Display impl emits "none"/"rootless"/"privileged"
        network_mode,
        network_enforcement,
        cgroup_version,
        memory_line,
        cpus_line,
        pids_line,
        agent_auth_mode,
        gh_line,
        residual,
    )
}

/// Default agent API endpoints added to `JACKIN_ALLOWED_HOSTS` when
/// `network = "allowlist"` is active. These are the minimum set required
/// for each agent to reach its model API.
pub fn default_allowed_hosts_for_agent(agent: &str) -> &'static [&'static str] {
    match agent {
        "claude" => &["api.anthropic.com"],
        "codex" => &["api.openai.com"],
        "amp" => &["ampcode.com", "sourcegraph.com"],
        "kimi" => &["api.kimi.com", "kimi.moonshot.cn"],
        "opencode" => &["api.z.ai", "api.anthropic.com", "api.openai.com"],
        _ => &[],
    }
}

/// Emit resource limit Docker CLI flags from resolved grants.
/// Returns an owned `Vec<String>` of alternating flag/value pairs ready to
/// extend a `Vec<&str>` `run_args` via `.iter().map(String::as_str)`.
pub fn resource_flags(grants: &EffectiveGrants) -> Vec<String> {
    let mut flags = Vec::new();
    if let Some(bytes) = grants.memory_bytes {
        flags.push("--memory".to_owned());
        flags.push(bytes.to_string());
    }
    if let Some(bytes) = grants.memory_reservation_bytes {
        flags.push("--memory-reservation".to_owned());
        flags.push(bytes.to_string());
    }
    if let Some(cpus) = grants.cpus {
        flags.push("--cpus".to_owned());
        flags.push(cpus.to_string());
    }
    if let Some(pids) = grants.pids {
        flags.push("--pids-limit".to_owned());
        flags.push(pids.to_string());
    }
    if let Some(nofile) = grants.nofile {
        flags.push("--ulimit".to_owned());
        flags.push(format!("nofile={nofile}:{nofile}"));
    }
    flags
}

/// Emit capability flags for the profile's base cap set.
///
/// Only meaningful when `grants.dind == DindGrant::None` — with `DinD` active,
/// capability drops are circumventable via `docker run --privileged` against
/// the sidecar. Returns empty when the profile uses Docker's default cap set
/// (`standard`/`compat`) to avoid redundant flags.
pub fn capability_flags(profile: DockerSecurityProfile, extra_caps: &[String]) -> Vec<String> {
    let drops_all = drops_all_caps(profile);
    if !drops_all && extra_caps.is_empty() {
        return Vec::new();
    }
    let mut flags = Vec::new();
    if drops_all {
        flags.push("--cap-drop=ALL".to_owned());
        for cap in MINIMUM_CAPABILITIES {
            flags.push("--cap-add".to_owned());
            flags.push(cap.to_string());
        }
    }
    for cap in extra_caps {
        let normalized = normalize_cap(cap);
        flags.push("--cap-add".to_owned());
        flags.push(normalized);
    }
    flags
}

/// Emit `--read-only` and `--tmpfs` flags for profiles that use a read-only
/// root filesystem.
///
/// The tmpfs preset covers paths that tooling writes to at runtime but that
/// are NOT already bind-mounted (`/jackin/run`, `/jackin/state`, and agent
/// home credential dirs are bind mounts and already writable regardless of
/// `--read-only`).
/// Tmpfs paths required for ALL read-only root profiles (hardened and locked).
/// These are the minimum paths needed for any agent session to start — shell
/// session state and the POSIX `/tmp` requirement.
const TMPFS_PATHS_MINIMAL: &[&str] = &[
    "/tmp",
    "/run",
    "/var/run",
    // Shell history and session state — must be writable for the shell to start.
    "/home/agent/.zsh_sessions",
    "/home/agent/.zsh_history",
    "/home/agent/.bash_history",
];

/// Additional tmpfs paths needed by `hardened` profile (roles that do package
/// management at build time but not at runtime). Under `locked`, these are
/// omitted because `apt install` is explicitly unsupported.
const TMPFS_PATHS_HARDENED_EXTRA: &[&str] = &[
    "/var/tmp",
    "/var/cache",
    "/var/log",
    "/var/lib/apt/lists",
    "/var/cache/apt/archives",
    "/var/lib/dpkg",
    "/home/agent/.cache",
];

/// Resolved tmpfs path set for a read-only-root profile.
///
/// `locked` uses the minimal set (apt is unsupported); `hardened` adds
/// package-manager paths. Single source of truth for `--tmpfs` flags and the
/// session contract's "writable tmpfs" line so the two never drift.
pub fn tmpfs_paths(profile: DockerSecurityProfile) -> Vec<&'static str> {
    let extra: &[&str] = if matches!(profile, DockerSecurityProfile::Locked) {
        &[]
    } else {
        TMPFS_PATHS_HARDENED_EXTRA
    };
    TMPFS_PATHS_MINIMAL.iter().chain(extra).copied().collect()
}

/// Emit `--read-only` plus a `--tmpfs <path>:rw,nosuid,nodev` pair for every
/// [`tmpfs_paths`] entry. Empty for writable-root profiles.
pub fn readonly_root_flags(
    profile: DockerSecurityProfile,
    grants: &EffectiveGrants,
) -> Vec<String> {
    if grants.system_writes {
        return Vec::new();
    }
    let mut flags = vec!["--read-only".to_owned()];
    for path in tmpfs_paths(profile) {
        flags.push("--tmpfs".to_owned());
        flags.push(format!("{path}:rw,nosuid,nodev"));
    }
    flags
}

// ── Convenience helpers ──────────────────────────────────────────────────────

/// Returns `true` when the profile uses `--cap-drop=ALL` + minimum cap set.
/// Centralises the Hardened/Locked check so callers don't re-spell it.
pub const fn drops_all_caps(profile: DockerSecurityProfile) -> bool {
    matches!(
        profile,
        DockerSecurityProfile::Hardened | DockerSecurityProfile::Locked
    )
}

/// Returns `true` when the effective grants enable any `DinD` tier.
pub fn dind_enabled(grants: &EffectiveGrants) -> bool {
    grants.dind != DindGrant::None
}

/// Returns `true` when the effective `DinD` tier is `Privileged`.
pub fn dind_privileged(grants: &EffectiveGrants) -> bool {
    grants.dind == DindGrant::Privileged
}

/// WP2: whether the role's Docker network must be created `internal`.
///
/// `locked` runs on a Docker-internal network so traffic cannot leave the
/// bridge even before the in-container iptables allowlist is installed — a
/// second, daemon-level egress boundary independent of `firewall-apply`. Every
/// other profile uses an ordinary (routable) network.
pub const fn role_network_internal(profile: DockerSecurityProfile) -> bool {
    matches!(profile, DockerSecurityProfile::Locked)
}

// ── Host probes (WP3 observability) ─────────────────────────────────────────

/// Detect the cgroup version on the host that will run containers.
///
/// Returns `"v2"`, `"v1"`, or `"hybrid"`.  The check is synchronous and reads
/// from `/sys/fs/cgroup/` — on Linux this file is always readable; on macOS the
/// Docker engine runs in a Linux VM so the host process checks the VM's cgroup
/// namespace through the Docker socket instead (callers should treat an unknown
/// result as `"v2"` on macOS since Docker Desktop always runs cgroup v2).
pub fn probe_cgroup_version() -> &'static str {
    // cgroup v2 has a unified hierarchy — `cgroup.controllers` exists at root.
    if std::path::Path::new("/sys/fs/cgroup/cgroup.controllers").exists() {
        // Hybrid: also has legacy `/sys/fs/cgroup/memory` mounts.
        if std::path::Path::new("/sys/fs/cgroup/memory").exists() {
            return "hybrid";
        }
        return "v2";
    }
    if std::path::Path::new("/sys/fs/cgroup").exists() {
        return "v1";
    }
    // Not Linux (macOS host with Docker Desktop/OrbStack) — inner VM is v2.
    "v2"
}

/// Parse `AppArmor` availability and layer from `docker info --format '{{.SecurityOptions}}'`.
///
/// Returns `(available, layer)` where `layer` is `"host"` or `"backend-vm"`.
/// `"backend-vm"` is reported when the Docker engine runs in a VM (Docker
/// Desktop / `OrbStack` on macOS) because `AppArmor` in the VM does not protect
/// the host's filesystem — it is a weaker boundary than host-native `AppArmor`.
pub fn parse_apparmor_from_docker_info(security_options: &str) -> (bool, &'static str) {
    let available = security_options.contains("apparmor");
    // On Docker Desktop and OrbStack the engine runs in a Linux VM; the host OS
    // is macOS. AppArmor in the VM protects the VM but not the macOS host.
    // Detect by checking whether `/proc/version` contains "Darwin" or whether
    // the security options mention OrbStack/Docker Desktop — but since we only
    // have the `docker info` security string here, we use a simpler heuristic:
    // the host is macOS when `/usr/bin/sw_vers` exists (macOS-only binary).
    let layer = if std::path::Path::new("/usr/bin/sw_vers").exists() {
        "backend-vm"
    } else {
        "host"
    };
    (available, layer)
}

/// Validate cgroup version against profile requirements, returning an error
/// message if the combination is unsupported.
///
/// Decision 14: `hardened`/`locked` require cgroup v2; fail-closed on v1.
/// `standard` degrades `memory_reservation` on v1 (warn only).
pub fn validate_cgroup_for_profile(
    profile: DockerSecurityProfile,
    cgroup_version: &str,
) -> Result<(), String> {
    if cgroup_version == "v1" {
        match profile {
            DockerSecurityProfile::Locked | DockerSecurityProfile::Hardened => {
                return Err(format!(
                    "Docker profile `{profile}` requires cgroup v2 for resource enforcement \
                     (memory limits, pids, cpus), but this host runs cgroup v1. \
                     Upgrade to a cgroup v2 host or use `--docker-profile standard`."
                ));
            }
            DockerSecurityProfile::Standard => {
                jackin_diagnostics::debug_log!(
                    "launch",
                    "cgroup v1 host: memory_reservation will not be enforced under `standard` profile (requires v2)"
                );
            }
            DockerSecurityProfile::Compat => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_roundtrip() {
        for profile in [
            DockerSecurityProfile::Locked,
            DockerSecurityProfile::Hardened,
            DockerSecurityProfile::Standard,
            DockerSecurityProfile::Compat,
        ] {
            let s = profile.to_string();
            let parsed: DockerSecurityProfile = s.parse().unwrap();
            assert_eq!(parsed, profile);
        }
    }

    #[test]
    fn ord_ascending_capability() {
        assert!(DockerSecurityProfile::Locked < DockerSecurityProfile::Hardened);
        assert!(DockerSecurityProfile::Hardened < DockerSecurityProfile::Standard);
        assert!(DockerSecurityProfile::Standard < DockerSecurityProfile::Compat);
    }

    #[test]
    fn default_is_compat() {
        assert_eq!(
            DockerSecurityProfile::default(),
            DockerSecurityProfile::Compat
        );
    }

    #[test]
    fn unknown_profile_is_error() {
        assert!("ultra".parse::<DockerSecurityProfile>().is_err());
    }

    #[test]
    fn resolve_cli_override_wins() {
        let (profile, source) = resolve_profile(
            Some(DockerSecurityProfile::Locked),
            Some(DockerSecurityProfile::Standard),
            Some(DockerSecurityProfile::Compat),
        );
        assert_eq!(profile, DockerSecurityProfile::Locked);
        assert_eq!(source, ProfileSource::Cli);
    }

    #[test]
    fn resolve_workspace_beats_config() {
        let (profile, source) = resolve_profile(
            None,
            Some(DockerSecurityProfile::Hardened),
            Some(DockerSecurityProfile::Compat),
        );
        assert_eq!(profile, DockerSecurityProfile::Hardened);
        assert_eq!(source, ProfileSource::Workspace);
    }

    #[test]
    fn resolve_no_override_returns_default() {
        let (profile, source) = resolve_profile(None, None, None);
        assert_eq!(profile, DockerSecurityProfile::default());
        assert_eq!(source, ProfileSource::Default);
    }

    #[test]
    fn resolve_config_source_tracked() {
        let (profile, source) = resolve_profile(None, None, Some(DockerSecurityProfile::Standard));
        assert_eq!(profile, DockerSecurityProfile::Standard);
        assert_eq!(source, ProfileSource::Config);
    }

    #[test]
    fn parse_memory_bytes_units() {
        assert_eq!(parse_memory_bytes("512M"), Some(512 * 1024 * 1024));
        assert_eq!(parse_memory_bytes("4G"), Some(4 * 1024 * 1024 * 1024));
        assert_eq!(parse_memory_bytes("2048K"), Some(2048 * 1024));
        assert_eq!(parse_memory_bytes("1024"), Some(1024));
        assert_eq!(parse_memory_bytes("4g"), Some(4 * 1024 * 1024 * 1024));
        assert_eq!(parse_memory_bytes("bad"), None);
        assert_eq!(parse_memory_bytes(""), None);
    }

    #[test]
    fn validate_grants_root_and_sudo_error() {
        let grants = DockerGrants {
            user: Some("root".to_owned()),
            sudo: Some(true),
            ..Default::default()
        };
        let errors = validate_grants(&grants);
        assert!(!errors.is_empty());
        assert!(matches!(errors[0], GrantValidationError::RootAndSudo));
    }

    #[test]
    fn validate_grants_unknown_cap_error() {
        let grants = DockerGrants {
            capabilities_add: vec!["MAGIC_CAP".to_owned()],
            ..Default::default()
        };
        let errors = validate_grants(&grants);
        assert!(!errors.is_empty());
        assert!(
            matches!(&errors[0], GrantValidationError::UnknownCapability(s) if s == "MAGIC_CAP")
        );
    }

    #[test]
    fn validate_grants_cap_prefix_stripped() {
        let grants = DockerGrants {
            capabilities_add: vec!["CAP_NET_RAW".to_owned()],
            ..Default::default()
        };
        let errors = validate_grants(&grants);
        assert!(
            errors.is_empty(),
            "CAP_NET_RAW should be valid after stripping prefix"
        );
    }

    #[test]
    fn validate_grants_memory_reservation_exceeds_memory() {
        let grants = DockerGrants {
            memory: Some("4G".to_owned()),
            memory_reservation: Some("8G".to_owned()),
            ..Default::default()
        };
        let errors = validate_grants(&grants);
        assert!(!errors.is_empty());
        assert!(matches!(
            errors[0],
            GrantValidationError::MemoryReservationExceedsMemory { .. }
        ));
    }

    #[test]
    fn validate_grants_valid_passes() {
        let grants = DockerGrants {
            memory: Some("16G".to_owned()),
            memory_reservation: Some("12G".to_owned()),
            cpus: Some(4.0),
            pids: Some(2048),
            nofile: Some(8192),
            capabilities_add: vec!["NET_RAW".to_owned(), "SYS_PTRACE".to_owned()],
            ..Default::default()
        };
        let errors = validate_grants(&grants);
        assert!(errors.is_empty());
    }

    #[test]
    fn resource_flags_full() {
        let grants = EffectiveGrants {
            network: NetworkGrant::Open,
            allowed_hosts: vec![],
            dind: DindGrant::Privileged,
            user: "agent".to_owned(),
            sudo: true,
            system_writes: true,
            memory_bytes: Some(4 * 1024 * 1024 * 1024),
            memory_reservation_bytes: Some(3 * 1024 * 1024 * 1024),
            cpus: Some(2.0),
            pids: Some(512),
            nofile: Some(2048),
            capabilities_add: vec![],
            no_new_privileges: false,
        };
        let flags = resource_flags(&grants);
        assert!(flags.contains(&"--memory".to_owned()));
        assert!(flags.contains(&"--memory-reservation".to_owned()));
        assert!(flags.contains(&"--cpus".to_owned()));
        assert!(flags.contains(&"--pids-limit".to_owned()));
        assert!(flags.contains(&"--ulimit".to_owned()));
    }

    #[test]
    fn resource_flags_empty_for_compat() {
        let grants = profile_base_grants(DockerSecurityProfile::Compat);
        let flags = resource_flags(&grants);
        assert!(flags.is_empty());
    }

    #[test]
    fn capability_flags_hardened_drops_all() {
        let flags = capability_flags(DockerSecurityProfile::Hardened, &[]);
        assert!(flags.contains(&"--cap-drop=ALL".to_owned()));
        for cap in MINIMUM_CAPABILITIES {
            assert!(
                flags.contains(&"--cap-add".to_owned()),
                "missing --cap-add for {cap}"
            );
            assert!(flags.contains(&(*cap).to_owned()));
        }
    }

    #[test]
    fn capability_flags_compat_empty() {
        let flags = capability_flags(DockerSecurityProfile::Compat, &[]);
        assert!(flags.is_empty());
    }

    #[test]
    fn readonly_root_flags_for_locked() {
        let grants = profile_base_grants(DockerSecurityProfile::Locked);
        let flags = readonly_root_flags(DockerSecurityProfile::Locked, &grants);
        assert!(flags.contains(&"--read-only".to_owned()));
        assert!(flags.iter().any(|f| f.starts_with("/tmp")));
    }

    #[test]
    fn readonly_root_flags_empty_for_compat() {
        let grants = profile_base_grants(DockerSecurityProfile::Compat);
        let flags = readonly_root_flags(DockerSecurityProfile::Compat, &grants);
        assert!(flags.is_empty());
    }

    #[test]
    fn apply_grants_raises_network() {
        let base = profile_base_grants(DockerSecurityProfile::Locked);
        assert_eq!(base.network, NetworkGrant::Allowlist);
        let grants = DockerGrants {
            network: Some(NetworkGrant::Open),
            ..Default::default()
        };
        let effective = apply_grants(base, &grants);
        assert_eq!(effective.network, NetworkGrant::Open);
    }

    #[test]
    fn apply_grants_cannot_lower_network() {
        let base = profile_base_grants(DockerSecurityProfile::Standard);
        assert_eq!(base.network, NetworkGrant::Open);
        let grants = DockerGrants {
            network: Some(NetworkGrant::None),
            ..Default::default()
        };
        // Grant is lower than profile default — profile wins.
        let effective = apply_grants(base, &grants);
        assert_eq!(effective.network, NetworkGrant::Open);
    }

    #[test]
    fn network_grant_ord() {
        assert!(NetworkGrant::None < NetworkGrant::Allowlist);
        assert!(NetworkGrant::Allowlist < NetworkGrant::Open);
    }

    #[test]
    fn dind_grant_ord() {
        assert!(DindGrant::None < DindGrant::Rootless);
        assert!(DindGrant::Rootless < DindGrant::Privileged);
    }

    // ── validate_effective_grants ─────────────────────────────────────────────

    #[test]
    fn validate_effective_grants_catches_cross_source_root_and_sudo() {
        let grants = EffectiveGrants {
            user: "root".to_owned(),
            sudo: true,
            ..profile_base_grants(DockerSecurityProfile::Compat)
        };
        let errors = validate_effective_grants(&grants);
        assert!(
            !errors.is_empty(),
            "user=root + sudo=true must be caught by validate_effective_grants"
        );
        assert!(matches!(errors[0], GrantValidationError::RootAndSudo));
    }

    #[test]
    fn validate_effective_grants_catches_cross_source_reservation_exceeds_memory() {
        let grants = EffectiveGrants {
            memory_bytes: Some(4 * 1024 * 1024 * 1024), // 4G
            memory_reservation_bytes: Some(8 * 1024 * 1024 * 1024), // 8G
            ..profile_base_grants(DockerSecurityProfile::Standard)
        };
        let errors = validate_effective_grants(&grants);
        assert!(
            !errors.is_empty(),
            "memory_reservation > memory must be caught by validate_effective_grants"
        );
        assert!(matches!(
            errors[0],
            GrantValidationError::MemoryReservationExceedsMemory { .. }
        ));
    }

    #[test]
    fn validate_effective_grants_passes_when_invariants_hold() {
        let grants = EffectiveGrants {
            memory_bytes: Some(16 * 1024 * 1024 * 1024),
            memory_reservation_bytes: Some(12 * 1024 * 1024 * 1024),
            ..profile_base_grants(DockerSecurityProfile::Standard)
        };
        let errors = validate_effective_grants(&grants);
        assert!(
            errors.is_empty(),
            "valid grants should produce no errors: {errors:?}"
        );
    }

    #[test]
    fn resolve_effective_grants_no_grants_still_gets_implicit_caps() {
        // When locked profile launches with no config/workspace grants,
        // resolve_effective_grants must inject NET_ADMIN/NET_RAW so the
        // iptables allowlist (`jackin-capsule firewall-apply`) can run.
        let grants = resolve_effective_grants(DockerSecurityProfile::Locked, None, None);
        assert_eq!(grants.network, NetworkGrant::Allowlist);
        assert!(
            grants.capabilities_add.iter().any(|c| c == "NET_ADMIN"),
            "Locked with no grants must have implicit NET_ADMIN from resolve_effective_grants"
        );
        assert!(
            grants.capabilities_add.iter().any(|c| c == "NET_RAW"),
            "Locked with no grants must have implicit NET_RAW from resolve_effective_grants"
        );
    }

    // ── Test gap fixes ────────────────────────────────────────────────────────

    /// Extract the path component from every `--tmpfs <path>:opts` pair in a flags vec.
    fn tmpfs_paths_from_flags(flags: &[String]) -> Vec<&str> {
        flags
            .iter()
            .enumerate()
            .filter(|(i, _)| *i > 0 && flags.get(*i - 1).is_some_and(|f| f == "--tmpfs"))
            .map(|(_, v)| v.split(':').next().unwrap_or(""))
            .collect()
    }

    /// locked tmpfs: minimal set only (no package-manager paths).
    #[test]
    fn locked_tmpfs_is_minimal_subset() {
        let grants = profile_base_grants(DockerSecurityProfile::Locked);
        let flags = readonly_root_flags(DockerSecurityProfile::Locked, &grants);
        assert!(
            flags.contains(&"--read-only".to_owned()),
            "locked must be read-only"
        );
        let tmpfs_values = tmpfs_paths_from_flags(&flags);
        assert!(
            tmpfs_values.contains(&"/tmp"),
            "locked must have /tmp tmpfs"
        );
        assert!(
            tmpfs_values.contains(&"/run"),
            "locked must have /run tmpfs"
        );
        // Package-manager paths absent from locked.
        for path in TMPFS_PATHS_HARDENED_EXTRA {
            assert!(
                !tmpfs_values.contains(path),
                "locked must not have {path} (package-manager path, hardened only)"
            );
        }
    }

    /// hardened tmpfs includes both minimal + package-manager paths.
    #[test]
    fn hardened_tmpfs_includes_extra_paths() {
        let grants = profile_base_grants(DockerSecurityProfile::Hardened);
        let flags = readonly_root_flags(DockerSecurityProfile::Hardened, &grants);
        let tmpfs_values = tmpfs_paths_from_flags(&flags);
        for path in TMPFS_PATHS_HARDENED_EXTRA {
            assert!(
                tmpfs_values.contains(path),
                "hardened tmpfs must include {path}"
            );
        }
    }

    /// `network_enforcement_label`: full, partial-sudo, partial-dind, n/a.
    #[test]
    fn network_enforcement_label_all_cases() {
        // n/a for open network.
        let open = EffectiveGrants {
            network: NetworkGrant::Open,
            ..profile_base_grants(DockerSecurityProfile::Standard)
        };
        assert_eq!(network_enforcement_label(&open), "n/a");

        // full: allowlist, no sudo, no dind.
        let full = profile_base_grants(DockerSecurityProfile::Locked);
        assert_eq!(network_enforcement_label(&full), "full");

        // partial: allowlist + sudo.
        let partial_sudo = EffectiveGrants {
            network: NetworkGrant::Allowlist,
            sudo: true,
            ..profile_base_grants(DockerSecurityProfile::Hardened)
        };
        assert_eq!(
            network_enforcement_label(&partial_sudo),
            "partial (sudo grants iptables access)"
        );

        // partial: allowlist + dind active.
        let partial_dind = EffectiveGrants {
            network: NetworkGrant::Allowlist,
            dind: DindGrant::Privileged,
            ..profile_base_grants(DockerSecurityProfile::Hardened)
        };
        assert_eq!(
            network_enforcement_label(&partial_dind),
            "partial (DinD inner containers bypass host iptables)"
        );
    }

    /// Implicit `NET_ADMIN/NET_RAW` caps injected by `resolve_effective_grants` for Allowlist network.
    /// Tests via the public API (`resolve_effective_grants`) so the test exercises the same
    /// path an operator launch uses, not an internal function.
    #[test]
    fn allowlist_network_adds_implicit_caps() {
        // No explicit grants: apply_implicit_grants() in resolve_effective_grants
        // must inject the caps even without any config/workspace DockerGrants.
        let grants = resolve_effective_grants(DockerSecurityProfile::Locked, None, None);
        assert_eq!(grants.network, NetworkGrant::Allowlist);
        assert!(
            grants.capabilities_add.iter().any(|c| c == "NET_ADMIN"),
            "resolve_effective_grants(Locked) must inject implicit NET_ADMIN; got: {:?}",
            grants.capabilities_add
        );
        assert!(
            grants.capabilities_add.iter().any(|c| c == "NET_RAW"),
            "resolve_effective_grants(Locked) must inject implicit NET_RAW; got: {:?}",
            grants.capabilities_add
        );
    }

    /// Implicit caps are also present when hardened profile (Allowlist) has config grants.
    /// `apply_implicit_grants` must fire even when `apply_grants` already ran.
    #[test]
    fn allowlist_network_with_grants_still_gets_implicit_caps() {
        // Hardened profile has Allowlist network; add a memory config grant.
        // apply_grants runs (so it fired), then apply_implicit_grants must still add caps.
        let config_grants = DockerGrants {
            memory: Some("8G".to_owned()),
            ..Default::default()
        };
        let grants =
            resolve_effective_grants(DockerSecurityProfile::Hardened, Some(&config_grants), None);
        assert_eq!(grants.network, NetworkGrant::Allowlist);
        assert!(
            grants.capabilities_add.iter().any(|c| c == "NET_ADMIN"),
            "Hardened with config grants must still have implicit NET_ADMIN; got: {:?}",
            grants.capabilities_add
        );
    }

    /// Grant layering: workspace wins over config when raising.
    #[test]
    fn grant_layering_workspace_wins_over_config() {
        let config_grants = DockerGrants {
            memory: Some("4G".to_owned()),
            cpus: Some(2.0),
            ..Default::default()
        };
        let workspace_grants = DockerGrants {
            memory: Some("16G".to_owned()), // workspace raises memory
            ..Default::default()
        };
        let grants = resolve_effective_grants(
            DockerSecurityProfile::Standard,
            Some(&config_grants),
            Some(&workspace_grants),
        );
        // Workspace memory wins (higher).
        assert_eq!(grants.memory_bytes, Some(16 * 1024 * 1024 * 1024));
        // Config cpus preserved (workspace didn't override).
        assert_eq!(grants.cpus, Some(4.0_f64.max(2.0))); // profile default 4.0 wins over config 2.0
    }

    /// `validate_grants` rejects pids <= 0.
    #[test]
    fn validate_grants_pids_must_be_positive() {
        let grants = DockerGrants {
            pids: Some(-1),
            ..Default::default()
        };
        let errors = validate_grants(&grants);
        assert!(!errors.is_empty(), "pids = -1 should be an error");
        assert!(
            matches!(&errors[0], GrantValidationError::ValueOutOfRange { field, .. } if *field == "pids"),
            "error should be ValueOutOfRange for pids"
        );
    }

    /// `validate_grants` rejects memory exceeding `i64::MAX`.
    #[test]
    fn validate_grants_memory_overflow_is_error() {
        // u64 value > i64::MAX — would silently wrap to negative without the check.
        let huge = format!("{}B", i64::MAX as u64 + 1);
        let grants = DockerGrants {
            memory: Some(huge),
            ..Default::default()
        };
        let errors = validate_grants(&grants);
        assert!(!errors.is_empty(), "memory > i64::MAX should be an error");
    }

    // ── WP-SUDO: profile sudo defaults ───────────────────────────────────────

    #[test]
    fn compat_profile_base_grants_sudo_on() {
        let grants = profile_base_grants(DockerSecurityProfile::Compat);
        assert!(grants.sudo, "compat base grants must have sudo=true");
    }

    #[test]
    fn standard_profile_base_grants_sudo_off() {
        let grants = profile_base_grants(DockerSecurityProfile::Standard);
        assert!(
            !grants.sudo,
            "standard base grants must have sudo=false (WP-SUDO)"
        );
    }

    #[test]
    fn hardened_profile_base_grants_sudo_off() {
        let grants = profile_base_grants(DockerSecurityProfile::Hardened);
        assert!(!grants.sudo, "hardened base grants must have sudo=false");
    }

    #[test]
    fn locked_profile_base_grants_sudo_off() {
        let grants = profile_base_grants(DockerSecurityProfile::Locked);
        assert!(!grants.sudo, "locked base grants must have sudo=false");
    }

    #[test]
    fn explicit_sudo_grant_flips_standard() {
        let config = DockerGrants {
            sudo: Some(true),
            ..Default::default()
        };
        let grants = resolve_effective_grants(DockerSecurityProfile::Standard, Some(&config), None);
        assert!(
            grants.sudo,
            "explicit sudo=true grant must override standard default"
        );
    }

    // ── WP-SUDO: no_new_privileges tied to !sudo ──────────────────────────────

    #[test]
    fn no_new_privileges_on_when_sudo_off() {
        let grants = resolve_effective_grants(DockerSecurityProfile::Standard, None, None);
        assert!(!grants.sudo);
        assert!(
            grants.no_new_privileges,
            "no_new_privileges must be true when sudo=false (standard no-grant)"
        );
    }

    #[test]
    fn no_new_privileges_off_when_sudo_granted() {
        let config = DockerGrants {
            sudo: Some(true),
            ..Default::default()
        };
        let grants = resolve_effective_grants(DockerSecurityProfile::Standard, Some(&config), None);
        assert!(grants.sudo);
        assert!(
            !grants.no_new_privileges,
            "no_new_privileges must be false when sudo=true"
        );
    }

    #[test]
    fn compat_sudo_on_means_no_new_privileges_off() {
        let grants = resolve_effective_grants(DockerSecurityProfile::Compat, None, None);
        assert!(grants.sudo);
        assert!(
            !grants.no_new_privileges,
            "compat profile: sudo=true so no_new_privileges must be false"
        );
    }

    // ── WP4: standard DinD default is None ───────────────────────────────────

    #[test]
    fn standard_profile_base_grants_dind_none() {
        let grants = profile_base_grants(DockerSecurityProfile::Standard);
        assert_eq!(
            grants.dind,
            DindGrant::None,
            "standard dind must default to None (WP4)"
        );
    }

    #[test]
    fn compat_profile_base_grants_dind_privileged() {
        let grants = profile_base_grants(DockerSecurityProfile::Compat);
        assert_eq!(
            grants.dind,
            DindGrant::Privileged,
            "compat keeps privileged DinD"
        );
    }

    #[test]
    fn hardened_profile_base_grants_dind_none() {
        let grants = profile_base_grants(DockerSecurityProfile::Hardened);
        assert_eq!(
            grants.dind,
            DindGrant::None,
            "hardened dind must default to None (Decision 12 / WP4)"
        );
    }

    #[test]
    fn locked_profile_base_grants_dind_none() {
        let grants = profile_base_grants(DockerSecurityProfile::Locked);
        assert_eq!(
            grants.dind,
            DindGrant::None,
            "locked dind must default to None (Decision 12 / WP4)"
        );
    }

    // ── WP3: AppArmor probe parser ────────────────────────────────────────────

    #[test]
    fn parse_apparmor_present_host() {
        let (available, layer) =
            parse_apparmor_from_docker_info("name=apparmor name=seccomp,profile=default");
        assert!(available, "apparmor string should be detected");
        // Layer is host on non-macOS test runner.
        assert!(layer == "host" || layer == "backend-vm");
    }

    #[test]
    fn parse_apparmor_absent() {
        let (available, _layer) = parse_apparmor_from_docker_info("name=seccomp,profile=default");
        assert!(
            !available,
            "should report unavailable when no apparmor token"
        );
    }

    #[test]
    fn parse_apparmor_empty_string() {
        let (available, _layer) = parse_apparmor_from_docker_info("");
        assert!(!available);
    }

    // ── WP3: cgroup validation ────────────────────────────────────────────────

    #[test]
    fn validate_cgroup_compat_accepts_v1() {
        let result = validate_cgroup_for_profile(DockerSecurityProfile::Compat, "v1");
        assert!(result.is_ok(), "compat must accept cgroup v1");
    }

    #[test]
    fn validate_cgroup_standard_warns_on_v1() {
        let result = validate_cgroup_for_profile(DockerSecurityProfile::Standard, "v1");
        assert!(
            result.is_ok(),
            "standard must not hard-fail on cgroup v1 (warn only)"
        );
    }

    #[test]
    fn validate_cgroup_hardened_fails_on_v1() {
        let result = validate_cgroup_for_profile(DockerSecurityProfile::Hardened, "v1");
        assert!(result.is_err(), "hardened must fail-closed on cgroup v1");
    }

    #[test]
    fn validate_cgroup_locked_fails_on_v1() {
        let result = validate_cgroup_for_profile(DockerSecurityProfile::Locked, "v1");
        assert!(result.is_err(), "locked must fail-closed on cgroup v1");
    }

    #[test]
    fn validate_cgroup_hardened_accepts_v2() {
        let result = validate_cgroup_for_profile(DockerSecurityProfile::Hardened, "v2");
        assert!(result.is_ok(), "hardened must accept cgroup v2");
    }

    // ── WP2: locked uses a Docker-internal network ────────────────────────────

    #[test]
    fn role_network_internal_only_for_locked() {
        assert!(
            role_network_internal(DockerSecurityProfile::Locked),
            "locked must run on a Docker-internal network"
        );
        for profile in [
            DockerSecurityProfile::Hardened,
            DockerSecurityProfile::Standard,
            DockerSecurityProfile::Compat,
        ] {
            assert!(
                !role_network_internal(profile),
                "{profile} must use a routable network"
            );
        }
    }
}
