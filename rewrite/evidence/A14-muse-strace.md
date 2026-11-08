# A14 Muse 1.4.14 strace outside the sandbox (T04)

Verdict: ROOT CAUSE FOUND. There is no failing path. The OpenShell Landlock sandbox denies `openat("/", O_RDONLY|O_DIRECTORY)` — muse's Agent Definition filesystem source opens the filesystem root as its capability anchor, gets EACCES, and maps it to `Agent Definition filesystem source failed: IoError`.

## Diagnostic environment

- macOS host, arm64. Diagnostic Linux host: OrbStack machine `muse-diag`, Debian 13.7 trixie (matches the role image `BASE_IMAGE=debian:trixie`), kernel 7.2.12-orbstack aarch64, strace 6.13, git 2.47.3. No jackin agent ran there; the machine hosted only the muse binary and probes (task T04 diagnosis).
- Binary located from `MUSE_CODE_SPEC` (crates/jackin-core/src/agent.rs:592-620): version `1.4.14-R7309.1`, url `https://lookaside.facebook.com/lookaside/muse/download/?channel=muse&version=1.4.14-R7309.1&file=muse-aarch64-linux`. Downloaded on the host; sha256 `3dc39b8fa26f9963cc2038f2966c0932ed5a39f0f9efb39cc0266a261cbbcf76` equals the spec. Static ELF aarch64, stripped, 371,529,960 bytes, no PT_INTERP.
- Sandbox replica in the VM (exact A6 round 3/4 layout): uid/gid 1000 user `sandbox`, HOME=/sandbox, `/sandbox/.config/muse/` (0700) holding `auth.json` (31 B placeholder), `settings.json` (63 B), `trust.json` (75 B), all 0600 owner 1000; `/sandbox/project` a real git repo; env `HOME=/sandbox USER=1000 OPENSHELL_SANDBOX=1 META_API_KEY=placeholder`, no MUSE_*/XDG_*; cwd `/sandbox/project`; binary at `/usr/local/bin/muse`.

## Step 1 — the brief's exact strace on unconfined Linux: no failing path exists

`strace -f -e trace=openat,open,stat,newfstatat muse exec "hi"` in the replica: 3213 syscall lines, zero EACCES. Every ENOENT is a benign absent-probe: `/etc/muse`, `/etc/xdg`, `/etc/pki/tls/certs`, `/etc/security/certificates`, `/var/facebook/credentials/1000/x509/1000.pem`, session-log rotation (`.log.1`..`.4`), model-catalog cache. muse proceeds past agent definitions to the model-catalog fetch and fails only on the placeholder credential (`failed to fetch model catalog: authentication failed: your API key from META_API_KEY was rejected`) — the expected end state without a real key.

Conclusion: the A6/blocker premise "record the failing openat path (ENOENT/EACCES)" is refuted. On Linux with the sandbox layout the binary is healthy; the failure is confinement-specific, not layout-specific.

## Step 2 — full syscall trace isolates the definition phase

Full `strace -f` (10,714 lines). The Agent Definition filesystem source does a capability-style walk (the `O_PATH`+`RESOLVE_BENEATH` component walk with `/proc/self/fd/N` readlink verification is the cap-std pattern):

```
openat(AT_FDCWD, "/sandbox/project", O_RDONLY|O_CLOEXEC|O_DIRECTORY) = 13
openat(AT_FDCWD, "/",          O_RDONLY|O_CLOEXEC|O_DIRECTORY) = 13
statx(13, "sandbox", AT_SYMLINK_NOFOLLOW, STATX_ALL) = 0
openat2(13, "sandbox", {O_RDONLY|O_NOFOLLOW|O_CLOEXEC|O_PATH|O_DIRECTORY, RESOLVE_NO_MAGICLINKS|RESOLVE_BENEATH}) = 14
statx(14, "project", ...) = 0
openat2(14, "project", {same flags}) = 15
readlinkat(AT_FDCWD, "/proc/self/fd/16", ...) = "/sandbox/project"
openat(AT_FDCWD, "/sandbox/.config", O_DIRECTORY) = 16
openat2(16, "muse", {O_PATH|RESOLVE_BENEATH}) = 17
openat(17, ".", O_RDONLY|O_NOFOLLOW|O_DIRECTORY) = 18 ; getdents64(...) = 152
openat(AT_FDCWD, "/", O_DIRECTORY) = 16 ; openat2(16, "etc", {O_PATH|RESOLVE_BENEATH}) = 18
statx(18, "muse") = -1 ENOENT ; statx(18, ".agents") = -1 ENOENT
statx(/sandbox/.local/share/muse fd, "plugins") = -1 ENOENT
```

Not io_uring: absent entirely. Seccomp control: a filter returning EPERM only for `openat2` does NOT break muse (it falls back to openat) — tested with a static `noopenat2` launcher.

## Step 3 — the sandbox's confinement facts (OpenShell v0.1.7, commit 5d25211)

- `crates/openshell-sandbox/src/linux/child_seccomp.rs`: denies (EPERM) ptrace, process_vm_*, pidfd_*, unshare/setns, mount family, bpf, perf_event_open, userfaultfd, io_uring_*, setuid family, etc.; `clone3`/`pidfd_open` → ENOSYS; fcntl F_SETOWN/F_SETSIG/F_SETOWN_EX and ioctl FIOSETOWN/SIOCSPGRP denied. **`openat2` is not denied.** muse's usage (F_SETFD, F_DUPFD_CLOEXEC, F_GETFL, F_SETFL; TIOCGWINSZ) hits none of the denied commands.
- `crates/openshell-sandbox/src/sandbox/linux/landlock.rs`: ruleset handles all AccessFs rights at ABI V3; every listed `read_only` path gets `AccessFs::from_read` (EXECUTE|READ_FILE|READ_DIR), every `read_write` path gets all rights. Landlock is default-deny: a path not beneath a granted path has all handled rights denied, and an `open(..., O_DIRECTORY)` needs READ_DIR on the directory itself.
- The sentinel role policy — the policy of every A6 launch, unchanged since the role's single commit ea6a629 — grants read_only `/bin /sbin /usr /lib /etc /proc /dev/urandom /var/log`, read_write `/tmp /dev/null /sandbox`, landlock `hard_requirement`. **No rule covers `/`.** The FilesystemPolicy default has no implicit root grant either.

## Step 4 — causal proof in the diagnostic VM

Static launcher `llpolicy` installs exactly that ruleset (same paths, same read/write right split, all-rights handled, ABI V3) then execs the target.

Probe `t04probe` (replays muse's exact calls) under it, as uid 1000:

```
openat(/sandbox/project, O_RDONLY|O_CLOEXEC|O_DIRECTORY)   = 3
openat(/, O_RDONLY|O_CLOEXEC|O_DIRECTORY)                  = -1 Permission denied
readlinkat(/proc/self/fd/1)                                = "pipe:[...]"
openat(/sandbox/.config, O_DIRECTORY)                      = 4
  openat2(cfgfd, "muse", O_PATH|RESOLVE_BENEATH)          = 5
    openat(musefd, ".", O_RDONLY|O_NOFOLLOW|O_DIRECTORY)  = 6 ; getdents64 = 152
openat(/usr, O_DIRECTORY)  = 4 ; /etc = 5 ; /proc = 6
openat(/var, O_DIRECTORY)  = -1 Permission denied ; /opt = -1 Permission denied
```

`llpolicy muse exec "hi"` → **`Agent Definition filesystem source failed: IoError`**, exit 1 — byte-identical to all four A6 rounds.

Root cause: the filesystem source opens `/` itself to anchor its walk; Landlock denies READ_DIR on `/` (nothing grants it); muse surfaces the raw io::Error (EACCES) as `IoError`. This explains every A6 observation: opens under /sandbox//etc//usr succeed (granted — matching the inotify record of successful opens), no failed open appears in inotify (failed opens emit no event, and `/` was not a watched dir), no path creation inside the sandbox can fix it, `--provider echo`/clean HOME/non-git cwd change nothing, and the macOS host (no Landlock) never reproduces.

## Step 5 — fix verified in the same setup

Same ruleset with `/` added to read_only: `openat("/") = 4` and `muse exec "hi"` passes the Agent Definition phase, now failing only at the model catalog on the placeholder key (the expected state in a diagnostic env without a real credential).

Implication for jackin: role sandbox policies need `/` in `read_only` for T04 (sentinel `policy.yaml` read_only gains `- /`). Landlock cannot express "list `/` without granting read beneath it" — a rule on `/` applies to the whole tree — so the grant is read-only over the entire tree; DAC still protects `/root` and `/etc/ssl/private` (0700 root-owned) from uid 1000, and read-write of `/` remains rejected by jackin policy validation (`check_read_write` → `ReadWriteRoot`). Alternatively the root READ_DIR grant could become an OpenShell default, but that is upstream's call; the role-policy fix is available to jackin now.

## Step 6 — cross-confirmation inside real OpenShell sandboxes

Both created via `openshell sandbox create -g jackin-local --from pr66-strace-arm-fixture:latest --policy <policy copy>` (gateway jackin-local; the bookworm+strace fixture image served only as a probe carrier; no jackin agent, no role launch).

`t04-diag` (unmodified sentinel policy; workload resolved to uid:gid 1000:1000, ConfigurationAccepted):

- `/tmp/t04probe`: `openat("/") = -1 Permission denied` while /sandbox//.config/muse/ getdents, /usr, /etc, /proc succeed and /var, /opt are denied — identical to the step 4 VM result.
- `/tmp/muse exec "hi"` (binary uploaded into the sandbox, sha256 re-verified there; sandbox-replica layout created first): **`Agent Definition filesystem source failed: IoError`, exit 1** — the exact A6 failure reproduced inside a real OpenShell sandbox, before any provider work.
- `strace -f -e trace=openat,open,stat,newfstatat /tmp/muse exec hi` inside the sandbox: strace itself dies on `PTRACE_TRACEME: Operation not permitted` — re-confirming round 3's ptrace denial on this gateway.

`t04-fix` (same policy plus `- /` in read_only; gateway accepted it, ConfigurationReady True):

- `/tmp/t04probe`: `openat("/") = 4` and the full openat2 walk `/`→`sandbox`→`project` succeeds.
- `/tmp/muse exec "hi"`: the Agent Definition phase passes; muse now fails at the network stage — `failed to fetch model catalog: transport error: error sending request for url (https://api.meta.ai/muse-code/models)` — expected here because this bare sandbox has `network_policies: {}` and a placeholder credential; a real T04 launch attaches the api.meta.ai endpoint rule and the gateway-held credential.

This closes the loop: same binary, same layout, only the Landlock grant of `/` differs between the A6 failure and success.

## Files

- Trace logs and probe sources were produced inside the disposable VM `/tmp` (not committed): `/tmp/trace-full.log` (full strace), `/tmp/trace-paths.log` (exact-filter strace), `/tmp/t04probe.c`, `/tmp/llpolicy.c`, `/tmp/noopenat2.c`.

## Required jackin change (precise)

1. Add `/` to `read_only` in the role sandbox policy used for T04 launches (sentinel `policy.yaml`), keeping everything else as-is.
2. Keep the trust.json launch writer (already landed; necessary but not sufficient).
3. Re-run the A6 live gate; with `/` granted, `muse exec` reaches provider work, so the remaining variable is only the real credential path.

## Cleanup

- Sandboxes `t04-diag` and `t04-fix` deleted from gateway jackin-local (sandbox list empty); OrbStack machine `muse-diag` deleted; downloaded binary left at `/tmp/muse-aarch64-linux` on the host (shasum recorded above). No product code edited, no commit, no push.
