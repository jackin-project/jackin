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
        "K" | "KB" => KB,
        "M" | "MB" => MB,
        "G" | "GB" => GB,
        "T" | "TB" => GB * 1_024,
        "" => 1,
        _ => return None,
    };
    number.checked_mul(multiplier)
}

const KB: u64 = 1_024;
const MB: u64 = KB * 1_024;
const GB: u64 = MB * 1_024;

fn format_bytes(bytes: u64) -> String {
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

/// Parse a `--memory`-style size grant and range-check it for the Docker/Bollard
/// `i64` boundary, pushing the matching validation error on failure. Returns the
/// parsed bytes (even when out of range) so cross-field comparisons can proceed.
fn parse_size_field(
    errors: &mut Vec<GrantValidationError>,
    field: &'static str,
    value: Option<&str>,
) -> Option<u64> {
    let raw = value?;
    let Some(bytes) = parse_memory_bytes(raw) else {
        errors.push(GrantValidationError::UnparsableSize {
            field,
            value: raw.to_owned(),
        });
        return None;
    };
    if bytes > i64::MAX as u64 {
        errors.push(GrantValidationError::ValueOutOfRange {
            field,
            reason: "exceeds i64::MAX (≈ 8 EiB); use a value ≤ 8 EiB",
        });
    }
    Some(bytes)
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

    // Parse + range-check the two memory size fields (identical rules: a parse
    // failure records UnparsableSize; a value over i64::MAX records ValueOutOfRange
    // for the Bollard/Docker API boundary). Returns the parsed value either way so
    // the reservation-vs-memory comparison below still runs.
    let memory_bytes = parse_size_field(&mut errors, "memory", grants.memory.as_deref());
    let reservation_bytes = parse_size_field(
        &mut errors,
        "memory_reservation",
        grants.memory_reservation.as_deref(),
    );

    if let (Some(res), Some(mem)) = (reservation_bytes, memory_bytes)
        && res > mem
    {
        errors.push(GrantValidationError::MemoryReservationExceedsMemory {
            reservation: res,
            memory: mem,
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

    // cpus must be finite and positive. A non-finite (NaN/inf) or non-positive
    // value survives raise_to_max (NaN fails every `>=` compare, so it is kept)
    // and reaches `--cpus <value>`, failing opaquely at `docker run` instead of
    // at this launch-time gate.
    if let Some(cpus) = grants.cpus
        && (!cpus.is_finite() || cpus <= 0.0)
    {
        errors.push(GrantValidationError::ValueOutOfRange {
            field: "cpus",
            reason: "must be a finite value > 0; omit the field to remove the limit",
        });
    }

    // nofile = 0 emits `--ulimit nofile=0:0`, forbidding the container from
    // opening any file descriptor — a launch that cannot function. Reject it.
    if grants.nofile == Some(0) {
        errors.push(GrantValidationError::ValueOutOfRange {
            field: "nofile",
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
    /// Configured container username. Only `"root"` is load-bearing — it is
    /// compared against `sudo` for the mutually-exclusive check and feeds the
    /// network-enforcement label; the default `"agent"` is an inert sentinel.
    /// The actual `--user` flag is governed by `identity::host_run_as_user`, not
    /// this field.
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
/// Raise `slot` to `candidate` when it's larger (or unset). Grants only ever
/// widen a resource ceiling, never lower it.
fn raise_to_max<T: PartialOrd>(slot: &mut Option<T>, candidate: T) {
    match slot {
        Some(existing) if *existing >= candidate => {}
        _ => *slot = Some(candidate),
    }
}

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
        raise_to_max(&mut base.memory_bytes, bytes);
    }
    if let Some(ref res) = grants.memory_reservation
        && let Some(bytes) = parse_memory_bytes(res)
    {
        raise_to_max(&mut base.memory_reservation_bytes, bytes);
    }
    if let Some(cpus) = grants.cpus {
        raise_to_max(&mut base.cpus, cpus);
    }
    if let Some(pids) = grants.pids {
        raise_to_max(&mut base.pids, pids);
    }
    if let Some(nofile) = grants.nofile {
        raise_to_max(&mut base.nofile, nofile);
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

/// Layer a role manifest's docker grants onto resolved effective grants.
///
/// [`apply_grants`] raises dind/network/hosts/caps (never lowers); then the role
/// may pin dind back to `None` — the **only** down-force in the grant system,
/// which `apply_grants` cannot express. A role that forbids `DinD` must override an
/// otherwise more-capable profile/config/workspace tier.
pub fn fold_role_grants(effective: EffectiveGrants, role: &DockerGrants) -> EffectiveGrants {
    let mut folded = apply_grants(effective, role);
    if role.dind == Some(DindGrant::None) {
        folded.dind = DindGrant::None;
    }
    folded
}

/// Whether the resolved profile satisfies a role's `min_profile` floor: at least
/// as capable as `min` in the ascending-capability [`DockerSecurityProfile`] Ord.
///
/// Note the direction: a floor of `hardened` rejects `locked` (locked is *more*
/// restrictive, *less* capable) and accepts `standard`/`compat`.
pub fn profile_meets_floor(resolved: DockerSecurityProfile, min: DockerSecurityProfile) -> bool {
    resolved >= min
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
/// 4. Compiled-in default (`Compat` until the WP6 flip; WP-SUDO removed the
///    sudo-audit blocker)
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
    // WP-SUDO: no_new_privileges is exactly the negation of the resolved sudo
    // grant. Set it bidirectionally so an explicit `sudo = true` under a profile
    // whose base is `no_new_privileges: true` (hardened/locked) actually clears
    // it — otherwise sudo is provisioned but no-new-privileges blocks the setuid
    // escalation, the silent-sudo-failure trap. Resolved post-merge so the final
    // sudo value governs.
    grants.no_new_privileges = !grants.sudo;
    grants
}

// ── Docker flag emission ─────────────────────────────────────────────────────

/// The `JACKIN_NETWORK_MODE` / contract label for a network tier. Delegates to
/// [`NetworkGrant::as_str`] so the label tracks the serde vocabulary.
pub fn network_grant_label(network: NetworkGrant) -> &'static str {
    network.as_str()
}

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
    } else if dind_enabled(grants) {
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
    let residual = if dind_enabled(grants) {
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
        "grok" => &["api.x.ai"],
        _ => &[],
    }
}

/// Fixed GitHub egress endpoints added to the allowlist when a GitHub token is
/// forwarded, plus the operator's enterprise `GH_HOST` when set. Sibling policy
/// to [`default_allowed_hosts_for_agent`] — the GitHub half of the egress set.
pub fn github_allowlist_hosts(gh_host: Option<&str>) -> Vec<String> {
    let mut hosts = vec!["github.com".to_owned(), "api.github.com".to_owned()];
    if let Some(host) = gh_host {
        hosts.push(host.to_owned());
    }
    hosts
}

/// WP1: assemble the full egress allowlist injected as `JACKIN_ALLOWED_HOSTS`.
///
/// Union of: operator/role-configured `grants.allowed_hosts`, the agent's
/// default API endpoint(s), any forwarded GitHub host(s), and the OTLP
/// telemetry endpoint host — deduplicated, order-preserving. The OTLP host is
/// jackin'-owned infrastructure egress (Decision 9): it is always present when
/// telemetry is active and is not operator-removable, so the capsule keeps
/// exporting under `hardened`/`locked`.
///
/// The result is fail-closed by construction: an empty union under
/// `network = allowlist` yields a DROP-only policy in `firewall-apply` (no
/// egress), never open egress.
pub fn allowlist_hosts(
    agent: &str,
    grants: &EffectiveGrants,
    github_hosts: &[String],
    otlp_host: Option<&str>,
) -> Vec<String> {
    let mut hosts: Vec<String> = Vec::new();
    let mut push = |h: &str| {
        let h = h.trim();
        if !h.is_empty() && !hosts.iter().any(|existing| existing == h) {
            hosts.push(h.to_owned());
        }
    };
    for h in &grants.allowed_hosts {
        push(h);
    }
    for h in default_allowed_hosts_for_agent(agent) {
        push(h);
    }
    for h in github_hosts {
        push(h);
    }
    if let Some(h) = otlp_host {
        push(h);
    }
    hosts
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

/// Container env that redirects tools writing under `$HOME` onto a writable
/// location when the profile's root filesystem is read-only. Empty for
/// writable-root profiles.
///
/// The env-redirect arm of the read-only-root `$HOME` story (the in-place
/// writable-path arm is [`tmpfs_paths`]). `git config --global` can't be fixed
/// with a tmpfs/bind on `~/.gitconfig` alone because it writes a `.gitconfig.lock`
/// in the read-only home dir, so it is pointed at the already-writable
/// `/jackin/state` bind mount instead. (The full `$HOME` audit is tracked on the
/// Docker hardening roadmap item.)
pub fn readonly_home_env(grants: &EffectiveGrants) -> Vec<String> {
    if grants.system_writes {
        return Vec::new();
    }
    vec!["GIT_CONFIG_GLOBAL=/jackin/state/gitconfig".to_owned()]
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

/// Returns `true` when the container gets no Docker network at all (`--network
/// none`): the `none` tier with no `DinD` sidecar needing the bridge.
pub fn network_disabled(grants: &EffectiveGrants) -> bool {
    grants.network == NetworkGrant::None && !dind_enabled(grants)
}

/// Returns `true` when the effective `DinD` tier is `Privileged`.
pub fn dind_privileged(grants: &EffectiveGrants) -> bool {
    grants.dind == DindGrant::Privileged
}

/// WP4 Part B: the sidecar image and `--privileged` flag for a `DinD` tier.
///
/// `rootless` runs `docker:dind-rootless` in a user namespace with no
/// `--privileged`; `privileged` runs `docker:dind` with `--privileged`. `none`
/// never starts a sidecar — it maps to the privileged pair only as an
/// unreachable default (the caller gates on `dind_enabled`).
pub const fn dind_image_and_privileged(grant: DindGrant) -> (&'static str, bool) {
    match grant {
        DindGrant::Rootless => ("docker:dind-rootless", false),
        DindGrant::Privileged | DindGrant::None => ("docker:dind", true),
    }
}

/// WP4 Part B: rootless `DinD` requires cgroup v2.
///
/// Fails closed on a cgroup-v1 host rather than silently falling back to a
/// privileged sidecar (which would defeat the operator's choice). Other tiers
/// impose no cgroup requirement here (the profile-level cgroup gate is separate,
/// see [`validate_cgroup_for_profile`]).
pub fn validate_dind_grant_for_cgroup(
    grant: DindGrant,
    cgroup_version: &str,
) -> Result<(), String> {
    if grant == DindGrant::Rootless && cgroup_version == "v1" {
        return Err(
            "rootless DinD requires cgroup v2 for user-namespace isolation; this host is cgroup v1. \
             Use `dind = \"privileged\"` or run on a cgroup v2 host — jackin' will not silently fall \
             back to a privileged sidecar."
                .to_owned(),
        );
    }
    Ok(())
}

/// In-container path of the capsule binary, used for post-run `docker exec`.
pub const CAPSULE_BIN_PATH: &str = "/jackin/runtime/jackin-capsule";

/// `docker exec --user root <container> <capsule> <subcommand>` argv.
///
/// Root via `exec` needs no setuid, so it composes with `no-new-privileges`.
/// Shared by the post-run privileged capsule steps (firewall, sudo).
fn capsule_root_exec_argv<'a>(container_name: &'a str, subcommand: &'a str) -> [&'a str; 6] {
    [
        "exec",
        "--user",
        "root",
        container_name,
        CAPSULE_BIN_PATH,
        subcommand,
    ]
}

/// WP1: the post-run `docker exec` argv that installs the egress allowlist, or
/// `None` when the profile does not enforce one (`open`/`none` install no
/// firewall). Fail-closed at the call site.
pub fn firewall_post_run_argv<'a>(
    grants: &EffectiveGrants,
    container_name: &'a str,
) -> Option<[&'a str; 6]> {
    (grants.network == NetworkGrant::Allowlist)
        .then(|| capsule_root_exec_argv(container_name, "firewall-apply"))
}

/// WP-SUDO: the post-run `docker exec` argv that provisions sudo. Only run when
/// the profile grants sudo (`compat`, or an explicit `sudo = true`); the base
/// image bakes no sudoers, so non-sudo profiles have nothing to provision.
pub fn sudo_provision_post_run_argv(container_name: &str) -> [&str; 6] {
    capsule_root_exec_argv(container_name, "sudo-provision")
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
    // On a macOS host (Docker Desktop / OrbStack) the engine runs in a Linux VM,
    // so AppArmor protects the VM but not the host. `/usr/bin/sw_vers` is a
    // macOS-only binary, so its presence flags the backend-VM layer.
    let layer = if std::path::Path::new("/usr/bin/sw_vers").exists() {
        "backend-vm"
    } else {
        "host"
    };
    (available, layer)
}

/// Validate cgroup version against profile requirements. `Err` = unsupported
/// (fail-closed); `Ok(Some(warning))` = supported but degraded, for the caller
/// to surface; `Ok(None)` = fully supported.
///
/// Decision 14: `hardened`/`locked` require cgroup v2; fail-closed on v1.
/// `standard` degrades `memory_reservation` on v1 (warn only). Pure policy — the
/// caller owns telemetry emission.
pub fn validate_cgroup_for_profile(
    profile: DockerSecurityProfile,
    cgroup_version: &str,
) -> Result<Option<&'static str>, String> {
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
                return Ok(Some(
                    "cgroup v1 host: memory_reservation will not be enforced under `standard` profile (requires v2)",
                ));
            }
            DockerSecurityProfile::Compat => {}
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests;
