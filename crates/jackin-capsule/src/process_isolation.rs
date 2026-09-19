//! Per-session process and filesystem isolation.
//!
//! The capsule supervisor is trusted. Agent sessions are not: they receive a
//! unique numeric identity, retain only the two DAC capabilities needed to
//! work in host bind mounts, and are confined with Landlock before `exec`.
//! Landlock is required rather than best-effort because DAC override would
//! otherwise let one slot walk into another slot's bind mount.

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
) -> Option<jackin_protocol::SessionIdentity> {
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
    use std::mem::size_of;
    use std::os::unix::process::CommandExt as _;
    use std::path::{Path, PathBuf};

    const CAP_DAC_OVERRIDE: u32 = 1;
    const CAP_FOWNER: u32 = 3;
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

    pub(super) const TRAVERSE: u64 = ACCESS_EXECUTE;
    pub(super) const READ_FILE_ONLY: u64 = ACCESS_EXECUTE | ACCESS_READ_FILE;
    const READ_ONLY: u64 = READ_FILE_ONLY | ACCESS_READ_DIR;
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
        let cwd = std::env::current_dir().context("resolve isolated session cwd")?;
        let rules = rules_for(config, instance, &cwd)?;
        drop_privileges(identity)?;
        install_landlock(&rules)?;

        let error = std::process::Command::new(program).args(args).exec();
        Err(error).with_context(|| format!("exec isolated session program {program}"))
    }

    pub(super) fn rules_for(
        config: &CapsuleConfig,
        instance: Option<&str>,
        cwd: &Path,
    ) -> Result<Vec<Rule>> {
        anyhow::ensure!(cwd.is_absolute(), "isolated session cwd must be absolute");
        anyhow::ensure!(
            !cwd.starts_with(Path::new(jackin_core::container_paths::JACKIN_ROOT))
                && !cwd.starts_with(Path::new("/home/agent")),
            "isolated session cwd cannot be a capsule or agent-private path"
        );
        let mut rules = Vec::new();
        required_exact_rule(&mut rules, cwd, FULL);
        required_exact_rule(
            &mut rules,
            Path::new(jackin_core::container_paths::STATE_DIR),
            FULL,
        );
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
        // Do not grant a broad /proc read rule: selected credentials are
        // transported in the child environment, and /proc/<pid>/environ would
        // otherwise let a DAC-capable sibling read them. Programs may inspect
        // only their own proc tree when the image provides these magic links.
        for path in ["/proc/self", "/proc/thread-self"] {
            optional_exact_rule(&mut rules, Path::new(path), READ_ONLY);
        }
        optional_exact_rule(&mut rules, Path::new("/tmp"), FULL);

        // Image-baked tools and shell configuration are shared, but are not
        // account slots. Slot roots below are the only mutable account paths.
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
            optional_exact_rule(&mut rules, Path::new(path), FULL);
        }
        for path in [
            jackin_core::container_paths::CAPSULE_CONFIG,
            jackin_core::container_paths::HOST_SOCK,
            jackin_core::container_paths::USAGE_SOCK,
            jackin_core::container_paths::USAGE_ACCOUNTS,
        ] {
            optional_exact_rule(&mut rules, Path::new(path), READ_ONLY);
        }
        optional_exact_rule(
            &mut rules,
            Path::new(jackin_core::container_paths::CLIPBOARD_DIR),
            FULL,
        );
        optional_exact_rule(&mut rules, Path::new("/home/agent/.config/gh"), FULL);

        if let Some(instance) = instance {
            let paths = config.mount_paths_for_instance(instance);
            anyhow::ensure!(
                !paths.is_empty(),
                "admitted instance has no private home/auth mount paths"
            );
            for path in paths {
                let path_ref = Path::new(path);
                let access = if path_ref.is_dir() {
                    FULL
                } else {
                    // Forwarded auth files are Docker/Apple read-only mounts;
                    // keep the Landlock grant read-only too.
                    READ_FILE_ONLY
                };
                required_exact_rule(&mut rules, path_ref, access);
                if let Some(relative) = path.strip_prefix("/home/agent/") {
                    // Runtime setup may seed this selected slot from the
                    // image snapshot. Never allow the snapshot root itself:
                    // it contains every agent's default fragment.
                    optional_exact_rule(
                        &mut rules,
                        &Path::new(jackin_core::container_paths::DEFAULT_HOME_DIR).join(relative),
                        READ_ONLY,
                    );
                }
            }
        }
        Ok(rules)
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
            handled_access_fs: FULL,
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
                    rule.access
                } else {
                    rule.access
                        & (ACCESS_EXECUTE | ACCESS_WRITE_FILE | ACCESS_READ_FILE | ACCESS_TRUNCATE)
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

        let mask = (1u32 << CAP_DAC_OVERRIDE) | (1u32 << CAP_FOWNER);
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
        for capability in [CAP_DAC_OVERRIDE, CAP_FOWNER] {
            // SAFETY: this raises one of the two capabilities just installed
            // in the calling process's ambient set.
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

    fn close_fd(fd: libc::c_int) {
        // SAFETY: callers pass file descriptors returned by the kernel and no
        // longer use them after this close.
        unsafe {
            libc::close(fd);
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::linux::{
        FULL, READ_FILE_ONLY, Rule, add_execute_only_ancestors, drop_privileges, install_landlock,
        rules_for,
    };
    use jackin_protocol::CapsuleConfig;
    use std::collections::BTreeMap;
    use std::fs;
    use std::mem::size_of;
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::Path;

    #[test]
    fn landlock_rules_are_exact_for_selected_slot_and_exclude_secret_roots() {
        assert_eq!(size_of::<super::linux::RulesetAttr>(), size_of::<u64>());
        assert_eq!(size_of::<super::linux::PathBeneathAttr>(), 12);
        let config = CapsuleConfig {
            instances: vec!["slot-a".to_owned()],
            instance_mount_paths: BTreeMap::from([(
                "slot-a".to_owned(),
                vec![
                    "/home/agent/.claude-a".to_owned(),
                    "/jackin/claude-a/credentials.json".to_owned(),
                ],
            )]),
            ..CapsuleConfig::default()
        };
        let rules = rules_for(&config, Some("slot-a"), Path::new("/workspace/project"))
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
            unsafe { libc::_exit(if result.is_ok() { 0 } else { 1 }) };
        }
        // SAFETY: the parent owns no use for the pipe's write end.
        unsafe { libc::close(write_fd) };
        let mut status = [0u8; 1];
        // SAFETY: status points to one writable byte and read_fd is valid.
        assert_eq!(
            unsafe { libc::read(read_fd, status.as_mut_ptr().cast(), 1) },
            1
        );
        // SAFETY: read_fd is no longer used after receiving the result.
        unsafe { libc::close(read_fd) };
        let mut wait_status = 0;
        // SAFETY: wait_status is writable and child is the pid returned by fork.
        assert_eq!(unsafe { libc::waitpid(child, &mut wait_status, 0) }, child);
        assert_eq!(status[0], 1, "isolation probe failed inside child");
        assert!(libc::WIFEXITED(wait_status));
        assert_eq!(libc::WEXITSTATUS(wait_status), 0);
    }

    fn open(path: &Path, flags: libc::c_int) -> libc::c_int {
        let path = std::ffi::CString::new(path.to_string_lossy().as_bytes())
            .expect("test path contains no NUL");
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
