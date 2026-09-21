//! Per-session process and filesystem isolation.
//!
//! The capsule supervisor is trusted. Agent sessions are not: they receive a
//! unique numeric identity, retain only the two DAC capabilities needed to
//! work in host bind mounts, and are confined with Landlock before `exec`.
//! Landlock is required rather than best-effort because DAC override would
//! otherwise let one slot walk into another slot's bind mount.
//!
//! The session boundary is intentionally layered. On Landlock ABI 9 and
//! newer, pathname Unix-socket resolution is denied outside explicitly
//! writable session roots. Older kernels only get the ABI-3 filesystem rules;
//! the daemon's kernel peer-UID plus per-session bearer capability then gates
//! the control socket. Mutable setup, temporary files, and git metadata live
//! below one root allocated for the exact PTY session. There is no agent-child
//! grant for capsule-wide state, `/tmp`, or shared GitHub CLI credentials.

// Landlock/capability syscalls are the fail-closed isolation boundary; unsafe
// is confined to this module and reviewed as a security API. Linux-only: no
// unsafe code remains on other targets, and an unfulfilled expect would fail
// the build there.
#![cfg_attr(
    target_os = "linux",
    expect(
        unsafe_code,
        reason = "fail-closed Landlock/capability isolation boundary"
    )
)]

#[cfg(target_os = "linux")]
use anyhow::Context;
use anyhow::{Result, bail};
#[cfg(target_os = "linux")]
use jackin_protocol::SessionIdentity;

/// Dispatch the internal session wrapper.
///
/// Arguments are: `<instance-or-> <uid> <gid> <program> [args...]`.
/// The wrapper is intentionally not a public user-facing command.
///
/// # Errors
///
/// Returns an error when the session identity is not admitted, the required
/// Linux isolation boundary cannot be installed, or the target cannot be
/// executed.
pub fn run_isolated_command(args: &[String]) -> Result<()> {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = args;
        bail!("isolated agent sessions require a Linux Landlock boundary");
    }

    #[cfg(target_os = "linux")]
    {
        if args.len() < 4 {
            bail!("isolated session wrapper requires instance, uid, gid, and program");
        }
        let instance = (args[0] != "-").then_some(args[0].as_str());
        let uid = args[1]
            .parse::<u32>()
            .context("invalid isolated session uid")?;
        let gid = args[2]
            .parse::<u32>()
            .context("invalid isolated session gid")?;
        let identity = SessionIdentity { uid, gid };
        let program = &args[3];
        let program_args = &args[4..];

        let config = crate::config::load()?;
        let expected = admitted_identity(&config, instance);
        anyhow::ensure!(
            expected == Some(identity),
            "isolated session identity is not admitted for this target"
        );

        linux::run(&config, instance, identity, program, program_args)
    }
}

#[cfg(target_os = "linux")]
fn admitted_identity(
    config: &jackin_protocol::CapsuleConfig,
    instance: Option<&str>,
) -> Option<SessionIdentity> {
    match instance {
        Some(id) => config.identity_for_instance(id),
        None => config.shell_identity,
    }
}

#[cfg(all(test, target_os = "linux"))]
mod admission_tests {
    use super::admitted_identity;
    use jackin_protocol::{CapsuleConfig, SessionIdentity};
    use std::collections::BTreeMap;

    #[test]
    fn unknown_instance_never_falls_back_to_shell_identity() {
        let config = CapsuleConfig {
            shell_identity: Some(SessionIdentity {
                uid: 3000,
                gid: 3000,
            }),
            instance_identities: BTreeMap::from([(
                "known".to_owned(),
                SessionIdentity {
                    uid: 3001,
                    gid: 3001,
                },
            )]),
            ..CapsuleConfig::default()
        };

        assert_eq!(
            admitted_identity(&config, Some("known")),
            config.instance_identities.get("known").copied()
        );
        assert_eq!(admitted_identity(&config, Some("missing")), None);
        assert_eq!(admitted_identity(&config, None), config.shell_identity);
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::SessionIdentity;
    use anyhow::{Context, Result, bail};
    use jackin_protocol::CapsuleConfig;
    use std::ffi::CString;
    use std::fs;
    use std::mem::size_of;
    use std::os::unix::fs::PermissionsExt as _;
    use std::os::unix::process::CommandExt as _;
    use std::path::{Path, PathBuf};

    const CAP_DAC_OVERRIDE: u32 = 1;
    const CAP_VERSION_3: u32 = 0x2008_0522;
    const PR_SET_KEEPCAPS: libc::c_int = 8;
    const PR_SET_NO_NEW_PRIVS: libc::c_int = 38;
    const PR_CAP_AMBIENT: libc::c_int = 47;
    const PR_CAP_AMBIENT_RAISE: libc::c_ulong = 2;
    const LANDLOCK_CREATE_RULESET_VERSION: libc::c_uint = 1;
    const LANDLOCK_RULE_TYPE_PATH_BENEATH: libc::c_uint = 1;

    const ACCESS_EXECUTE: u64 = 1 << 0;
    const ACCESS_WRITE_FILE: u64 = 1 << 1;
    const ACCESS_READ_FILE: u64 = 1 << 2;
    const ACCESS_READ_DIR: u64 = 1 << 3;
    const ACCESS_REMOVE_DIR: u64 = 1 << 4;
    const ACCESS_REMOVE_FILE: u64 = 1 << 5;
    const ACCESS_MAKE_CHAR: u64 = 1 << 6;
    const ACCESS_MAKE_DIR: u64 = 1 << 7;
    const ACCESS_MAKE_REG: u64 = 1 << 8;
    const ACCESS_MAKE_SOCK: u64 = 1 << 9;
    const ACCESS_MAKE_FIFO: u64 = 1 << 10;
    const ACCESS_MAKE_BLOCK: u64 = 1 << 11;
    const ACCESS_MAKE_SYM: u64 = 1 << 12;
    const ACCESS_REFER: u64 = 1 << 13;
    const ACCESS_TRUNCATE: u64 = 1 << 14;
    const ACCESS_RESOLVE_UNIX: u64 = 1 << 16;
    const LANDLOCK_ABI_RESOLVE_UNIX: libc::c_long = 9;

    pub(super) const TRAVERSE: u64 = ACCESS_EXECUTE;
    pub(super) const READ_FILE_ONLY: u64 = ACCESS_EXECUTE | ACCESS_READ_FILE;
    const READ_ONLY: u64 = READ_FILE_ONLY | ACCESS_READ_DIR;
    // `std::process::Stdio::null()` opens the null device read/write even
    // when it is used for stdin. Keep the device tree read-only and grant
    // only this exact character device the file I/O needed by that primitive.
    const NULL_DEVICE: u64 = READ_FILE_ONLY | ACCESS_WRITE_FILE;
    const WRITABLE: u64 = ACCESS_WRITE_FILE
        | ACCESS_REMOVE_DIR
        | ACCESS_REMOVE_FILE
        | ACCESS_MAKE_CHAR
        | ACCESS_MAKE_DIR
        | ACCESS_MAKE_REG
        | ACCESS_MAKE_SOCK
        | ACCESS_MAKE_FIFO
        | ACCESS_MAKE_BLOCK
        | ACCESS_MAKE_SYM
        | ACCESS_REFER
        | ACCESS_TRUNCATE;
    pub(super) const FULL: u64 = READ_ONLY | WRITABLE;
    const FULL_WITH_UNIX: u64 = FULL | ACCESS_RESOLVE_UNIX;
    const READ_ONLY_WITH_UNIX: u64 = READ_ONLY | ACCESS_RESOLVE_UNIX;

    // Pass only the ABI-1 prefix to create_ruleset. ABI 3 accepts this
    // prefix, and this boundary does not use the later network/scoped fields;
    // passing the full newer struct would make ABI-3 support depend on the
    // kernel accepting fields introduced after the advertised ABI.
    #[repr(C)]
    pub(super) struct RulesetAttr {
        handled_access_fs: u64,
    }

    #[repr(C, packed)]
    pub(super) struct PathBeneathAttr {
        allowed_access: u64,
        parent_fd: libc::c_int,
    }

    #[repr(C)]
    struct CapUserHeader {
        version: u32,
        pid: libc::pid_t,
    }

    #[repr(C)]
    struct CapUserData {
        effective: u32,
        permitted: u32,
        inheritable: u32,
    }

    #[derive(Debug, Clone)]
    pub(super) struct Rule {
        pub(super) path: PathBuf,
        pub(super) access: u64,
        pub(super) required: bool,
    }

    pub(super) fn run(
        config: &CapsuleConfig,
        instance: Option<&str>,
        identity: SessionIdentity,
        program: &str,
        args: &[String],
    ) -> Result<()> {
        // SAFETY: `geteuid` has no pointer arguments and only returns the
        // effective uid of the calling process.
        let effective_uid = unsafe { libc::geteuid() };
        anyhow::ensure!(
            effective_uid == 0,
            "capsule isolation wrapper must start as root"
        );
        let session_id = std::env::var(jackin_protocol::ISOLATION_SESSION_ID_ENV)
            .context("isolated session wrapper requires JACKIN_ISOLATION_SESSION_ID")?
            .parse::<u64>()
            .context("isolated session wrapper has invalid JACKIN_ISOLATION_SESSION_ID")?;
        anyhow::ensure!(session_id > 0, "isolated session id cannot be zero");
        let session_root = session_root_path(session_id);
        prepare_session_root(&session_root)?;
        prepare_pane_homes_parent(config, instance)?;
        let cwd = std::env::current_dir().context("resolve isolated session cwd")?;
        let rules = rules_for(config, instance, &cwd, &session_root)?;
        drop_privileges(identity)?;
        install_landlock(&rules)?;

        let error = std::process::Command::new(program).args(args).exec();
        Err(error).with_context(|| format!("exec isolated session program {program}"))
    }

    pub(super) fn rules_for(
        config: &CapsuleConfig,
        instance: Option<&str>,
        cwd: &Path,
        session_root: &Path,
    ) -> Result<Vec<Rule>> {
        rules_for_impl(config, instance, cwd, session_root, true)
    }

    #[cfg(test)]
    pub(super) fn rules_for_test(
        config: &CapsuleConfig,
        instance: Option<&str>,
        cwd: &Path,
        session_root: &Path,
    ) -> Result<Vec<Rule>> {
        rules_for_impl(config, instance, cwd, session_root, false)
    }

    fn rules_for_impl(
        config: &CapsuleConfig,
        instance: Option<&str>,
        cwd: &Path,
        session_root: &Path,
        require_runtime_files: bool,
    ) -> Result<Vec<Rule>> {
        let cwd = validate_cwd_boundary(config, cwd)?;
        let session_root = normalize_existing_path(session_root)?;
        let mut rules = Vec::new();
        // The workspace and selected slot roots may contain legitimate
        // process-local Unix sockets. Sensitive `/jackin/run` sockets never
        // receive this bit.
        required_exact_rule(&mut rules, &cwd, FULL_WITH_UNIX);
        required_exact_rule(&mut rules, &session_root, FULL_WITH_UNIX);
        for mount in &config.workspace_mounts {
            let mount = validate_workspace_mount_boundary(config, mount)?;
            // `:ro` dsts are still enforced by the bind mount itself; the
            // Landlock grant may be write-capable.
            required_exact_rule(&mut rules, &mount, FULL_WITH_UNIX);
        }
        for target in &config.worktree_git_targets {
            let target = validate_worktree_git_target(target)?;
            required_exact_rule(&mut rules, &target, FULL_WITH_UNIX);
        }
        if require_runtime_files {
            for path in [
                format!(
                    "{}/entrypoint.sh",
                    jackin_core::container_paths::RUNTIME_DIR
                ),
                format!(
                    "{}/jackin-capsule",
                    jackin_core::container_paths::RUNTIME_DIR
                ),
            ] {
                required_exact_rule(&mut rules, Path::new(&path), READ_ONLY);
            }
        }
        for path in [
            format!("{}/hooks", jackin_core::container_paths::RUNTIME_DIR),
            format!("{}/agent-status", jackin_core::container_paths::RUNTIME_DIR),
        ] {
            optional_exact_rule(&mut rules, Path::new(&path), READ_ONLY);
        }
        for path in [
            "/bin", "/usr", "/lib", "/lib64", "/sbin", "/etc", "/dev", "/sys", "/var",
        ] {
            optional_exact_rule(&mut rules, Path::new(path), READ_ONLY);
        }
        // Recovery/runtime setup uses jackin-process's StdioMode::Null for
        // non-interactive children. Its stdin fd is opened O_RDWR, so a
        // read-only /dev rule is insufficient. This is the narrow device
        // exception; no other device path receives write access.
        optional_exact_rule(&mut rules, Path::new("/dev/null"), NULL_DEVICE);
        // Do not grant a broad /proc read rule: selected credentials are
        // transported in the child environment, and /proc/<pid>/environ would
        // otherwise let a DAC-capable sibling read them. Programs may inspect
        // only their own proc tree when the image provides these magic links.
        for path in ["/proc/self", "/proc/thread-self"] {
            optional_exact_rule(&mut rules, Path::new(path), READ_ONLY);
        }
        // There is no broad /tmp grant. TMPDIR/TMP/TEMP point into the exact
        // session root; an agent trying the host/container /tmp is denied.

        // Image-baked tools and shell configuration are shared, but are not
        // account slots. They are read-only. Slot roots below are the only
        // mutable account paths outside the private session root.
        for path in [
            "/home/agent/.oh-my-zsh",
            "/home/agent/.local/bin",
            "/home/agent/.local/share/mise",
            "/home/agent/.local/state/mise",
            "/home/agent/.cache/mise",
            "/home/agent/.config/fish",
            "/home/agent/.config/git",
            "/home/agent/.config/mise",
            "/home/agent/.amp/bin",
            "/home/agent/.antigravity/bin",
            "/home/agent/.cursor-agent/bin",
            "/home/agent/.gemini-cli/bin",
            "/home/agent/.grok/bin",
            "/home/agent/.hermes/bin",
            "/home/agent/.kimi-code/bin",
            "/home/agent/.muse/bin",
            "/home/agent/.omp/bin",
            "/home/agent/.opencode/bin",
            "/home/agent/.gitconfig",
            "/home/agent/.zshrc",
            "/home/agent/.zshenv",
        ] {
            optional_exact_rule(&mut rules, Path::new(path), READ_ONLY);
        }
        for path in [
            jackin_core::container_paths::CAPSULE_CONFIG,
            jackin_core::container_paths::USAGE_ACCOUNTS,
        ] {
            optional_exact_rule(&mut rules, Path::new(path), READ_ONLY);
        }
        // These are exact socket inodes, not writable roots. ABI 9 needs the
        // explicit pathname-socket right for connect(2); older kernels strip
        // it in `access_for_abi` and retain the ABI-3 fail-closed filesystem
        // policy.
        for path in [
            jackin_core::container_paths::CAPSULE_SOCKET,
            jackin_core::container_paths::HOST_SOCK,
            jackin_core::container_paths::USAGE_SOCK,
        ] {
            optional_exact_rule(&mut rules, Path::new(path), READ_ONLY_WITH_UNIX);
        }
        // Docker clients in the agent session use the mounted DinD client
        // certificates. The parent `/jackin/run` rule is traverse-only, so
        // this exact mount must be readable without exposing other runtime
        // state or sockets.
        optional_exact_rule(
            &mut rules,
            Path::new(jackin_core::container_paths::DIND_CERTS_CLIENT_DIR),
            READ_ONLY,
        );
        optional_exact_rule(
            &mut rules,
            Path::new(jackin_core::container_paths::CLIPBOARD_DIR),
            READ_ONLY,
        );

        if let Some(instance) = instance {
            let paths = config.mount_paths_for_instance(instance);
            anyhow::ensure!(
                !paths.is_empty(),
                "admitted instance has no private home/auth mount paths"
            );
            for path in paths {
                let path = normalize_existing_path(Path::new(path))?;
                let access = if path.is_dir() {
                    FULL_WITH_UNIX
                } else {
                    // Forwarded auth files are Docker/Apple read-only mounts;
                    // keep the Landlock grant read-only too.
                    READ_FILE_ONLY
                };
                required_exact_rule(&mut rules, &path, access);
            }
            // Concurrent same-instance sessions run in derived pane homes
            // (`{home}/panes/{seq}`). The parent sits outside the base mount
            // grants for XDG-parent homes, so it gets its own grant; the
            // wrapper pre-created it before dropping privileges, and requiring
            // it here fails closed with the path named when that breaks.
            let panes = pane_homes_parent(config, instance)?;
            required_exact_rule(&mut rules, &panes, FULL_WITH_UNIX);
            // Runtime setup seeds a fresh home — base or derived pane home —
            // from the image snapshot of this agent's own default fragment(s).
            // Never the snapshot root itself: it holds every agent's defaults.
            // The fragment is keyed by the agent runtime, not by the instance
            // home suffix: seed reads `credential_dir` for every home shape,
            // so a suffix-derived grant would miss and fail the seed closed.
            if let Some(slug) = config.agents.get(instance)
                && let Some(agent) = jackin_core::Agent::from_slug(slug)
            {
                let state = agent.runtime().state_paths();
                let mut fragments = vec![state.credential_dir];
                fragments.extend(state.config_dir);
                for fragment in fragments {
                    optional_exact_rule(
                        &mut rules,
                        &Path::new(jackin_core::container_paths::DEFAULT_HOME_DIR).join(fragment),
                        READ_ONLY,
                    );
                }
            }
        }
        Ok(rules)
    }

    /// Existing cwd aliases are canonicalized before any recursive Landlock
    /// grant is created. The grant must not overlap capsule-owned roots or any
    /// private mount destination, including a path supplied by a stale or
    /// hostile launch config.
    fn normalize_existing_path(path: &Path) -> Result<PathBuf> {
        anyhow::ensure!(path.is_absolute(), "isolated session path must be absolute");
        let normalized = jackin_core::container_paths::normalize_path(path);
        match fs::canonicalize(&normalized) {
            Ok(path) => Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(normalized),
            Err(error) => Err(error.into()),
        }
    }

    fn validate_cwd_boundary(config: &CapsuleConfig, cwd: &Path) -> Result<PathBuf> {
        let lexical_cwd = jackin_core::container_paths::normalize_path(cwd);
        let cwd = normalize_existing_path(&lexical_cwd)?;
        ensure_no_protected_overlap(config, "isolated session cwd", &lexical_cwd, &cwd)?;
        Ok(cwd)
    }

    fn validate_workspace_mount_boundary(config: &CapsuleConfig, mount: &str) -> Result<PathBuf> {
        let lexical_mount = jackin_core::container_paths::normalize_path(Path::new(mount));
        let mount = normalize_existing_path(&lexical_mount)?;
        ensure_no_protected_overlap(
            config,
            "isolated session workspace mount",
            &lexical_mount,
            &mount,
        )?;
        Ok(mount)
    }

    /// Aux git dirs live under `/jackin/host` by construction
    /// (`/jackin/host/<dst>/.git`), a subtree the workspace boundary policy
    /// can never admit. Grant exactly strict descendants of that root so git
    /// can follow the worktree gitdir pointer; anything else fails closed.
    fn validate_worktree_git_target(target: &str) -> Result<PathBuf> {
        anyhow::ensure!(
            !target.split('/').any(|component| component == ".."),
            "isolated session worktree git target {target} must not contain .."
        );
        let lexical_target = jackin_core::container_paths::normalize_path(Path::new(target));
        let target = normalize_existing_path(&lexical_target)?;
        for candidate in [&lexical_target, &target] {
            anyhow::ensure!(
                is_strict_descendant(candidate, Path::new(jackin_core::container_paths::HOST_DIR)),
                "isolated session worktree git target {} is outside {}",
                candidate.display(),
                jackin_core::container_paths::HOST_DIR
            );
        }
        Ok(target)
    }

    fn is_strict_descendant(path: &Path, root: &Path) -> bool {
        let path = jackin_core::container_paths::normalize_path(path);
        let root = jackin_core::container_paths::normalize_path(root);
        path != root && jackin_core::container_paths::path_is_ancestor_or_equal(&root, &path)
    }

    fn ensure_no_protected_overlap(
        config: &CapsuleConfig,
        label: &str,
        lexical: &Path,
        canonical: &Path,
    ) -> Result<()> {
        for protected_root in ["/home/agent", jackin_core::container_paths::JACKIN_ROOT] {
            let lexical_root =
                jackin_core::container_paths::normalize_path(Path::new(protected_root));
            anyhow::ensure!(
                !jackin_core::container_paths::paths_overlap(lexical, &lexical_root),
                "{label} {} overlaps protected root {}",
                lexical.display(),
                lexical_root.display()
            );
            let protected_root = normalize_existing_path(&lexical_root)?;
            anyhow::ensure!(
                !jackin_core::container_paths::paths_overlap(canonical, &protected_root),
                "{label} {} overlaps protected root {}",
                canonical.display(),
                protected_root.display()
            );
        }
        for (instance, paths) in &config.instance_mount_paths {
            for path in paths {
                let lexical_mount = jackin_core::container_paths::normalize_path(Path::new(path));
                anyhow::ensure!(
                    !jackin_core::container_paths::paths_overlap(lexical, &lexical_mount),
                    "{label} {} overlaps protected mount destination {} for instance {instance}",
                    lexical.display(),
                    lexical_mount.display()
                );
                let mount = normalize_existing_path(&lexical_mount)?;
                anyhow::ensure!(
                    !jackin_core::container_paths::paths_overlap(canonical, &mount),
                    "{label} {} overlaps protected mount destination {} for instance {instance}",
                    canonical.display(),
                    mount.display()
                );
            }
        }
        Ok(())
    }

    fn session_root_path(session_id: u64) -> PathBuf {
        Path::new(jackin_core::container_paths::SESSION_ROOTS_DIR).join(session_id.to_string())
    }

    /// Lexical `{home}/panes` parent for an instance's derived pane homes.
    /// Every admitted instance carries a home entry; a missing entry fails
    /// the spawn closed (the daemon enforces the same invariant).
    fn pane_homes_parent(config: &CapsuleConfig, instance: &str) -> Result<PathBuf> {
        let home = config.instance_home_dirs.get(instance).with_context(|| {
            format!("admitted instance {instance} has no home dir for pane homes")
        })?;
        let home = normalize_existing_path(Path::new(home))?;
        Ok(home.join(jackin_core::container_paths::PANE_HOMES_DIR_NAME))
    }

    /// Create the instance's `{home}/panes` parent before dropping UID and
    /// installing Landlock. The host mount provides the home itself; only the
    /// leaf is ever created here. A symlink or non-directory at the leaf fails
    /// closed (a same-instance session could otherwise redirect the next
    /// spawn's grant outside its home), as does a leaf that resolves outside
    /// the home. Shell sessions carry no instance home and skip this.
    fn prepare_pane_homes_parent(config: &CapsuleConfig, instance: Option<&str>) -> Result<()> {
        let Some(instance) = instance else {
            return Ok(());
        };
        let home = config.instance_home_dirs.get(instance).with_context(|| {
            format!("admitted instance {instance} has no home dir for pane homes")
        })?;
        let home = normalize_existing_path(Path::new(home))?;
        anyhow::ensure!(
            home.is_dir(),
            "isolated pane homes require an existing instance home dir: {}",
            home.display()
        );
        let panes = home.join(jackin_core::container_paths::PANE_HOMES_DIR_NAME);
        match fs::symlink_metadata(&panes) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                bail!(
                    "isolated pane homes parent is a symlink: {}",
                    panes.display()
                )
            }
            Ok(metadata) if !metadata.is_dir() => {
                bail!(
                    "isolated pane homes parent is not a directory: {}",
                    panes.display()
                )
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&panes)
                    .with_context(|| format!("create isolated pane homes {}", panes.display()))?;
                fs::set_permissions(&panes, fs::Permissions::from_mode(0o700))
                    .with_context(|| format!("lock isolated pane homes {}", panes.display()))?;
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("inspect isolated pane homes {}", panes.display()));
            }
        }
        let canonical_home = fs::canonicalize(&home)
            .with_context(|| format!("resolve instance home {}", home.display()))?;
        let canonical_panes = fs::canonicalize(&panes)
            .with_context(|| format!("resolve isolated pane homes {}", panes.display()))?;
        anyhow::ensure!(
            canonical_panes.starts_with(&canonical_home),
            "isolated pane homes {} escape instance home {}",
            canonical_panes.display(),
            canonical_home.display()
        );
        Ok(())
    }

    /// Create the private tree before dropping UID and installing Landlock.
    /// Session ids are process-local and restart from one, so a stale root is
    /// removed only after rejecting symlinks/non-directories. The root is
    /// never accepted from child input.
    fn prepare_session_root(root: &Path) -> Result<()> {
        match fs::symlink_metadata(root) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                bail!("isolated session root is a symlink: {}", root.display())
            }
            Ok(metadata) if !metadata.is_dir() => {
                bail!(
                    "isolated session root is not a directory: {}",
                    root.display()
                )
            }
            Ok(_) => fs::remove_dir_all(root)
                .with_context(|| format!("clear stale isolated session root {}", root.display()))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("inspect isolated session root {}", root.display()));
            }
        }
        fs::create_dir_all(root)
            .with_context(|| format!("create isolated session root {}", root.display()))?;
        for child in ["state", "tmp", "runtime", "cache"] {
            fs::create_dir(root.join(child)).with_context(|| {
                format!("create isolated session path {}/{}", root.display(), child)
            })?;
        }
        fs::set_permissions(root, fs::Permissions::from_mode(0o700))
            .with_context(|| format!("lock isolated session root {}", root.display()))?;
        Ok(())
    }

    fn required_exact_rule(rules: &mut Vec<Rule>, path: &Path, access: u64) {
        add_execute_only_ancestors(rules, path);
        rules.push(Rule {
            path: path.to_path_buf(),
            access,
            required: true,
        });
    }

    fn optional_exact_rule(rules: &mut Vec<Rule>, path: &Path, access: u64) {
        add_execute_only_ancestors(rules, path);
        rules.push(Rule {
            path: path.to_path_buf(),
            access,
            required: false,
        });
    }

    /// Permit path lookup only. Every readable or writable location is added
    /// separately above; no parent rule may accidentally expose a sibling
    /// home, default-home fragment, runtime secret, or credential directory.
    pub(super) fn add_execute_only_ancestors(rules: &mut Vec<Rule>, path: &Path) {
        for ancestor in path.ancestors().skip(1) {
            rules.push(Rule {
                path: ancestor.to_path_buf(),
                access: TRAVERSE,
                required: false,
            });
        }
    }

    pub(super) fn install_landlock(rules: &[Rule]) -> Result<()> {
        // SAFETY: Landlock's version query is defined as a null attribute
        // pointer with zero size and a version-query flag.
        let abi = unsafe {
            libc::syscall(
                libc::SYS_landlock_create_ruleset,
                std::ptr::null::<RulesetAttr>(),
                0usize,
                LANDLOCK_CREATE_RULESET_VERSION,
            )
        };
        if abi < 3 {
            bail!("Landlock ABI 3 is required for credential isolation; kernel reported {abi}");
        }
        let handled = RulesetAttr {
            handled_access_fs: FULL
                | if abi >= LANDLOCK_ABI_RESOLVE_UNIX {
                    ACCESS_RESOLVE_UNIX
                } else {
                    0
                },
        };
        // SAFETY: `handled` is a valid, initialized ruleset attribute and its
        // size matches the ABI structure passed to the kernel.
        let ruleset = unsafe {
            libc::syscall(
                libc::SYS_landlock_create_ruleset,
                &handled,
                size_of::<RulesetAttr>(),
                0usize,
            )
        };
        if ruleset < 0 {
            return Err(std::io::Error::last_os_error())
                .context("create required Landlock credential boundary");
        }
        let ruleset = i32::try_from(ruleset).context("Landlock ruleset fd out of range")?;
        for rule in rules {
            if !rule.path.exists() {
                if rule.required {
                    close_fd(ruleset);
                    bail!(
                        "required isolated path does not exist: {}",
                        rule.path.display()
                    );
                }
                continue;
            }
            let path = CString::new(rule.path.as_os_str().as_encoded_bytes())
                .with_context(|| format!("invalid isolated path {}", rule.path.display()))?;
            // SAFETY: `path` is a NUL-terminated path owned for this call;
            // O_PATH|O_CLOEXEC requests only a kernel path handle.
            let parent = unsafe { libc::open(path.as_ptr(), libc::O_PATH | libc::O_CLOEXEC) };
            if parent < 0 {
                close_fd(ruleset);
                return Err(std::io::Error::last_os_error())
                    .with_context(|| format!("open isolated path {}", rule.path.display()));
            }
            let beneath = PathBeneathAttr {
                // File-only rules cannot carry directory creation/removal or
                // READ_DIR rights. Most rules are directories, but selected
                // auth mounts and runtime files are exact regular files.
                allowed_access: if rule.path.is_dir() {
                    access_for_abi(rule.access, abi)
                } else {
                    access_for_abi(rule.access, abi)
                        & (ACCESS_EXECUTE
                            | ACCESS_WRITE_FILE
                            | ACCESS_READ_FILE
                            | ACCESS_TRUNCATE
                            | ACCESS_RESOLVE_UNIX)
                },
                parent_fd: parent,
            };
            // SAFETY: `beneath` is a valid path-beneath attribute whose fd is
            // open for the duration of the syscall.
            let result = unsafe {
                libc::syscall(
                    libc::SYS_landlock_add_rule,
                    ruleset,
                    LANDLOCK_RULE_TYPE_PATH_BENEATH,
                    &beneath,
                    0u32,
                )
            };
            close_fd(parent);
            if result < 0 {
                close_fd(ruleset);
                return Err(std::io::Error::last_os_error()).with_context(|| {
                    format!("install isolated path rule {}", rule.path.display())
                });
            }
        }
        // SAFETY: `prctl` changes only the calling process's no-new-privs
        // attribute and receives no pointer arguments.
        if unsafe { libc::prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0 {
            close_fd(ruleset);
            return Err(std::io::Error::last_os_error())
                .context("enable no-new-privileges for Landlock");
        }
        // SAFETY: `ruleset` is the valid fd returned by Landlock and remains
        // open until this syscall completes.
        let result = unsafe { libc::syscall(libc::SYS_landlock_restrict_self, ruleset, 0u32) };
        close_fd(ruleset);
        if result < 0 {
            return Err(std::io::Error::last_os_error())
                .context("activate required Landlock credential boundary");
        }
        Ok(())
    }

    fn access_for_abi(access: u64, abi: libc::c_long) -> u64 {
        if abi >= LANDLOCK_ABI_RESOLVE_UNIX {
            access
        } else {
            access & !ACCESS_RESOLVE_UNIX
        }
    }

    pub(super) fn drop_privileges(identity: SessionIdentity) -> Result<()> {
        anyhow::ensure!(
            identity.uid > 0 && identity.gid > 0,
            "session identity cannot be root"
        );
        // SAFETY: `prctl` changes only this process's keep-caps flag and
        // receives no pointer arguments.
        if unsafe { libc::prctl(PR_SET_KEEPCAPS, 1, 0, 0, 0) } != 0 {
            return Err(std::io::Error::last_os_error()).context("retain session DAC capabilities");
        }
        // SAFETY: a zero count with a null group list is the documented way
        // to clear supplementary groups for this process.
        if unsafe { libc::setgroups(0, std::ptr::null()) } != 0 {
            return Err(std::io::Error::last_os_error())
                .context("clear supervisor supplementary groups");
        }
        // SAFETY: all three gid values are the validated non-root session gid.
        if unsafe { libc::setresgid(identity.gid, identity.gid, identity.gid) } != 0 {
            return Err(std::io::Error::last_os_error()).context("drop session gid");
        }
        // SAFETY: all three uid values are the validated non-root session uid.
        if unsafe { libc::setresuid(identity.uid, identity.uid, identity.uid) } != 0 {
            return Err(std::io::Error::last_os_error()).context("drop session uid");
        }

        let mask = retained_capability_mask();
        let header = CapUserHeader {
            version: CAP_VERSION_3,
            pid: 0,
        };
        let mut data = [
            CapUserData {
                effective: mask,
                permitted: mask,
                inheritable: mask,
            },
            CapUserData {
                effective: 0,
                permitted: 0,
                inheritable: 0,
            },
        ];
        // SAFETY: `header` and the two-element capability data array are
        // initialized to the kernel's documented capset ABI.
        if unsafe { libc::syscall(libc::SYS_capset, &header, data.as_mut_ptr()) } != 0 {
            return Err(std::io::Error::last_os_error()).context("retain session DAC capabilities");
        }
        for capability in [CAP_DAC_OVERRIDE] {
            // SAFETY: this raises the one capability just installed in the
            // calling process's ambient set.
            if unsafe { libc::prctl(PR_CAP_AMBIENT, PR_CAP_AMBIENT_RAISE, capability, 0, 0) } != 0 {
                return Err(std::io::Error::last_os_error())
                    .context("make session DAC boundary capabilities survive exec");
            }
        }
        // SAFETY: `geteuid` has no pointer arguments and reports this process.
        let effective_uid = unsafe { libc::geteuid() };
        anyhow::ensure!(
            effective_uid == identity.uid,
            "session uid drop did not stick"
        );
        // SAFETY: `getegid` has no pointer arguments and reports this process.
        let effective_gid = unsafe { libc::getegid() };
        anyhow::ensure!(
            effective_gid == identity.gid,
            "session gid drop did not stick"
        );
        Ok(())
    }

    /// DAC override is retained only because host bind mounts can be owned by
    /// a different numeric UID than the per-session identity. Landlock remains
    /// the path boundary. `CAP_FOWNER` is deliberately not retained: no session
    /// operation needs to bypass ownership checks for chmod/chown/signal-like
    /// ownership actions.
    pub(super) const fn retained_capability_mask() -> u32 {
        1u32 << CAP_DAC_OVERRIDE
    }

    fn close_fd(fd: libc::c_int) {
        // SAFETY: callers pass file descriptors returned by the kernel and no
        // longer use them after this close.
        unsafe {
            libc::close(fd);
        }
    }

    // Keep production constants/functions private. The Linux-only unit tests
    // live beside, rather than inside, this implementation module, so expose
    // a test-only view instead of widening the production API.
    #[cfg(test)]
    pub(super) mod test_support {
        pub(crate) const ACCESS_RESOLVE_UNIX: u64 = super::ACCESS_RESOLVE_UNIX;
        pub(crate) const FULL_WITH_UNIX: u64 = super::FULL_WITH_UNIX;
        pub(crate) const NULL_DEVICE: u64 = super::NULL_DEVICE;
        pub(crate) const READ_ONLY: u64 = super::READ_ONLY;
        pub(crate) const READ_ONLY_WITH_UNIX: u64 = super::READ_ONLY_WITH_UNIX;
        pub(crate) const WRITABLE: u64 = super::WRITABLE;

        pub(crate) fn access_for_abi(access: u64, abi: libc::c_long) -> u64 {
            super::access_for_abi(access, abi)
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::linux::{
        FULL, READ_FILE_ONLY, Rule, add_execute_only_ancestors, drop_privileges, install_landlock,
        retained_capability_mask, rules_for, rules_for_test, test_support,
    };
    use anyhow::Context as _;
    use jackin_protocol::CapsuleConfig;
    use std::collections::BTreeMap;
    use std::fs;
    use std::mem::size_of;
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::Path;
    use test_support::{
        ACCESS_RESOLVE_UNIX, FULL_WITH_UNIX, NULL_DEVICE, READ_ONLY, READ_ONLY_WITH_UNIX,
        access_for_abi,
    };

    #[test]
    fn landlock_rules_are_exact_for_selected_slot_and_exclude_secret_roots() {
        assert_eq!(size_of::<super::linux::RulesetAttr>(), size_of::<u64>());
        assert_eq!(size_of::<super::linux::PathBeneathAttr>(), 12);
        let config = CapsuleConfig {
            instances: vec!["slot-a".to_owned()],
            agents: BTreeMap::from([("slot-a".to_owned(), "claude".to_owned())]),
            instance_home_dirs: BTreeMap::from([(
                "slot-a".to_owned(),
                "/home/agent/.claude-a".to_owned(),
            )]),
            instance_mount_paths: BTreeMap::from([(
                "slot-a".to_owned(),
                vec![
                    "/home/agent/.claude-a".to_owned(),
                    "/jackin/claude-a/credentials.json".to_owned(),
                ],
            )]),
            ..CapsuleConfig::default()
        };
        let rules = rules_for(
            &config,
            Some("slot-a"),
            Path::new("/workspace/project"),
            Path::new("/jackin/run/sessions/1"),
        )
        .expect("construct exact Landlock rules");
        let paths = rules
            .iter()
            .map(|rule| rule.path.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(paths.iter().any(|path| path == "/home/agent/.claude-a"));
        assert!(
            paths
                .iter()
                .any(|path| path == "/jackin/claude-a/credentials.json")
        );
        assert_eq!(
            rules
                .iter()
                .find(|rule| rule.path == Path::new("/jackin/claude-a/credentials.json"))
                .expect("selected auth file rule")
                .access,
            READ_FILE_ONLY
        );
        assert!(!rules.iter().any(|rule| {
            rule.path == Path::new(jackin_protocol::ACCOUNT_CREDENTIALS_DIR)
                || rule
                    .path
                    .starts_with(Path::new(jackin_protocol::ACCOUNT_CREDENTIALS_DIR))
                || (rule.path == Path::new("/jackin/runtime")
                    && rule.access != super::linux::TRAVERSE)
                || (rule.path == Path::new("/jackin/default-home")
                    && rule.access != super::linux::TRAVERSE)
                || (rule.path == Path::new("/home/agent") && rule.access != super::linux::TRAVERSE)
                || (rule.path == Path::new("/jackin/run") && rule.access != super::linux::TRAVERSE)
                || (rule.path == Path::new("/proc") && rule.access != super::linux::TRAVERSE)
        }));
        assert!(!paths.iter().any(|path| path == "/home/agent/.claude-b"));
        assert!(
            rules
                .iter()
                .any(|rule| rule.path == Path::new("/home/agent")
                    && rule.access == super::linux::TRAVERSE)
        );
        assert!(
            rules
                .iter()
                .any(|rule| rule.path == Path::new("/jackin/runtime")
                    && rule.access == super::linux::TRAVERSE)
        );
    }

    #[test]
    fn cwd_boundary_rejects_root_ancestors_and_noncanonical_aliases() {
        let config = CapsuleConfig::default();
        for cwd in ["/", "/home", "/jackin", "/workspace/../"] {
            let error = rules_for_test(
                &config,
                None,
                Path::new(cwd),
                Path::new("/jackin/run/sessions/1"),
            )
            .expect_err("protected cwd must be rejected before rule construction");
            assert!(
                error.to_string().contains("protected"),
                "unexpected cwd rejection for {cwd}: {error:#}"
            );
        }
    }

    #[test]
    fn cwd_boundary_rejects_existing_symlink_alias_to_protected_root() {
        let temp = tempfile::tempdir().expect("symlink fixture");
        let alias = temp.path().join("home-alias");
        std::os::unix::fs::symlink("/home", &alias).expect("protected-root symlink");

        let error = rules_for_test(
            &CapsuleConfig::default(),
            None,
            &alias,
            Path::new("/jackin/run/sessions/1"),
        )
        .expect_err("symlink alias to protected root must be rejected");
        assert!(error.to_string().contains("protected"));
    }

    #[test]
    fn mount_boundaries_reject_existing_symlink_alias_to_protected_root() {
        let temp = tempfile::tempdir().expect("symlink fixture");
        let alias = temp.path().join("home-alias");
        std::os::unix::fs::symlink("/home", &alias).expect("protected-root symlink");
        let alias = alias.to_string_lossy().into_owned();

        let config = CapsuleConfig {
            workspace_mounts: vec![alias.clone()],
            ..CapsuleConfig::default()
        };
        let error = rules_for_test(
            &config,
            None,
            Path::new("/workspace/project"),
            Path::new("/jackin/run/sessions/1"),
        )
        .expect_err("symlink alias dst to protected root must be rejected");
        assert!(error.to_string().contains("protected"));

        let config = CapsuleConfig {
            worktree_git_targets: vec![alias],
            ..CapsuleConfig::default()
        };
        let error = rules_for_test(
            &config,
            None,
            Path::new("/workspace/project"),
            Path::new("/jackin/run/sessions/1"),
        )
        .expect_err("symlink alias git target must be rejected");
        assert!(error.to_string().contains("outside"));
    }

    #[test]
    fn cwd_boundary_rejects_ancestor_of_any_private_mount_destination() {
        let config = CapsuleConfig {
            instance_mount_paths: BTreeMap::from([(
                "canary".to_owned(),
                vec!["/workspace/private-slot".to_owned()],
            )]),
            ..CapsuleConfig::default()
        };
        let error = rules_for_test(
            &config,
            None,
            Path::new("/workspace"),
            Path::new("/jackin/run/sessions/1"),
        )
        .expect_err("cwd ancestor of private mount must be rejected");
        assert!(error.to_string().contains("mount destination"));
    }

    #[test]
    fn workspace_mounts_and_git_targets_gain_full_access_outside_cwd() {
        let config = CapsuleConfig {
            workspace_mounts: vec!["/workspace/other".to_owned()],
            worktree_git_targets: vec!["/jackin/host/workspace/other/.git".to_owned()],
            ..CapsuleConfig::default()
        };
        let rules = rules_for_test(
            &config,
            None,
            Path::new("/workspace/project"),
            Path::new("/jackin/run/sessions/1"),
        )
        .expect("dst outside cwd and aux git target must be granted");
        for path in ["/workspace/other", "/jackin/host/workspace/other/.git"] {
            let rule = rules
                .iter()
                .find(|rule| rule.path == Path::new(path))
                .unwrap_or_else(|| panic!("missing Landlock rule for {path}"));
            assert_eq!(rule.access, FULL_WITH_UNIX, "wrong access for {path}");
            assert!(rule.required, "rule for {path} must be required");
        }
    }

    #[test]
    fn hostile_worktree_git_targets_are_rejected() {
        for target in [
            "/jackin/run/x",
            "/home/agent/x",
            "/jackin/host",
            "/workspace/x",
            "/jackin/host/../run/x",
            "/jackin/host/a/../b",
            "relative/path",
            "",
        ] {
            let config = CapsuleConfig {
                worktree_git_targets: vec![target.to_owned()],
                ..CapsuleConfig::default()
            };
            let error = rules_for_test(
                &config,
                None,
                Path::new("/workspace/project"),
                Path::new("/jackin/run/sessions/1"),
            )
            .unwrap_err();
            let message = error.to_string();
            assert!(
                message.contains("outside")
                    || message.contains("must not contain")
                    || message.contains("absolute"),
                "unexpected rejection for {target:?}: {message}"
            );
        }
    }

    #[test]
    fn hostile_workspace_mounts_are_rejected() {
        for dst in [
            "/",
            "/home",
            "/home/agent",
            "/home/agent/.claude",
            "/jackin",
            "/jackin/run",
            "/workspace/../jackin",
            "relative/path",
            "",
        ] {
            let config = CapsuleConfig {
                workspace_mounts: vec![dst.to_owned()],
                ..CapsuleConfig::default()
            };
            let error = rules_for_test(
                &config,
                None,
                Path::new("/workspace/project"),
                Path::new("/jackin/run/sessions/1"),
            )
            .unwrap_err();
            let message = error.to_string();
            assert!(
                message.contains("protected") || message.contains("absolute"),
                "unexpected rejection for {dst:?}: {message}"
            );
        }
        let config = CapsuleConfig {
            workspace_mounts: vec!["/workspace".to_owned()],
            instance_mount_paths: BTreeMap::from([(
                "canary".to_owned(),
                vec!["/workspace/private-slot".to_owned()],
            )]),
            ..CapsuleConfig::default()
        };
        let error = rules_for_test(
            &config,
            None,
            Path::new("/workspace/project"),
            Path::new("/jackin/run/sessions/1"),
        )
        .expect_err("dst ancestor of private mount must be rejected");
        assert!(error.to_string().contains("mount destination"));
    }

    #[test]
    fn valid_workspace_cwd_retains_full_access_for_workspace_only() {
        let temp = tempfile::tempdir().expect("workspace fixture");
        let workspace = temp.path().join("workspace");
        let session_root = temp.path().join("session");
        fs::create_dir(&workspace).expect("workspace");
        fs::create_dir(&session_root).expect("session root");

        let rules = rules_for_test(&CapsuleConfig::default(), None, &workspace, &session_root)
            .expect("ordinary workspace cwd must remain valid");
        let workspace = workspace.canonicalize().expect("canonical workspace");
        assert!(rules.iter().any(|rule| {
            rule.path == workspace && rule.access == FULL_WITH_UNIX && rule.required
        }));
        assert!(!rules.iter().any(|rule| {
            rule.path == Path::new("/") && rule.access & test_support::WRITABLE != 0
        }));
    }

    #[test]
    fn socket_resolution_is_only_granted_to_non_sensitive_roots_on_abi9() {
        assert_eq!(access_for_abi(FULL_WITH_UNIX, 3), FULL);
        assert_eq!(access_for_abi(FULL_WITH_UNIX, 9), FULL_WITH_UNIX);
        assert_eq!(access_for_abi(READ_ONLY_WITH_UNIX, 3), READ_ONLY);
        assert_eq!(access_for_abi(READ_ONLY_WITH_UNIX, 9), READ_ONLY_WITH_UNIX);
        assert_eq!(
            access_for_abi(super::linux::TRAVERSE, 9),
            super::linux::TRAVERSE
        );

        let config = CapsuleConfig {
            instances: vec!["slot-a".to_owned()],
            agents: BTreeMap::from([("slot-a".to_owned(), "claude".to_owned())]),
            instance_home_dirs: BTreeMap::from([(
                "slot-a".to_owned(),
                "/home/agent/.claude-a".to_owned(),
            )]),
            instance_mount_paths: BTreeMap::from([(
                "slot-a".to_owned(),
                vec!["/home/agent/.claude-a".to_owned()],
            )]),
            ..CapsuleConfig::default()
        };
        let rules = rules_for(
            &config,
            Some("slot-a"),
            Path::new("/workspace/project"),
            Path::new("/jackin/run/sessions/1"),
        )
        .expect("construct Landlock rules");
        for socket in [
            jackin_core::container_paths::CAPSULE_SOCKET,
            jackin_core::container_paths::HOST_SOCK,
            jackin_core::container_paths::USAGE_SOCK,
        ] {
            assert!(
                rules.iter().all(|rule| {
                    rule.path != Path::new(socket) || rule.access & ACCESS_RESOLVE_UNIX != 0
                }),
                "RPC socket is missing ResolveUnix: {socket}"
            );
        }
        assert!(
            rules.iter().any(|rule| {
                rule.path == Path::new("/workspace/project")
                    && rule.access & ACCESS_RESOLVE_UNIX != 0
            }),
            "workspace must retain local Unix-socket behavior on ABI9"
        );
    }

    #[test]
    fn shared_state_and_tmp_are_not_agent_grants_and_private_root_is_exact() {
        let config = CapsuleConfig {
            instances: vec!["slot-a".to_owned()],
            agents: BTreeMap::from([("slot-a".to_owned(), "claude".to_owned())]),
            instance_home_dirs: BTreeMap::from([(
                "slot-a".to_owned(),
                "/home/agent/.claude-a".to_owned(),
            )]),
            instance_mount_paths: BTreeMap::from([(
                "slot-a".to_owned(),
                vec!["/home/agent/.claude-a".to_owned()],
            )]),
            ..CapsuleConfig::default()
        };
        let session_root = Path::new("/jackin/run/sessions/7");
        let rules = rules_for(
            &config,
            Some("slot-a"),
            Path::new("/workspace/project"),
            session_root,
        )
        .expect("construct Landlock rules");
        assert!(
            rules
                .iter()
                .any(|rule| { rule.path == session_root && rule.access == FULL_WITH_UNIX })
        );
        assert!(!rules.iter().any(|rule| {
            rule.path == Path::new(jackin_core::container_paths::STATE_DIR)
                && rule.access & test_support::WRITABLE != 0
        }));
        assert!(!rules.iter().any(|rule| {
            rule.path == Path::new("/tmp") && rule.access & test_support::WRITABLE != 0
        }));
        assert!(!rules.iter().any(|rule| {
            rule.path == Path::new("/home/agent/.config/gh")
                && rule.access & test_support::WRITABLE != 0
        }));
        assert!(
            rules.iter().any(|rule| {
                rule.path == Path::new(jackin_core::container_paths::RUN_DIR)
                    && rule.access == super::linux::TRAVERSE
            }),
            "run directory must remain traverse-only in the filesystem policy"
        );
        assert_eq!(retained_capability_mask(), 1u32 << 1);
        assert_eq!(retained_capability_mask() & (1u32 << 3), 0);
    }

    #[test]
    fn derived_pane_homes_and_agent_seed_fragment_are_granted() {
        // Secondary same-agent slot: suffixed home, agent default fragment.
        // The seed reads `/jackin/default-home/.codex` for every home shape,
        // so the grant must be keyed by runtime — never by home suffix.
        let config = CapsuleConfig {
            instances: vec!["cx-b-inst".to_owned()],
            agents: BTreeMap::from([("cx-b-inst".to_owned(), "codex".to_owned())]),
            instance_home_dirs: BTreeMap::from([(
                "cx-b-inst".to_owned(),
                "/home/agent/.codex-cx-b-inst".to_owned(),
            )]),
            instance_mount_paths: BTreeMap::from([(
                "cx-b-inst".to_owned(),
                vec!["/home/agent/.codex-cx-b-inst".to_owned()],
            )]),
            ..CapsuleConfig::default()
        };
        let rules = rules_for(
            &config,
            Some("cx-b-inst"),
            Path::new("/workspace/project"),
            Path::new("/jackin/run/sessions/4"),
        )
        .expect("construct Landlock rules for secondary slot");
        let panes = rules
            .iter()
            .find(|rule| rule.path == Path::new("/home/agent/.codex-cx-b-inst/panes"))
            .expect("derived pane-homes parent rule");
        assert_eq!(panes.access, FULL_WITH_UNIX);
        assert!(panes.required);
        let fragment = rules
            .iter()
            .find(|rule| rule.path == Path::new("/jackin/default-home/.codex"))
            .expect("agent seed-fragment rule");
        assert_eq!(fragment.access, READ_ONLY);
        assert!(!fragment.required);
        assert!(!rules.iter().any(|rule| {
            rule.path == Path::new("/jackin/default-home/.codex-cx-b-inst")
                && rule.access != super::linux::TRAVERSE
        }));
    }

    #[test]
    fn xdg_parent_home_gets_pane_homes_and_config_fragments() {
        // XDG-parent home: the base mount grants cover only subpaths, so the
        // derived `{home}/panes/{seq}` tree needs its own parent grant.
        let config = CapsuleConfig {
            instances: vec!["oc-c-inst".to_owned()],
            agents: BTreeMap::from([("oc-c-inst".to_owned(), "opencode".to_owned())]),
            instance_home_dirs: BTreeMap::from([(
                "oc-c-inst".to_owned(),
                "/home/agent/.local/share".to_owned(),
            )]),
            instance_mount_paths: BTreeMap::from([(
                "oc-c-inst".to_owned(),
                vec![
                    "/home/agent/.local/share/opencode".to_owned(),
                    "/home/agent/.cache/opencode".to_owned(),
                    "/home/agent/.config/opencode".to_owned(),
                ],
            )]),
            ..CapsuleConfig::default()
        };
        let rules = rules_for(
            &config,
            Some("oc-c-inst"),
            Path::new("/workspace/project"),
            Path::new("/jackin/run/sessions/5"),
        )
        .expect("construct Landlock rules for XDG-parent home");
        let panes = rules
            .iter()
            .find(|rule| rule.path == Path::new("/home/agent/.local/share/panes"))
            .expect("derived pane-homes parent rule");
        assert_eq!(panes.access, FULL_WITH_UNIX);
        assert!(panes.required);
        for fragment in [
            "/jackin/default-home/.local/share/opencode",
            "/jackin/default-home/.config/opencode",
        ] {
            let rule = rules
                .iter()
                .find(|rule| rule.path == Path::new(fragment))
                .unwrap_or_else(|| panic!("seed-fragment rule for {fragment}"));
            assert_eq!(rule.access, READ_ONLY);
            assert!(!rule.required);
        }
        // The parent home itself stays ungranted: only the panes subtree and
        // the instance's own mount subpaths are writable.
        assert!(!rules.iter().any(|rule| {
            rule.path == Path::new("/home/agent/.local/share")
                && rule.access & test_support::WRITABLE != 0
        }));
    }

    #[test]
    fn missing_instance_home_fails_rules_closed() {
        let config = CapsuleConfig {
            instances: vec!["slot-a".to_owned()],
            instance_mount_paths: BTreeMap::from([(
                "slot-a".to_owned(),
                vec!["/home/agent/.claude-a".to_owned()],
            )]),
            ..CapsuleConfig::default()
        };
        let error = rules_for(
            &config,
            Some("slot-a"),
            Path::new("/workspace/project"),
            Path::new("/jackin/run/sessions/1"),
        )
        .expect_err("instance without a home entry must fail closed");
        assert!(error.to_string().contains("no home dir"), "{error:#}");
    }

    #[test]
    fn shell_sessions_carry_no_pane_homes_grant() {
        let rules = rules_for(
            &CapsuleConfig::default(),
            None,
            Path::new("/workspace/project"),
            Path::new("/jackin/run/sessions/1"),
        )
        .expect("construct shell Landlock rules");
        assert!(!rules.iter().any(|rule| {
            rule.path.to_string_lossy().ends_with(&format!(
                "/{}",
                jackin_core::container_paths::PANE_HOMES_DIR_NAME
            )) && rule.access & test_support::WRITABLE != 0
        }));
    }

    #[test]
    fn null_stdio_has_only_an_exact_null_device_write_grant() {
        let config = CapsuleConfig::default();
        let rules = rules_for(
            &config,
            None,
            Path::new("/workspace/project"),
            Path::new("/jackin/run/sessions/1"),
        )
        .expect("construct Landlock rules");

        assert_eq!(
            rules
                .iter()
                .find(|rule| rule.path == Path::new("/dev/null"))
                .expect("exact null-device rule")
                .access,
            NULL_DEVICE
        );
        assert!(
            rules
                .iter()
                .any(|rule| { rule.path == Path::new("/dev") && rule.access == READ_ONLY })
        );
        assert!(!rules.iter().any(|rule| {
            rule.path == Path::new("/dev") && rule.access & test_support::WRITABLE != 0
        }));
        assert!(!rules.iter().any(|rule| {
            rule.path.starts_with(Path::new("/dev/"))
                && rule.path != Path::new("/dev/null")
                && rule.access & test_support::WRITABLE != 0
        }));
    }

    #[test]
    fn dind_client_cert_mount_is_exactly_read_only() {
        let rules = rules_for(
            &CapsuleConfig::default(),
            None,
            Path::new("/workspace/project"),
            Path::new("/jackin/run/sessions/1"),
        )
        .expect("construct DinD isolation rules");

        let certs = rules
            .iter()
            .find(|rule| {
                rule.path == Path::new(jackin_core::container_paths::DIND_CERTS_CLIENT_DIR)
            })
            .expect("exact DinD client-cert mount rule");
        assert_eq!(certs.access, READ_ONLY);
        assert!(!certs.required);
        assert!(!rules.iter().any(|rule| {
            rule.path == Path::new(jackin_core::container_paths::DIND_CERTS_CLIENT_DIR)
                && rule.access & test_support::WRITABLE != 0
        }));
    }

    #[test]
    fn isolated_runtime_setup_can_spawn_git_config_with_null_stdio() {
        // SAFETY: the production wrapper starts as root. Non-root test hosts
        // cannot exercise its capability-preserving UID transition.
        if unsafe { libc::geteuid() } != 0 {
            return;
        }

        let temp = tempfile::tempdir().expect("temporary runtime setup fixture");
        let workspace = temp.path().join("workspace");
        let session_root = temp.path().join("session");
        fs::create_dir(&workspace).expect("workspace");
        fs::create_dir(&session_root).expect("session root");
        fs::set_permissions(&workspace, fs::Permissions::from_mode(0o777))
            .expect("workspace permissions");
        fs::set_permissions(&session_root, fs::Permissions::from_mode(0o777))
            .expect("session root permissions");

        let (read_fd, write_fd) = {
            let mut fds = [0; 2];
            // SAFETY: `fds` points to two writable integers for pipe output.
            assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
            (fds[0], fds[1])
        };
        // SAFETY: the child immediately enters the isolated probe. This test
        // follows the same fork boundary as the sibling capability probe so a
        // successful Landlock install cannot restrict the test harness.
        let child = unsafe { libc::fork() };
        assert!(child >= 0, "fork runtime setup probe");
        if child == 0 {
            // SAFETY: `read_fd` is the unused read end returned by pipe.
            unsafe { libc::close(read_fd) };
            let result = (|| -> anyhow::Result<()> {
                drop_privileges(jackin_protocol::SessionIdentity {
                    uid: 65_534,
                    gid: 65_534,
                })?;
                let rules =
                    rules_for_test(&CapsuleConfig::default(), None, &workspace, &session_root)?;
                install_landlock(&rules)?;

                let global_config = session_root.join("gitconfig");
                let request = jackin_process::ExecRequest::new(
                    "git",
                    [
                        "config",
                        "--global",
                        "--get-all",
                        "url.https://github.com/.insteadOf",
                    ],
                )
                .cwd(&workspace)
                .envs([("GIT_CONFIG_GLOBAL", global_config.as_os_str())]);
                let output = jackin_process::exec_sync(&request)
                    .context("spawn git config under session Landlock");
                let output = output?;
                anyhow::ensure!(
                    output.code == Some(1),
                    "git config probe exited unexpectedly: {:?}, stderr={}",
                    output.code,
                    String::from_utf8_lossy(&output.stderr)
                );
                Ok(())
            })();
            if let Err(error) = &result {
                eprintln!("isolated runtime setup probe failed: {error:#}");
            }
            let status = [u8::from(result.is_ok())];
            // SAFETY: `status` points to one initialized byte and `write_fd`
            // is the pipe's valid write end.
            unsafe { libc::write(write_fd, status.as_ptr().cast(), 1) };
            // SAFETY: `write_fd` is no longer used after reporting the result.
            unsafe { libc::close(write_fd) };
            // SAFETY: the child must terminate without running parent-side
            // Rust destructors after fork.
            unsafe { libc::_exit(i32::from(result.is_err())) };
        }
        // SAFETY: the parent owns no use for the pipe's write end.
        unsafe { libc::close(write_fd) };
        let mut status = [0u8; 1];
        // SAFETY: `status` points to one writable byte and `read_fd` is the
        // pipe's valid read end.
        let bytes_read = unsafe { libc::read(read_fd, status.as_mut_ptr().cast(), 1) };
        assert_eq!(bytes_read, 1);
        // SAFETY: `read_fd` is no longer used after receiving the result.
        unsafe { libc::close(read_fd) };
        let mut wait_status = 0;
        // SAFETY: `wait_status` is writable and `child` is the pid returned by
        // fork.
        let wait_result = unsafe { libc::waitpid(child, &raw mut wait_status, 0) };
        assert_eq!(wait_result, child);
        assert_eq!(status[0], 1, "isolated runtime setup probe failed");
        assert!(libc::WIFEXITED(wait_status));
        assert_eq!(libc::WEXITSTATUS(wait_status), 0);
    }

    #[test]
    fn sibling_home_read_and_write_are_denied_after_uid_drop() {
        // SAFETY: `geteuid` has no pointer arguments and reports this test
        // process's effective uid.
        if unsafe { libc::geteuid() } != 0 {
            // The production wrapper always starts as root. Non-root test
            // hosts cannot exercise the capability-preserving UID transition.
            return;
        }
        let temp = tempfile::tempdir().expect("temporary isolation fixture");
        let own = temp.path().join("home/slot-a");
        let sibling = temp.path().join("home/slot-b");
        let staged = temp.path().join("account-credentials");
        let workspace = temp.path().join("workspace");
        let selected_auth = temp.path().join("selected-auth.json");
        fs::create_dir_all(own.parent().expect("home parent")).expect("home root");
        fs::create_dir(&own).expect("own slot");
        fs::create_dir(&sibling).expect("sibling slot");
        fs::create_dir(&staged).expect("staged credential root");
        fs::create_dir(&workspace).expect("workspace");
        fs::set_permissions(&own, fs::Permissions::from_mode(0o700)).expect("own permissions");
        fs::set_permissions(&sibling, fs::Permissions::from_mode(0o700))
            .expect("sibling permissions");
        fs::set_permissions(&staged, fs::Permissions::from_mode(0o700))
            .expect("staged permissions");
        fs::set_permissions(&workspace, fs::Permissions::from_mode(0o700))
            .expect("workspace permissions");
        let own_secret = own.join("credential");
        let sibling_secret = sibling.join("credential");
        let sibling_staged = staged.join("acct-slot-b.json");
        fs::write(&own_secret, b"own").expect("own credential");
        fs::write(&sibling_secret, b"sibling").expect("sibling credential");
        fs::write(&sibling_staged, b"sibling-staged").expect("sibling staged credential");
        fs::write(&selected_auth, b"selected-auth").expect("selected auth mount");

        let (read_fd, write_fd) = {
            let mut fds = [0; 2];
            // SAFETY: `fds` points to two writable integers for pipe output.
            assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
            (fds[0], fds[1])
        };
        // SAFETY: `fork` is called before any Rust threads are created in the
        // test process and the child immediately enters the isolated probe.
        let child = unsafe { libc::fork() };
        assert!(child >= 0, "fork isolation probe");
        if child == 0 {
            // SAFETY: `read_fd` is the unused read end returned by pipe.
            unsafe { libc::close(read_fd) };
            let result = (|| -> anyhow::Result<()> {
                drop_privileges(jackin_protocol::SessionIdentity {
                    uid: 65_534,
                    gid: 65_534,
                })?;
                let mut rules = Vec::new();
                add_execute_only_ancestors(&mut rules, &own);
                rules.push(Rule {
                    path: own.clone(),
                    access: FULL,
                    required: true,
                });
                add_execute_only_ancestors(&mut rules, &workspace);
                rules.push(Rule {
                    path: workspace.clone(),
                    access: FULL,
                    required: true,
                });
                add_execute_only_ancestors(&mut rules, &selected_auth);
                rules.push(Rule {
                    path: selected_auth.clone(),
                    access: READ_FILE_ONLY,
                    required: true,
                });
                install_landlock(&rules)?;
                let own_fd = open(&own_secret, libc::O_RDONLY);
                anyhow::ensure!(own_fd >= 0, "selected slot read denied");
                // SAFETY: `own_fd` was returned by open and is no longer used.
                unsafe { libc::close(own_fd) };
                let own_write = open(&own.join("selected-write"), libc::O_WRONLY | libc::O_CREAT);
                anyhow::ensure!(own_write >= 0, "selected slot write denied");
                // SAFETY: `own_write` was returned by open and is no longer used.
                unsafe { libc::close(own_write) };
                let workspace_write = open(
                    &workspace.join("agent-write"),
                    libc::O_WRONLY | libc::O_CREAT,
                );
                anyhow::ensure!(workspace_write >= 0, "workspace write denied");
                // SAFETY: `workspace_write` was returned by open and is no longer used.
                unsafe { libc::close(workspace_write) };
                let selected_auth_fd = open(&selected_auth, libc::O_RDONLY);
                anyhow::ensure!(selected_auth_fd >= 0, "selected auth read denied");
                // SAFETY: `selected_auth_fd` was returned by open and is no
                // longer used.
                unsafe { libc::close(selected_auth_fd) };
                let selected_auth_write = open(&selected_auth, libc::O_WRONLY | libc::O_CREAT);
                anyhow::ensure!(
                    selected_auth_write < 0,
                    "read-only selected auth mount was writable"
                );
                let sibling_fd = open(&sibling_secret, libc::O_RDONLY);
                anyhow::ensure!(sibling_fd < 0, "sibling slot read was allowed");
                let sibling_write = open(&sibling.join("new-file"), libc::O_WRONLY | libc::O_CREAT);
                anyhow::ensure!(sibling_write < 0, "sibling slot write was allowed");
                let sibling_staged_fd = open(&sibling_staged, libc::O_RDONLY);
                anyhow::ensure!(
                    sibling_staged_fd < 0,
                    "sibling staged credential read was allowed"
                );
                let sibling_staged_write = open(&sibling_staged, libc::O_WRONLY | libc::O_CREAT);
                anyhow::ensure!(
                    sibling_staged_write < 0,
                    "sibling staged credential write was allowed"
                );
                Ok(())
            })();
            if let Err(error) = &result {
                eprintln!("Linux isolation probe failed: {error:#}");
            }
            let status = [u8::from(result.is_ok())];
            // SAFETY: `status` points to one initialized byte and write_fd is
            // the pipe's valid write end.
            unsafe { libc::write(write_fd, status.as_ptr().cast(), 1) };
            // SAFETY: write_fd is no longer used after reporting the result.
            unsafe { libc::close(write_fd) };
            // SAFETY: the child must terminate without running parent-side
            // Rust destructors after fork.
            unsafe { libc::_exit(i32::from(result.is_err())) };
        }
        // SAFETY: the parent owns no use for the pipe's write end.
        unsafe { libc::close(write_fd) };
        let mut status = [0u8; 1];
        // SAFETY: `status` points to one writable byte and `read_fd` is the
        // pipe's valid read end.
        let bytes_read = unsafe { libc::read(read_fd, status.as_mut_ptr().cast(), 1) };
        assert_eq!(bytes_read, 1);
        // SAFETY: read_fd is no longer used after receiving the result.
        unsafe { libc::close(read_fd) };
        let mut wait_status = 0;
        // SAFETY: `wait_status` is writable and `child` is the pid returned by
        // fork.
        let wait_result = unsafe { libc::waitpid(child, &raw mut wait_status, 0) };
        assert_eq!(wait_result, child);
        assert_eq!(status[0], 1, "isolation probe failed inside child");
        assert!(libc::WIFEXITED(wait_status));
        assert_eq!(libc::WEXITSTATUS(wait_status), 0);
    }

    fn open(path: &Path, flags: libc::c_int) -> libc::c_int {
        let Ok(path) = std::ffi::CString::new(path.to_string_lossy().as_bytes()) else {
            return -1;
        };
        // SAFETY: `path` is NUL-terminated and remains alive through open;
        // the test requests a bounded mode for a possible new file.
        unsafe { libc::open(path.as_ptr(), flags, 0o600) }
    }
}

#[cfg(all(test, not(target_os = "linux")))]
mod non_linux_tests {
    #[test]
    fn isolated_sessions_fail_closed_before_any_launch() {
        let args = vec![
            "-".to_owned(),
            "2000".to_owned(),
            "2000".to_owned(),
            "/bin/true".to_owned(),
        ];
        let error = super::run_isolated_command(&args).expect_err("non-Linux launch must fail");
        assert!(error.to_string().contains("Linux Landlock boundary"));
    }
}
