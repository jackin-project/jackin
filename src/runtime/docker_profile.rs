/// Docker security profiles and capability grant model.
///
/// Profiles are ordered ascending by capability: `Locked` is the tightest,
/// `Compat` grants everything. An operator grants up from a locked baseline
/// rather than restricting down from a permissive one.
use serde::{Deserialize, Serialize};

// ── Profile enum ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DockerSecurityProfile {
    /// Minimal — allowlist network, no DinD, no sudo, read-only root, 4G memory.
    /// Purpose-built read-only analysis roles. Highest confidence in container
    /// boundary.
    Locked,
    /// Restricted — allowlist network, no DinD by default, no sudo, read-only
    /// root, 16G memory. For untrusted repos or long autonomous runs where
    /// inner Docker is not needed.
    Hardened,
    /// Typical dev work — open network, DinD, sudo, writable root, 16G memory.
    /// Intended eventual default after the sudo audit.
    Standard,
    /// Maximum compatibility — today's behavior. Privileged DinD, open network,
    /// NOPASSWD:ALL sudo, no resource limits. Explicit opt-in for roles that
    /// need everything.
    Compat,
}

impl Default for DockerSecurityProfile {
    fn default() -> Self {
        // TODO(docker-security-profile-default): flip to `Standard` once the
        // base-image sudo audit resolves — see TODO.md "Docker security profile
        // — flip default from compat to standard".
        Self::Compat
    }
}

impl std::fmt::Display for DockerSecurityProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Locked => write!(f, "locked"),
            Self::Hardened => write!(f, "hardened"),
            Self::Standard => write!(f, "standard"),
            Self::Compat => write!(f, "compat"),
        }
    }
}

impl std::str::FromStr for DockerSecurityProfile {
    type Err = ParseProfileError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "locked" => Ok(Self::Locked),
            "hardened" => Ok(Self::Hardened),
            "standard" => Ok(Self::Standard),
            "compat" => Ok(Self::Compat),
            other => Err(ParseProfileError(other.to_string())),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParseProfileError(String);

impl std::fmt::Display for ParseProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "unknown docker profile {:?} — valid values: locked, hardened, standard, compat",
            self.0
        )
    }
}

impl std::error::Error for ParseProfileError {}

// ── Per-dimension grant enums ────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NetworkGrant {
    None,
    Allowlist,
    Open,
}

impl std::fmt::Display for NetworkGrant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "none"),
            Self::Allowlist => write!(f, "allowlist"),
            Self::Open => write!(f, "open"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DindGrant {
    None,
    Rootless,
    Privileged,
}

impl std::fmt::Display for DindGrant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "none"),
            Self::Rootless => write!(f, "rootless"),
            Self::Privileged => write!(f, "privileged"),
        }
    }
}

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

/// The 8-cap minimum set applied under `hardened` and `locked` — regardless of
/// DinD status (with DinD active the caps can be circumvented via `docker run
/// --privileged` against the sidecar, but they are still emitted for defense in
/// depth). Derived from common role workflows (package managers, build tools,
/// process supervisors). Everything else is dropped from Docker's 14-cap default.
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

// ── Grant overrides struct ───────────────────────────────────────────────────

/// Per-dimension explicit overrides that layer on top of a profile's defaults.
/// All fields are optional — `None` means "use the profile's default for this
/// dimension." Validated at launch time before any container is started.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DockerGrants {
    /// Outbound network tier. Overrides the profile default for this dimension.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<NetworkGrant>,
    /// Extra hosts/IPs added to the `allowlist` network tier (beyond the
    /// auto-assembled agent API endpoints). Each entry is a domain name,
    /// IPv4/IPv6 CIDR, wildcard subdomain (`*.example.com`), or
    /// `domain:port`. Only meaningful when `network = "allowlist"`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_hosts: Vec<String>,
    /// Docker-in-Docker tier. Overrides the profile default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dind: Option<DindGrant>,
    /// User the container process runs as (`"agent"`, `"root"`, or any
    /// username defined in the image). Maps to `--user <value>`.
    /// Default: `"agent"` (host-UID-remapped from the construct image).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// Grant `NOPASSWD:ALL` sudo to the container user.
    /// **Validation error** if `user = "root"` and `sudo = true` are both set —
    /// root does not need sudo escalation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sudo: Option<bool>,
    /// Allow writes to the container's root filesystem (image layer).
    /// `false` = `--read-only` + tmpfs preset; `true` = writable root (default
    /// for `standard`/`compat`). Workspace mounts have their own `readonly`
    /// flag independent of this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_writes: Option<bool>,
    /// Hard memory ceiling (`--memory`). Parsed as a human-readable size:
    /// `"512M"`, `"4G"`, `"32G"`. Omit = no limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<String>,
    /// Soft memory ceiling (`--memory-reservation`). Must be ≤ `memory`
    /// when both are set. Triggers throttling before OOM-kill.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_reservation: Option<String>,
    /// CPU share quota (`--cpus`). `4.0` = 4 full cores. Omit = unlimited.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpus: Option<f64>,
    /// Maximum number of processes (`--pids-limit`). Prevents fork bombs.
    /// Omit = unlimited.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pids: Option<i64>,
    /// Open file descriptor limit (`--ulimit nofile=N:N`). Omit = system
    /// default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nofile: Option<u64>,
    /// Linux capabilities to add beyond the profile's base cap set
    /// (`--cap-add`). Each entry is a cap name without the `CAP_` prefix,
    /// case-insensitive. **Validation error** if any entry is not in
    /// `VALID_CAPABILITIES`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities_add: Vec<String>,
}

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
    /// A numeric field is outside its valid range (e.g. `pids <= 0`, memory > i64::MAX).
    ValueOutOfRange { field: &'static str, reason: &'static str },
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
            Self::ValueOutOfRange { field, reason } => write!(
                f,
                "grants.{field} is out of range: {reason}"
            ),
        }
    }
}

impl std::error::Error for GrantValidationError {}

/// Parse a human-readable byte size into a byte count. Case-insensitive suffix.
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
    let split = s
        .find(|c: char| c.is_alphabetic())
        .unwrap_or(s.len());
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
    if bytes % GB == 0 {
        format!("{}G", bytes / GB)
    } else if bytes % MB == 0 {
        format!("{}M", bytes / MB)
    } else if bytes % KB == 0 {
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
                value: s.to_string(),
            });
        }
        v
    });

    let reservation_bytes = grants.memory_reservation.as_deref().and_then(|s| {
        let v = parse_memory_bytes(s);
        if v.is_none() {
            errors.push(GrantValidationError::UnparsableSize {
                field: "memory_reservation",
                value: s.to_string(),
            });
        }
        v
    });

    if let (Some(res), Some(mem)) = (reservation_bytes, memory_bytes) {
        if res > mem {
            errors.push(GrantValidationError::MemoryReservationExceedsMemory {
                reservation: res,
                memory: mem,
            });
        }
    }

    // Memory values must fit in i64 for the Bollard/Docker API boundary.
    if let Some(bytes) = memory_bytes {
        if bytes > i64::MAX as u64 {
            errors.push(GrantValidationError::ValueOutOfRange {
                field: "memory",
                reason: "exceeds i64::MAX (≈ 8 EiB); use a value ≤ 8 EiB",
            });
        }
    }
    if let Some(bytes) = reservation_bytes {
        if bytes > i64::MAX as u64 {
            errors.push(GrantValidationError::ValueOutOfRange {
                field: "memory_reservation",
                reason: "exceeds i64::MAX (≈ 8 EiB); use a value ≤ 8 EiB",
            });
        }
    }

    // pids must be positive. Docker uses -1 as "unlimited", but that would
    // disable the limit that hardened/locked profiles are designed to enforce.
    if let Some(pids) = grants.pids {
        if pids <= 0 {
            errors.push(GrantValidationError::ValueOutOfRange {
                field: "pids",
                reason: "must be > 0; omit the field to remove the limit",
            });
        }
    }

    errors
}

/// Normalize a capability name to uppercase without `CAP_` prefix.
fn normalize_cap(cap: &str) -> String {
    let upper = cap.to_ascii_uppercase();
    upper.strip_prefix("CAP_").unwrap_or(&upper).to_string()
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
    /// `true` for `hardened` and `locked`; `false` for `standard` (sudo audit
    /// pending — blanket sudo in the base image blocks this, see TODO.md) and
    /// `compat`.
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
            user: "agent".to_string(),
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
            user: "agent".to_string(),
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
            dind: DindGrant::Privileged,
            user: "agent".to_string(),
            sudo: true,
            system_writes: true,
            memory_bytes: Some(16 * GB),
            memory_reservation_bytes: Some(12 * GB),
            cpus: Some(4.0),
            pids: Some(2048),
            nofile: Some(8192),
            capabilities_add: Vec::new(),
            no_new_privileges: false,
        },
        DockerSecurityProfile::Compat => EffectiveGrants {
            network: NetworkGrant::Open,
            allowed_hosts: Vec::new(),
            dind: DindGrant::Privileged,
            user: "agent".to_string(),
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
    if let Some(network) = grants.network {
        if network > base.network {
            base.network = network;
        }
    }
    if !grants.allowed_hosts.is_empty() {
        base.allowed_hosts.extend(grants.allowed_hosts.iter().cloned());
        base.allowed_hosts.sort_unstable();
        base.allowed_hosts.dedup();
    }
    if let Some(dind) = grants.dind {
        if dind > base.dind {
            base.dind = dind;
        }
    }
    if let Some(ref user) = grants.user {
        base.user = user.clone();
    }
    if let Some(sudo) = grants.sudo {
        base.sudo = base.sudo || sudo;
    }
    if let Some(sw) = grants.system_writes {
        base.system_writes = base.system_writes || sw;
    }
    if let Some(ref mem) = grants.memory {
        if let Some(bytes) = parse_memory_bytes(mem) {
            base.memory_bytes = Some(base.memory_bytes.map_or(bytes, |b| b.max(bytes)));
        }
    }
    if let Some(ref res) = grants.memory_reservation {
        if let Some(bytes) = parse_memory_bytes(res) {
            base.memory_reservation_bytes =
                Some(base.memory_reservation_bytes.map_or(bytes, |b| b.max(bytes)));
        }
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
    // `allowlist` network tier requires CAP_NET_ADMIN and CAP_NET_RAW for
    // iptables/ipset to function. These are implicit side-effects of the
    // network grant — not in `capabilities_add` in the TOML, but reported
    // in the session contract as `source=implicit_network_grant`.
    if base.network == NetworkGrant::Allowlist {
        for cap in ["NET_ADMIN", "NET_RAW"] {
            if !base.capabilities_add.iter().any(|c| c == cap) {
                base.capabilities_add.push(cap.to_string());
            }
        }
    }

    base
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
/// 3. Global `[docker] default_profile` from `config.toml`
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
    if let (Some(res), Some(mem)) = (grants.memory_reservation_bytes, grants.memory_bytes) {
        if res > mem {
            errors.push(GrantValidationError::MemoryReservationExceedsMemory {
                reservation: res,
                memory: mem,
            });
        }
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
                grants.capabilities_add.push(cap.to_string());
            }
        }
    }
    grants
}

// ── Docker flag emission ─────────────────────────────────────────────────────

/// Returns the network enforcement quality label for session contract output
/// and `JACKIN_NETWORK_ENFORCEMENT`. Shared between `format_session_contract`
/// and `launch_role_runtime` so both surfaces stay in sync.
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
/// Emitted via `crate::debug_log!` at launch; surfaced to the operator in
/// `--debug` mode as a factual summary of what the container can do.
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
    let caps_line = if drops_all_caps(profile) {
        let extra = if grants.capabilities_add.is_empty() {
            String::new()
        } else {
            format!(" + {}", grants.capabilities_add.join(","))
        };
        format!(
            "drop-all + {}{}",
            MINIMUM_CAPABILITIES.join(","),
            extra
        )
    } else {
        format!(
            "docker-default (14 caps){}",
            if grants.capabilities_add.is_empty() {
                String::new()
            } else {
                format!(" + {}", grants.capabilities_add.join(","))
            }
        )
    };
    let network_mode = match grants.network {
        NetworkGrant::None => "none (--network none)".to_string(),
        NetworkGrant::Allowlist => format!(
            "allowlist ({} hosts)",
            grants.allowed_hosts.len() + 1 // +1 for agent endpoint always included
        ),
        NetworkGrant::Open => "open".to_string(),
    };
    let network_enforcement = network_enforcement_label(grants);
    let memory_line = grants
        .memory_bytes
        .map(format_bytes)
        .unwrap_or_else(|| "unlimited".to_string());
    let cpus_line = grants
        .cpus
        .map(|c| c.to_string())
        .unwrap_or_else(|| "unlimited".to_string());
    let pids_line = grants
        .pids
        .map(|p| p.to_string())
        .unwrap_or_else(|| "unlimited".to_string());
    let gh_line = if gh_auth_forwarded { "forwarded" } else { "not forwarded" };
    let residual = if grants.dind != DindGrant::None {
        "shared host kernel; writable workspace mounts can still be changed; DinD sidecar has kernel access"
    } else if !grants.system_writes {
        "shared host kernel; writable workspace mounts can still be changed"
    } else {
        "shared host kernel; writable workspace mounts can still be changed; writable container root"
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
        if apparmor_available { "docker-default" } else { "unavailable" },
        apparmor_layer,
        if grants.no_new_privileges { "enforced" } else { "not applied" },
        caps_line,
        if grants.system_writes { "writable" } else { "read-only" },
        if grants.system_writes {
            "none (writable root)".to_string()
        } else {
            // Use the same consts as readonly_root_flags so the contract stays in sync.
            let extra: &[&str] = if matches!(profile, DockerSecurityProfile::Locked) {
                &[]
            } else {
                TMPFS_PATHS_HARDENED_EXTRA
            };
            TMPFS_PATHS_MINIMAL
                .iter()
                .chain(extra.iter())
                .copied()
                .collect::<Vec<_>>()
                .join(",")
        },
        grants.dind,  // Display impl emits "none"/"rootless"/"privileged"
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
/// extend a `Vec<&str>` run_args via `.iter().map(String::as_str)`.
pub fn resource_flags(grants: &EffectiveGrants) -> Vec<String> {
    let mut flags = Vec::new();
    if let Some(bytes) = grants.memory_bytes {
        flags.push("--memory".to_string());
        flags.push(bytes.to_string());
    }
    if let Some(bytes) = grants.memory_reservation_bytes {
        flags.push("--memory-reservation".to_string());
        flags.push(bytes.to_string());
    }
    if let Some(cpus) = grants.cpus {
        flags.push("--cpus".to_string());
        flags.push(cpus.to_string());
    }
    if let Some(pids) = grants.pids {
        flags.push("--pids-limit".to_string());
        flags.push(pids.to_string());
    }
    if let Some(nofile) = grants.nofile {
        flags.push("--ulimit".to_string());
        flags.push(format!("nofile={nofile}:{nofile}"));
    }
    flags
}

/// Emit capability flags for the profile's base cap set.
///
/// Only meaningful when `grants.dind == DindGrant::None` — with DinD active,
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
        flags.push("--cap-drop=ALL".to_string());
        for cap in MINIMUM_CAPABILITIES {
            flags.push("--cap-add".to_string());
            flags.push(cap.to_string());
        }
    }
    for cap in extra_caps {
        let normalized = normalize_cap(cap);
        flags.push("--cap-add".to_string());
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

/// Emit `--read-only` and `--tmpfs` flags for profiles that use a read-only root.
///
/// `locked` uses the minimal tmpfs set (no package-manager paths — apt is
/// unsupported under locked). `hardened` adds the full package-manager path set
/// for roles that might call `apt-get update` but not `apt install`.
pub fn readonly_root_flags(
    profile: DockerSecurityProfile,
    grants: &EffectiveGrants,
) -> Vec<String> {
    if grants.system_writes {
        return Vec::new();
    }
    let mut flags = vec!["--read-only".to_string()];

    let extra_paths: &[&str] = if matches!(profile, DockerSecurityProfile::Locked) {
        &[]
    } else {
        TMPFS_PATHS_HARDENED_EXTRA
    };

    for path in TMPFS_PATHS_MINIMAL.iter().chain(extra_paths.iter()) {
        flags.push("--tmpfs".to_string());
        flags.push(format!("{path}:rw,nosuid,nodev"));
    }
    flags
}

// ── Convenience helpers ──────────────────────────────────────────────────────

/// Returns `true` when the profile uses `--cap-drop=ALL` + minimum cap set.
/// Centralises the Hardened/Locked check so callers don't re-spell it.
pub fn drops_all_caps(profile: DockerSecurityProfile) -> bool {
    matches!(
        profile,
        DockerSecurityProfile::Hardened | DockerSecurityProfile::Locked
    )
}

/// Returns `true` when the effective grants enable any DinD tier.
pub fn dind_enabled(grants: &EffectiveGrants) -> bool {
    grants.dind != DindGrant::None
}

/// Returns `true` when the effective DinD tier is `Privileged`.
pub fn dind_privileged(grants: &EffectiveGrants) -> bool {
    grants.dind == DindGrant::Privileged
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
        assert_eq!(DockerSecurityProfile::default(), DockerSecurityProfile::Compat);
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
        let (profile, source) = resolve_profile(
            None,
            None,
            Some(DockerSecurityProfile::Standard),
        );
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
            user: Some("root".to_string()),
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
            capabilities_add: vec!["MAGIC_CAP".to_string()],
            ..Default::default()
        };
        let errors = validate_grants(&grants);
        assert!(!errors.is_empty());
        assert!(matches!(&errors[0], GrantValidationError::UnknownCapability(s) if s == "MAGIC_CAP"));
    }

    #[test]
    fn validate_grants_cap_prefix_stripped() {
        let grants = DockerGrants {
            capabilities_add: vec!["CAP_NET_RAW".to_string()],
            ..Default::default()
        };
        let errors = validate_grants(&grants);
        assert!(errors.is_empty(), "CAP_NET_RAW should be valid after stripping prefix");
    }

    #[test]
    fn validate_grants_memory_reservation_exceeds_memory() {
        let grants = DockerGrants {
            memory: Some("4G".to_string()),
            memory_reservation: Some("8G".to_string()),
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
            memory: Some("16G".to_string()),
            memory_reservation: Some("12G".to_string()),
            cpus: Some(4.0),
            pids: Some(2048),
            nofile: Some(8192),
            capabilities_add: vec!["NET_RAW".to_string(), "SYS_PTRACE".to_string()],
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
            user: "agent".to_string(),
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
        assert!(flags.contains(&"--memory".to_string()));
        assert!(flags.contains(&"--memory-reservation".to_string()));
        assert!(flags.contains(&"--cpus".to_string()));
        assert!(flags.contains(&"--pids-limit".to_string()));
        assert!(flags.contains(&"--ulimit".to_string()));
    }

    #[test]
    fn resource_flags_empty_for_compat() {
        let grants = profile_base_grants(DockerSecurityProfile::Compat);
        let flags = resource_flags(&grants);
        assert!(flags.is_empty());
    }

    #[test]
    fn capability_flags_hardened_drops_all() {
        let flags =
            capability_flags(DockerSecurityProfile::Hardened, &[]);
        assert!(flags.contains(&"--cap-drop=ALL".to_string()));
        for cap in MINIMUM_CAPABILITIES {
            assert!(
                flags.contains(&"--cap-add".to_string()),
                "missing --cap-add for {cap}"
            );
            assert!(flags.contains(&cap.to_string()));
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
        assert!(flags.contains(&"--read-only".to_string()));
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
            user: "root".to_string(),
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
            memory_bytes: Some(4 * 1024 * 1024 * 1024),         // 4G
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
        assert!(errors.is_empty(), "valid grants should produce no errors: {errors:?}");
    }

    #[test]
    fn resolve_effective_grants_no_grants_still_gets_implicit_caps() {
        // When locked profile launches with no config/workspace grants,
        // resolve_effective_grants must inject NET_ADMIN/NET_RAW so the
        // iptables allowlist (init-firewall.sh) can run.
        let grants = resolve_effective_grants(
            DockerSecurityProfile::Locked,
            None,
            None,
        );
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
    fn tmpfs_paths_from_flags<'a>(flags: &'a [String]) -> Vec<&'a str> {
        flags
            .iter()
            .enumerate()
            .filter(|(i, _)| {
                *i > 0 && flags.get(*i - 1).map(|f| f == "--tmpfs").unwrap_or(false)
            })
            .map(|(_, v)| v.split(':').next().unwrap_or(""))
            .collect()
    }

    /// locked tmpfs: minimal set only (no package-manager paths).
    #[test]
    fn locked_tmpfs_is_minimal_subset() {
        let grants = profile_base_grants(DockerSecurityProfile::Locked);
        let flags = readonly_root_flags(DockerSecurityProfile::Locked, &grants);
        assert!(flags.contains(&"--read-only".to_string()), "locked must be read-only");
        let tmpfs_values = tmpfs_paths_from_flags(&flags);
        assert!(tmpfs_values.contains(&"/tmp"), "locked must have /tmp tmpfs");
        assert!(tmpfs_values.contains(&"/run"), "locked must have /run tmpfs");
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

    /// network_enforcement_label: full, partial-sudo, partial-dind, n/a.
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

    /// Implicit NET_ADMIN/NET_RAW caps injected when apply_grants sees network = allowlist.
    #[test]
    fn allowlist_network_adds_implicit_caps() {
        // The implicit caps are injected inside apply_grants when network == Allowlist.
        // Trigger it by applying an empty grants struct over a locked base.
        let base = profile_base_grants(DockerSecurityProfile::Locked);
        assert_eq!(base.network, NetworkGrant::Allowlist);
        let resolved = apply_grants(base, &DockerGrants::default());
        assert!(
            resolved.capabilities_add.iter().any(|c| c == "NET_ADMIN"),
            "apply_grants over Allowlist network must inject implicit NET_ADMIN; got: {:?}",
            resolved.capabilities_add
        );
        assert!(
            resolved.capabilities_add.iter().any(|c| c == "NET_RAW"),
            "apply_grants over Allowlist network must inject implicit NET_RAW; got: {:?}",
            resolved.capabilities_add
        );
    }

    /// Grant layering: workspace wins over config when raising.
    #[test]
    fn grant_layering_workspace_wins_over_config() {
        let config_grants = DockerGrants {
            memory: Some("4G".to_string()),
            cpus: Some(2.0),
            ..Default::default()
        };
        let workspace_grants = DockerGrants {
            memory: Some("16G".to_string()),  // workspace raises memory
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

    /// validate_grants rejects pids <= 0.
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

    /// validate_grants rejects memory exceeding i64::MAX.
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
}
