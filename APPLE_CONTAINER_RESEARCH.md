# Apple Container 1.0 and the macOS Agent-Sandbox Landscape — Deep Research for PR #527

**Date**: 2026-06-10 (mid-WWDC26, June 8–12). **Scope**: deep analysis of PR #527 (`feat(runtime): add jackin-exec and Apple Container backend foundation`) against the upstream state of apple/container as of its 1.0.0 release (June 9, 2026), the new `container machine` feature, Apple's experimental "Sandboxy" agent sandbox, and the competing runtimes (Docker Desktop / Docker Sandboxes, OrbStack, smolvm) — latest versions only, macOS-first lens. All upstream claims below were verified against primary sources (GitHub API, raw repo files, developer.apple.com) on 2026-06-10; a 3-vote adversarial verification pass confirmed 24/25 extracted claims. Items marked *(analysis)* are this document's reasoning, not verified upstream statements.

---

## 1. Executive summary

1. **The bet on Apple Container is confirmed — and Apple just validated the entire jackin❯ architecture.** apple/container shipped **1.0.0 on June 9, 2026** (first stable release, one-year anniversary, WWDC26 week). Three days earlier, Apple merged an experimental example into apple/containerization called **Sandboxy — "an example tool to run isolated coding agents"** — which runs Claude Code with `--dangerously-skip-permissions` inside a per-session lightweight VM behind a host-side network allowlist proxy with CPU/memory limits and explicit mounts. That is the jackin❯ four-layer model (VM boundary + egress allowlist + resource limits + explicit mounts), built by Apple as a demo. jackin❯ is building the product version of what Apple just sketched.
2. **PR #527 was written against v0.11.0 (March 2026) and leaves most of 1.0.0's security surface on the table.** Since 0.11.0, upstream added/stabilized: `--cpus`/`--memory` per container, `--cap-add`/`--cap-drop` with a **reduced default capability set**, `readonly` mount option, `--read-only` rootfs, `--tmpfs`, `--ssh` agent forwarding, `-p` port publish **and `--publish-socket host:container`** (Unix socket publishing — tailor-made for the jackin-exec `host.sock`), named networks with `--internal` (host-only, no internet), `--dns*` options, `--rosetta`, `--stop-signal`, structured `ls`/`inspect` output (breaking), and a TOML system config replacing `container system property`. The PR uses none of these.
3. **`container machine` is not a new isolation tier — it is persistence + host integration on the same per-VM-kernel primitive.** Each machine is its own lightweight VM booted from an OCI image **through the image's `/sbin/init`** (systemd works), with automatic `$HOME` mapping (`rw`/`ro`/`none`) and per-machine `cpus`/`memory`. For jackin❯ it is strategically important for one reason above all: *(analysis)* an init-booted per-session VM can run a stock Docker daemon as a systemd unit — potentially dissolving the rootless-DinD Phase 0 gate by replacing "DinD inside a container" with "Docker inside a VM", which is exactly how OrbStack/Colima run Docker today, except per-session with its own kernel.
4. **OrbStack 2.2 (June 4, 2026) improved isolated machines but did not change the architecture.** Per-machine CPU/memory/disk limits, `--isolate-network`, opt-in `--forward-ssh-agent`, and "Fixed Docker not working in isolated machines" all landed — but machines still share one Linux kernel (7.0.11) inside one VM. The deferral rationale in the roadmap stands: OrbStack closes productivity gaps, not the kernel-boundary gap.
5. **smolvm hit 1.0 (June 5, 2026) and is a stronger fallback than the roadmap assumes.** Now at smol-machines/smolvm (Rust, Apache-2.0, ~3.7k stars, 54 releases since Dec 2025): VM fork (checkpoint/restore + copy-on-write clones), elastic memory via virtio-balloon (host reclaims unused guest memory — something Virtualization.framework only partially does), deny-by-default networking with `--allow-host` egress allowlists as first-class CLI flags, and `--ssh-agent` keeping keys host-side. It explicitly markets to coding agents. Apple Container remains the right primary (platform vendor, zero-cost maintenance trajectory, vminitd attach contract), but smolvm now ships, as product features, two things Apple only demoed in an example (egress allowlist) or lacks (memory reclaim).
6. **Two upstream facts require correcting the jackin❯ roadmap docs**: (a) apple/container is **not "built into macOS 26"** — it is a separate open-source tool installed via a signed `.pkg` (or Homebrew); "zero install friction" should be downgraded to "first-party, one installer, no daemon license"; (b) the sleep/wake DNS hiccup has **no upstream fix** through 1.0.0 (the related "network unreachable after restart" issue #1321 was closed *not planned*), so the PR's DNS health check is load-bearing indefinitely, not a temporary workaround.

---

## 2. The problem this PR solves (roadmap recap)

From `docs/content/docs/reference/roadmap/agent-isolation-architecture.mdx`: every jackin❯ session today runs inside one shared OrbStack Linux VM kernel. The 2025–2026 CVE record (runc escapes Nov 2025, CVE-2026-34040 Docker authz bypass, CVE-2024-1086 netfilter UAF exploited in the wild) plus the jackin❯ `--privileged` DinD sidecar make the kernel the single shared attack surface: one compromised session can reach every other session. The four-layer answer:

| Layer | Mechanism | Status |
|---|---|---|
| 1 | Docker hardening (rootless DinD, cap policy, read-only root) | In progress (separate item) |
| 2 | **Apple Container VM boundary — own kernel per session** | **This PR (Phases 1–4 code, Phase 0 gate pending)** |
| 3 | jackin-exec on-demand credential injection | This PR (Phase 1+2 code) |
| 4 | Network egress policy at VM boundary | Future |
| × | zerobox per-operation sandboxing | Future |

OrbStack isolated machines were researched first and deferred: they are cgroups+namespaces on OrbStack's single shared kernel — "isolated" means filesystem-isolated, not kernel-isolated. The PR's premise — per-container VMs via Virtualization.framework are the only macOS-native per-workload kernel boundary — was correct in March and is *more* correct after WWDC26 (§3).

---

## 3. What changed upstream — June 2026 timeline

| Date | Event | Why it matters to jackin❯ |
|---|---|---|
| 2026-06-01 | apple/containerization `0.33.3-prerelease` (latest published release of the framework; framework stays pre-1.0) | Framework requires macOS 26 + Xcode 26 to build; kernel baseline 6.14.9; vminitd = PID 1, gRPC over vsock |
| 2026-06-03 | containerization tag `0.33.4`: **PR #607 "[Experimental] Add Sandboxy binary — an example tool to run isolated coding agents"** (+5,132 lines, by Apple core maintainers dcantah/crosbymichael) | Apple's own agent sandbox: Claude Code in a microVM behind a host-side allowlist proxy. Blueprint + validation for jackin❯ Layers 2/4 (§6) |
| 2026-06-04 | **OrbStack 2.2.0 / 2.2.1**: per-machine CPU/memory/disk limits, `--isolate-network`, opt-in `--forward-ssh-agent`, selective mounts, "Fixed Docker not working in isolated machines", `orb push/pull` fix milestone | Closes the CLI-surface gaps the roadmap flagged — but still one shared kernel (Linux 7.0.11) for all machines |
| 2026-06-05 | **smolvm v1.0.0 / v1.0.1** (smol-machines/smolvm): fast VM fork (checkpoint/restore + CoW clones, PR #344), OCI image-index support (PR #351) | Fallback matured: 1.0, elastic balloon memory, `--allow-host` egress allowlist, `--ssh-agent`, "for coding agents" positioning |
| 2026-06-08 | **WWDC26 keynote: macOS 27 "Golden Gate"** (Apple-silicon-only, last full Rosetta 2 release, ships Sept 2026); new **DiskImageKit** framework (ASIF disk images for Virtualization.framework) — press-reported | Platform direction: Virtualization.framework keeps getting first-party investment; apple/container README still says "supported on macOS 26" (no 27 requirement) |
| 2026-06-08 | **WWDC26 Session 389 "Discover container machines"** (10:33) | First-party narrative for persistent Linux-on-Mac environments on per-VM Containerization |
| 2026-06-09 | **apple/container 1.0.0** — first stable release (16th overall; signed installer pkg, ~3.5k downloads in <2 days). Headline: `container machine` (PR #1662, merged ~7h before the release). Also: TOML config replaces `system property get/set` (PR #1425); structured `ls`/`inspect` output (breaking); `container cp`; `--stop-signal`; XPC v0 API compat removed (versioned API promised next) | The platform jackin❯ is integrating just declared API stability at the CLI level and shipped its biggest feature since launch |
| 2026-06-10 | `examples/container-machine-vscode` lands on apple/container main | Apple is actively building the "machine as dev environment" story |

Sources: github.com/apple/container/releases (API-verified), github.com/apple/container/pull/1662, github.com/apple/containerization (PR #607, tag 0.33.4), developer.apple.com/videos/play/wwdc2026/389/, docs.orbstack.dev/release-notes, github.com/smol-machines/smolvm/releases, docs.docker.com/desktop/release-notes.

---

## 4. apple/container 1.0.0 — capability inventory relevant to jackin❯

### 4.1 Release-by-release (0.8.0 → 1.0.0): what shipped since the PR's v0.11.0 baseline — plus what the PR's baseline already had

The PR and roadmap docs were written against v0.11.0 (2026-03-31). Relevant features by release (verbatim-condensed from release notes):

- **0.8.0 (2026-01-22)**: `--read-only` rootfs for create/run; IPv6, container DNS, and port forwarding; `container network prune`; volume filesystem performance improvements; image-load path-traversal fix.
- **0.9.0 (2026-02-03)**: **configurable resource limits**; `host.docker.internal`-style hostnames to reach host services; **host-only networks**; Kata 3.26.0 kernel; zstd layer unpack.
- **0.10.0 (2026-02-26)**: `--init-image` (select VM init image); container bundle creation moved to `SandboxService`; export running container to image; multiple network plugins (breaking API); minimum-memory validation; network-privacy alert on port publish.
- **0.11.0 (2026-03-31)** *(the PR's baseline)*: default CPUs/memory via system properties (#1260/#1261); `CONTAINER_DEFAULT_PLATFORM`; `container export` OCI layout tar; build secrets; network `mtu` option.
- **0.12.0 (2026-04-27)**: **`--cap-add` and `--cap-drop` for containers (#1260) — "The default set of capabilities has been reduced"** (breaking: old all-caps behavior requires container recreation); single-file mount reliability fix (#1251); `container volume create --journal`; kata-3.28.0 kernel; TOML plugin config; `SSH_AUTH_SOCK` re-pass after login change.
- **0.12.1 (2026-04-29)**: macOS 15 Sequoia compat fix (list/delete networks) — upstream still ships Sequoia fixes, but README support policy is macOS 26 only.
- **0.12.3 (2026-04-30)**: two security fixes — prevent HTTP downgrade in registry commands; prevent path/rule injection in `container system dns`.
- **1.0.0 (2026-06-09)**: `container machine` (§5); **TOML configuration file replaces UserDefaults-backed system properties (removes `container system property get/set`)**; **breaking: cleaned-up structured (JSON/YAML/TOML) output for `container`/`image`/`network`/`volume` `ls` + `inspect`**; `container cp`; `--stop-signal` for run; XPC-connection-as-lease fixes IP address leaks (#1378); `system df` accounting fixes; removed v0 XPC API compatibility ("A subsequent release will introduce a version on the API itself").

### 4.2 `container run` flag surface at 1.0.0 vs what PR #527 uses

Verified against `docs/command-reference.md` at the `1.0.0` tag. The PR's `AppleContainerSpec` carries only `image`, `env`, `mounts (rw)`, `caps_add (empty)` (`crates/jackin-runtime/src/apple_container_client.rs:40-50`), emitted as `run --name <n> -d -e… -v… --cap-add… <image> jackin-capsule` (`apple_container_client.rs:119-148`).

| 1.0.0 capability | Flag | PR #527 uses it? | jackin❯ relevance |
|---|---|---|---|
| CPU limit | `--cpus <n>` | ❌ | Declarative resource limits roadmap item; parity with Sandboxy (`--cpus`, default 4) and `machine set cpus=4` |
| Memory limit | `--memory <size>` (K/M/G/T/P suffix) | ❌ | Same; critical given partial ballooning (§4.3) — an unbounded VM holds peak memory until stopped |
| Read-only bind mounts | `--mount type=…,source=…,target=…,readonly` | ❌ (bare rw `-v` only) | **Host-enforced `ro` mounts** — the OrbStack roadmap item could only get guest-enforced `:ro`; here the hypervisor host side enforces it |
| Read-only rootfs | `--read-only` | ❌ | Hardened profile parity with the Docker hardening contract |
| tmpfs | `--tmpfs` | ❌ | Scratch space under read-only rootfs |
| Capability drop | `--cap-drop` (+ reduced default set since 0.12.0) | ❌ | Session contract should report the *actual* default cap set, and the hardened profile should `--cap-drop` further |
| Unix socket publish | **`--publish-socket host_path:container_path`** | ❌ (bind-mounts the whole socket dir) | Publish exactly `host.sock` into the guest instead of mounting `~/.jackin/sockets/<n>/` wholesale — smaller surface, first-class primitive |
| Port publish | `-p [ip:]host:container[/proto]` | ❌ | Operator reaches agent-started dev servers from macOS without knowing the VM IP |
| Named networks | `--network <name>[,mtu=…]` | ❌ (default network) | **Per-session networks**; `container network create --internal` = host-only, no internet — the deny-all egress baseline (§6.2) |
| DNS control | `--dns`, `--dns-domain`, `--dns-option`, `--dns-search` | ❌ | Point guest DNS at a jackin-controlled resolver for egress-policy enforcement/diagnostics |
| SSH agent | `--ssh` | ❌ | Host-side key custody for git push — complements jackin-exec without exposing key material |
| Rosetta | `--rosetta` (+ `--arch/--os/--platform`) | ❌ | amd64-only role images on Apple silicon |
| Stop signal | `--stop-signal` | ❌ | Graceful capsule shutdown contract on `container stop` |
| Init image | `--init-image` | ❌ | Pin the vminitd image version for reproducible boots |
| Kernel | `container system kernel set` | ❌ | Pin/upgrade guest kernel fleet-wide |

*(analysis)* None of these are blockers — the PR is an explicitly minimal Phase 1–4 scaffold gated on Phase 0. But the gap list is exactly the follow-up backlog, and three of them (`readonly` mounts, `--memory`/`--cpus`, `--publish-socket`) are cheap, high-value, and should land in the PR or its immediate successor (§8, §9).

### 4.3 Limitations still standing at 1.0.0

| Limitation (PR table said, at v0.11.0) | Status at 1.0.0 | Evidence |
|---|---|---|
| `--privileged` not supported | **Still true** (by design; `--cap-add` is the path; default cap set reduced in 0.12.0) | command reference, 0.12.0 notes |
| DNS hiccup after macOS sleep/wake | **No fix found in any release 0.8.0→1.0.0**; closest issue #1321 ("network unreachable after system restarts", vmnet interface invalid) **closed as not planned** 2026-03-20 | release notes sweep, issue #1321 |
| Multi-container bridge networking rough edges | Partially addressed: IPv6/DNS/port-forwarding (0.8.0), `host.docker.internal` + host-only networks (0.9.0), network plugins (0.10.0), mtu (0.11.0), IP-leak fix (1.0.0). Needs Phase 0 re-validation on 1.0.0 rather than assumed broken | release notes |
| No health checks | **Still true** — open PR #1504 only reserves a `HealthStatus` enum ("always nil at runtime") | PR #1504, task #440 |
| Apple Silicon only / macOS 26 required | Still true: README at 1.0.0 verbatim "supported on macOS 26 … We do not support older versions". macOS 15 degrades (no container-to-container networking; single default vmnet network; `container network` unavailable) — irrelevant for jackin❯ (macOS 26+ target) | README, technical-overview.md |
| *(new)* Memory ballooning is partial | "memory pages freed to the Linux operating system by processes running in the container's VM are not relinquished to the host"; Apple suggests periodic restarts for memory-intensive workloads | technical-overview.md verbatim |
| *(new)* No Docker Engine API | Issue #66 "Expose Docker Engine API" **closed as not planned** ("we don't have plans to implement the Docker API atop the container client today" — jglogan, Apple); community shim **socktainer** v0.12.0 (2026-04-28) offers partial Engine API v1.51 over `~/.socktainer/container.sock`, macOS 26 arm64, "local development and experimentation" | issue #66, socktainer README |
| *(new)* Shell-out remains the only stable integration | 1.0.0 removed v0 XPC API compat; versioned XPC API promised "in a subsequent release". CLI + structured output is the supported surface today — the PR's `tokio::process::Command` pattern is correct | 1.0.0 release notes |

---

## 5. `container machine` — what it actually is

### 5.1 Verified facts

From `docs/container-machine.md` (added in PR #1674, 2026-06-09, signed off by michael_crosby@apple.com / eric_ernst@apple.com) and WWDC26 Session 389:

- **One lightweight VM per machine** on the same Containerization framework — same isolation tier as `container run`; the differences are lifecycle and host integration, not boundary. Apple's framing: "fast and lightweight, like a container, and persistent like a virtual machine."
- **Boots the image's init system**: "Containers are typically modeled after an application. A container machine is modeled after a Linux environment. It runs the image's init system… Any Linux image that includes `/sbin/init` works"; `systemctl start postgresql` works on systemd images.
- **Persistent**: rootfs modifications survive across sessions; `machine rm` deletes "including its persistent storage".
- **Command family**: `container machine create <image> --name <n>` | `run -n <n> [cmd]` (no cmd → interactive shell as a user matching the host account; boots if stopped) | `set-default` | `ls` | `inspect` (JSON) | `stop` | `rm` | `set -n <n> cpus=4 memory=8G` (applies after next stop/start) | `logs`; alias `m`.
- **Host integration**: macOS `$HOME` auto-mounted at `/Users/<username>` (doc internally inconsistent — quickstart shows `/home/<you>`); home-mount modes **`rw` (default), `ro`, `none`**. First-boot user provisioning script overridable via `/etc/machine/create-user.sh` (env: `CONTAINER_UID/GID/USER/HOME/MACHINE_ID`).
- **Resources**: memory defaults to **half of host memory**; per-machine `cpus`/`memory` via `machine set`.
- **Not documented**: extra `-v` volumes for machines, machine networking/IP semantics, port publish, SSH flags. The doc contains **zero occurrences of "agent", "AI", "sandbox", or "isolation"**.
- Ecosystem signal: `examples/container-machine-vscode` on main (~21h post-tag); WWDC26 Session 389 dedicated to it.

### 5.2 Fit analysis for jackin❯ *(analysis)*

**What `container machine` is for, per Apple**: edit-on-Mac/build-in-Linux, macOS tooling against Linux artifacts, real init-supervised services, one environment per distro. It is OrbStack-machines-but-per-VM-kernel — Apple competing for the exact niche OrbStack's isolated machines occupy, with the kernel boundary OrbStack lacks.

**Why it matters for jackin❯ despite not being agent-branded:**

1. **It may dissolve the Phase 0 rootless-DinD gate.** The entire prerequisite chain (Docker hardening → rootless DinD → `CAP_SYS_ADMIN` inside apple/container) exists because `container run` runs a single application process under vminitd, so an inner Docker daemon needs container-style privilege juggling. A machine boots **systemd**; a stock `docker-ce` installed in the role image runs as a normal root systemd unit **inside the machine's own VM** — exactly how OrbStack runs Docker today, except the daemon is per-session with its own kernel. There is no `--privileged` question because there is no outer container: the VM is the privilege boundary, and root-inside-VM is contained by the hypervisor. The threat-model arithmetic also changes for `container run`: granting `CAP_SYS_ADMIN` inside an apple/container VM weakens only that VM's interior, not the host boundary — the original fear ("if apple/container allows CAP_SYS_ADMIN, the isolation story weakens", smolvm-backend-research.mdx) conflated the Docker-on-shared-kernel consequence with the per-VM consequence. **Phase 0 should test both paths: (a) rootless DinD under `container run --cap-add`, (b) dockerd-under-systemd in a `container machine`.** If (b) works, it is architecturally cleaner and matches role authors' mental model (a "real Linux box" per session).
2. **Persistence maps to eject/reconnect.** Today's Docker backend keeps stopped containers for reconnect; machines make that a first-party concept with `machine stop` / `machine run` resume and surviving rootfs. Warm reattach gets cheaper than re-launching a `container run` VM.
3. **The home-mount must be `none` for jackin❯.** Default `rw` `$HOME` mapping is the antithesis of explicit-mounts-only. Any machine-based backend must set home-mount `none` and add explicit mounts — **but machines currently document no extra-volume mechanism**, which is the single blocker for machine-as-role-environment today. Until machines grow `-v`, `container run` remains the only shape that satisfies the jackin❯ mount contract. Track upstream.
4. **PID-1 nuance to validate**: with the image's init as the in-VM process tree root, where does `JACKIN_CAPSULE_FORCE_DAEMON=1` fit? Likely answer: capsule becomes a systemd unit (cleaner than the env-var workaround), with `machine run -n <n> jackin-capsule` as attach. Phase 0 item.

**Bottom line**: `container machine` is not what the PR should switch to today (no extra mounts, no networking docs), but it is very likely the **V2 shape** of the apple-container backend — "one persistent machine per jackin❯ instance, systemd-supervised capsule + dockerd, home-mount none, explicit volumes" — pending two upstream gaps (volumes, network semantics). The roadmap item should name this trajectory.

---

## 6. Sandboxy — Apple's reference agent sandbox, dissected

PR #607 on apple/containerization (merged 2026-06-03, tag 0.33.4, `examples/sandboxy/`, +5,132 lines). README: "`sandboxy` runs AI coding agents in sandboxed Linux environments on macOS with Apple silicon." Experimental example, not a product — but written by the framework's core maintainers, and the clearest signal of where Apple is pointing this stack.

### 6.1 Mechanism (verified from source)

- **CLI**: `sandboxy run <agent> [args]` with `-w/--workspace` (default cwd), `--cpus` (default 4), `--memory` (default "4g"), `--allow-hosts <h…>`, `--no-network-filter`, `-m host:container[:ro|rw]` (repeatable), `-e KEY[=VALUE]` (host env forwarding), `--name` (persistent instance, resumable conversation), `--rm`, `--ssh-agent`, `-k/--kernel` (auto-download).
- **Network filtering = host-only vmnet + host-side HTTP CONNECT proxy. No DNS filtering, no vsock magic, no TLS interception.** The workload container attaches to `VmnetNetwork(mode: .VMNET_HOST_MODE)` — **no internet route at all**. A SwiftNIO TCP listener binds to the host-only gateway IP on the macOS host; guests are steered to it via injected `HTTP_PROXY`/`HTTPS_PROXY` (+ `GLOBAL_AGENT_*` and a `NODE_OPTIONS` preload of npm `global-agent`, because Claude Code is a Node app). HTTPS filtering uses the plaintext hostname in the CONNECT request — source comment: "the client sends a CONNECT request with the target hostname in plaintext before TLS begins, so we can filter without any certificate interception." Disallowed host → 403. Wildcards: `*.example.com` matches the bare domain and subdomains; empty list = deny all. **Fail-closed property**: tools that ignore proxy env vars get nothing — there is no route to bypass to.
- **Install-phase split**: agent `installCommands` (apt/npm) run on a default NAT vmnet network with full internet; afterwards the container is **recreated** on the host-only network with the rootfs `clonefile`'d. Build-time open, run-time filtered.
- **Built-in claude agent definition** (the only built-in; user JSON overrides at `~/.config/sandboxy/agents/`): `baseImage docker.io/library/node:22`; installs `git gh ripgrep jq…` + `@anthropic-ai/claude-code` + `global-agent`; `launchCommand: ["claude", "--dangerously-skip-permissions"]`; env `ANTHROPIC_API_KEY` forwarded from host, `IS_SANDBOX=1`; mounts `~/.claude → /root/.claude` (rw); `allowedHosts`: `*.anthropic.com, *.claude.com, npm.org, *.npmjs.org, *.github.com, *.githubusercontent.com, *.pypi.org, *.pythonhosted.org`.
- **Attach**: `container.exec` with `terminal = true`, host stdin/stdout, raw mode, SIGWINCH → `agentProcess.resize` — same contract jackin❯ gets via `container exec -it`.
- **Persistence**: rootfs snapshot on exit; `--name` resumes (sub-second warm starts claimed via cached kernel/init/rootfs).
- **vmnet requires macOS 26** (`#available(macOS 26, *)`); MTU lowered to 1400 to avoid PMTU black holes.
- **Not exposed in apple/container CLI**: code search confirms no `allowed-hosts`/network-filter flags exist in the CLI repo. Closest primitive: `container network create --internal` ("Restrict to host-only network") — the building block without the proxy.

### 6.2 What jackin❯ should take from it *(analysis)*

| Sandboxy technique | jackin❯ translation | Effort |
|---|---|---|
| Host-only network + host-side CONNECT proxy + proxy env steering | **This is Layer 4 (network egress policy), implementable today on the apple-container backend**: `container network create --internal jackin-<id>` per session; a Rust hyper/tokio CONNECT proxy inside the existing host process (peer of `exec_host.rs` — same lifecycle, same socket dir); inject `HTTP(S)_PROXY`/`NO_PROXY`/`GLOBAL_AGENT_*`/`NODE_OPTIONS` at launch; allowlist from workspace config; every allow/deny decision into the diagnostics run file (`clog!` summary + `cdebug!` per-connection). Fail-closed for proxy-ignoring tools by construction | Medium — the host process, socket conventions, and launch-env plumbing all exist in this PR |
| Install-phase NAT → runtime host-only recreation | jackin❯ equivalent: image build/pull happens host-side already; role `setup-once` hooks may need network — run first boot on `default` network, then recreate on the `--internal` network before agent launch (or: declare setup-once hosts in the role manifest and keep one network) | Medium; design choice for the egress roadmap item |
| Per-agent `allowedHosts` defaults (claude → anthropic.com, npmjs, github…) | Role manifests / agent definitions ship default egress allowlists per agent runtime; workspace config extends. jackin❯ already has the layering model (config → workspace → role) | Low (schema + docs; lands with egress item) |
| `--cpus 4` / `--memory 4g` defaults | Adopt as `AppleContainerSpec` defaults (configurable); Apple chose the same defaults twice (Sandboxy 4/4g, smolvm 4/8GiB) — sane agent-workload envelope | Low |
| `~/.claude` rw mount for agent state | jackin❯ already does credential/agent-state mounts better (per-agent `/jackin/<agent>/` mounts + auth sync modes); no change | — |
| Env-var credential forwarding (`-e ANTHROPIC_API_KEY`) | **jackin❯ is ahead**: jackin-exec's on-demand picker + host.sock resolution + output redaction is strictly stronger than Sandboxy's launch-time env injection (which any process in the VM can `printenv`). Keep jackin-exec; cite Sandboxy as the weaker baseline in the roadmap comparison | — |
| Rootfs `clonefile` snapshot + `--name` resume | Mirrors `container machine` persistence; reinforces §5.2 trajectory | — |

**Strategic read**: Apple just published, as an example, the architecture jackin❯ committed to in the isolation roadmap — per-session VM, allowlist egress, resource caps, explicit mounts, persistent resumable sessions. That (a) validates the design, (b) supplies a proven mechanism for the one layer jackin❯ had not designed in detail (egress), and (c) signals upstream trajectory: if Sandboxy graduates into the supported product, jackin❯ rides it; meanwhile the jackin❯ TUI, roles, multi-agent orchestration, and jackin-exec remain differentiators no Apple example touches.

---

## 7. The landscape — latest versions only (June 10, 2026)

Per operator directive: macOS-first, newest versions and features only. Contenders at their current best: **Docker Desktop 4.77.0** (2026-06-08) + **Docker Sandboxes** (standalone `sbx` CLI, GA, paid), **OrbStack 2.2.1** (2026-06-04), **apple/container 1.0.0** `container run`, **`container machine`** (1.0.0), **smolvm 1.0.1** (2026-06-05).

### 7.1 Security / isolation

| | Docker Desktop 4.77 | Docker Sandboxes (`sbx`) | OrbStack 2.2.1 isolated machines | apple/container 1.0.0 (`run`) | `container machine` (1.0.0) | smolvm 1.0.1 |
|---|---|---|---|---|---|---|
| Kernel boundary | One shared Linux VM for all containers | **VM per sandbox** (dedicated kernel) | One shared VM/kernel (7.0.11) for all machines; namespaces per machine | **VM per container** (Virtualization.framework) | **VM per machine** | **VM per workload** (libkrun on Hypervisor.framework) |
| Kernel-CVE blast radius | All containers + VM | One sandbox | All machines + Docker | One container | One machine | One workload |
| Privileged/DinD story | `--privileged` DinD (today's jackin❯ risk) | Private Docker daemon per sandbox, built-in | Docker inside isolated machines **fixed in 2.2.0** (shared kernel beneath) | `--privileged` unsupported; `--cap-add` with reduced default set; rootless DinD unvalidated (Phase 0) | *(analysis)* stock dockerd under systemd inside the machine VM — no outer-container privilege question; unvalidated (Phase 0b) | Docker-in-VM documented recipe (Alpine + ext4 + virtio-net) |
| Network policy | None native (ECI = Business tier, hardening not egress) | **Host-side proxy; Open/Balanced/Locked-Down policies; org-managed** | `--isolate-network` blocks host/machines; no domain allowlist | `network create --internal` (host-only) primitive; **no allowlist proxy in CLI** (Sandboxy demos one on the framework) | Networking undocumented | **Deny-by-default; `--net` opt-in; `--allow-host <h>` per-host allowlist, first-class CLI** |
| Credential posture | Env vars in container | **Host-side credential proxy** (key "never exposed inside the sandbox"); OS-keychain secrets | Opt-in `--forward-ssh-agent`; everything else manual | jackin-exec (this PR) provides it on top | same | `--ssh-agent` — "Private keys never enter the guest — the hypervisor enforces this" |
| Host FS exposure | Bind mounts | Explicit workspace mount, same path | **Selective mounts** (`--mount`), no Mac FS by default; ro flag undocumented | Explicit `-v`/`--mount` incl. **`readonly`**; `--read-only` rootfs | `$HOME` auto-mapped **rw by default** (`ro`/`none` available); no extra volumes documented | Explicit volume mounts |
| Supply chain / audit | Proprietary VMM | Proprietary VMM | Proprietary (closed-source core) | **Open source (Apache-2.0), Apple-maintained** | same | Open source (Apache-2.0) |

### 7.2 Performance

| | Docker Desktop | Docker Sandboxes | OrbStack 2.2.1 | apple/container 1.0.0 | `container machine` | smolvm 1.0.1 |
|---|---|---|---|---|---|---|
| Cold start | VM already running; container ~instant | microVM per sandbox (sub-second class, unbenchmarked) | Machine create seconds; container instant | **Sub-second VM** (Apple, WWDC26) — PR Phase 0 budget <5s to capsule-ready | Sub-second class + first-boot init | **<200 ms claimed** (self-reported) |
| Memory model | Static VM allocation | Per-sandbox VM | One VM, dynamic, very efficient (OrbStack's strength) | Per-VM; allocates as app needs **but freed guest pages not returned to host** (partial ballooning; Apple suggests restarts) | Defaults to **half of host memory** — must set `machine set memory=` | **Elastic virtio-balloon — host reclaims unused guest memory automatically**; idle vCPUs sleep |
| Warm resume | n/a | sandbox persists | machine persists | `container stop`/`start` (PR uses this) | **First-class persistence**; snapshot/resume | **VM fork: checkpoint/restore + CoW clones (v1.0.0)** — fastest possible "new session from warm template" |
| FS sharing | VirtioFS/gRPC-FUSE | virtio-fs class | Very fast (custom, OrbStack's benchmark win) | virtio-fs | virtio-fs | virtio-fs |
| x86 on ARM | Rosetta | Rosetta-class | Rosetta (fast) | **`--rosetta` flag** | undocumented | Rosetta noted in docs (unverified detail) |
| Per-workload limits | cgroup flags | sandbox config | **Per-machine CPU/mem/disk (new in 2.2.0)** | **`--cpus`/`--memory`** | **`machine set cpus= memory=`** | `--cpus`/`--mem` (defaults 4/8GiB) |

*(analysis)* Performance ranking for the jackin❯ "many parallel agent sessions on one Mac" shape: smolvm's elastic memory + fork is the theoretical best; apple/container is second with sub-second starts but needs explicit `--memory` caps + a restart-hygiene policy because of partial ballooning; OrbStack remains the most memory-efficient *shared*-kernel option (one VM amortized) — its efficiency is the flip side of the boundary jackin❯ is leaving it for.

### 7.3 Productivity / integration (for jackin❯ as the orchestrator)

| | Docker Desktop | Docker Sandboxes | OrbStack 2.2.1 | apple/container 1.0.0 | `container machine` | smolvm 1.0.1 |
|---|---|---|---|---|---|---|
| Programmatic surface | **bollard (full typed Rust API)** — today's backend | `sbx` CLI; `--output json` on some cmds; **no events API** (Rivet reverse-engineered an undocumented one) | `orb` CLI, JSON-ish | `container` CLI; **structured JSON/YAML/TOML `ls`/`inspect` (1.0.0)**; versioned XPC API promised | same CLI | CLI + HTTP/OpenAPI over Unix socket (Rust-friendly) |
| Attach quality | docker exec -it | `sbx exec` | `orb -m` (PTY behavior was undocumented — a deferral reason) | **`container exec -it` over vminitd gRPC/vsock — PTY + SIGWINCH + signals, validated design** | `machine run -n <n>` interactive shell | CLI exec (PTY behavior was a question mark pre-1.0) |
| Agent-runner fit | jackin❯ builds everything | **Complete agent runner — replaces jackin❯, not a backend** (closed runner, no API, paid) | Backend candidate (deferred) | **Primitive — exactly what jackin❯ needs** | Primitive+persistence | Primitive, agent-marketed |
| OCI images / role Dockerfiles | ✅ | ✅ | ✅ | ✅ (`container build` exists) | ✅ (machines built **from OCI images**) | ✅ (+ image-index v1.0.0) |
| Docker-workflow inside (Compose/Testcontainers) | native | private daemon ✅ | ✅ (fixed 2.2.0) | Phase 0 gate (rootless DinD) | *(analysis)* systemd dockerd — Phase 0b | documented recipe, constrained |
| Install friction (operator) | App + license | `brew install docker/tap/sbx` + **paid subscription** | App; **$8/user/mo commercial** | **One signed pkg / brew; free, open source** (not "built into macOS" — correction to roadmap docs) | included in same pkg | brew-class install, free |
| Cost | Free tier exists; ECI = Business $24/u/mo | **Separate paid subscription** | Free personal; $8/u/mo business | **Free** | Free | Free |

### 7.4 Maintenance health / strategic risk

| | Backing | Trajectory | Risk for jackin❯ |
|---|---|---|---|
| Docker Sandboxes | Docker Inc. | GA, paid, agent list growing | Product not primitive; no API; subordinates jackin❯ — rejected (roadmap Part 3 stands) |
| OrbStack | Small commercial (kdrag0n) | 2.2.x active; shared-kernel by design | No kernel boundary — structurally out for Layer 2; remains the recommended *Docker engine* for today's default backend |
| apple/container | **Apple, platform vendor** | 1.0.0 stable; WWDC session; machine + Sandboxy momentum; versioned API promised | Lowest long-term risk; macOS-26+ only (fine); closed Virtualization.framework underneath (can't extend devices) |
| smolvm | smol-machines org (new, ~6 mo old) | 1.0.1; 54 releases in 6 months; ~3.7k stars; self-reported benchmarks only | Young/bus-factor; but Apache-2.0, Rust, agent-focused — credible fallback, now with shipped features Apple lacks |

### 7.5 Verdict: Apple Container vs OrbStack (and the rest) — explicit answer

**Apple Container wins for jackin❯ Layer 2, and June 2026 widened the gap.** The question was always "who gives each session its own kernel on macOS, natively, with a scriptable lifecycle and a real PTY attach": OrbStack 2.2 still answers "nobody-but-shared-kernel" — its June improvements (per-machine limits, network blocking, Docker-in-isolated-machines fix) make it a better *productivity* sandbox but do not move the kernel boundary, which is the entire motivation (runc/netfilter-class CVEs reach every session). apple/container 1.0.0 answers it with: per-container VM, sub-second start, first stable release, structured CLI output, `--cap-add/--cap-drop`, host-enforced `readonly` mounts, `--internal` networks, vminitd gRPC/vsock attach — plus a platform-vendor trajectory (WWDC session, `container machine`, Sandboxy, DiskImageKit) that no third party can match on macOS. Docker Sandboxes remains the benchmark product but not a backend (closed runner, paid, no API — unchanged conclusion). smolvm graduates from "research fallback" to "credible fallback with two genuine advantages" (elastic memory, CLI-native egress allowlist, VM fork) — the right response is to *steal those ideas* on top of Apple Container (egress proxy per §6.2, explicit `--memory` caps + restart hygiene per §9.2), not to switch primaries. The PR's direction is confirmed; the implementation needs the 1.0.0 uplift below.

---

## 8. Gap analysis: PR #527 implementation vs container 1.0.0

Findings ordered by severity. File:line refs are to the PR branch.

1. **`container ps` parser targets a pre-1.0 schema (breaking risk).** 1.0.0 release notes: "cleaned up and structured output (JSON, YAML, TOML) for `ls` and `inspect` commands" — a **breaking CLI change** landing exactly on the surface `list_containers`/`extract_container_info` scrape with a substring heuristic (`status.to_lowercase().contains("running")`, capitalized+lowercase key fallback; `apple_container_client.rs:61-66, 287-299`; own TODO at :61-64 says "once Phase 0 pins the `container ps` JSON schema"). **Action**: re-pin the schema against 1.0.0 in Phase 0 (the phase0 script should capture `container ps --format json` raw output as an artifact), then replace the heuristic with typed serde structs. Also note `container inspect` now emits structured JSON — likely a better single-container probe than filtering `ps --all`.
2. **Version probe accepts anything.** `probe_version()` (`apple_container.rs:470-484`) logs the string but never gates. With 1.0.0's breaking output changes and the TOML-config migration, running against 0.x is now a real correctness hazard, not a nicety. **Action**: parse and require `>= 1.0.0`, fail with the install hint. Drop the "v0.11.0+" doc references.
3. **Mounts are silently rw-only.** `AppleContainerSpec.mounts: Vec<(PathBuf, PathBuf)>` (`apple_container_client.rs:40-50`) cannot express `readonly`, yet 1.0.0 supports `--mount …,readonly`. the jackin❯ mount model has ro semantics (global mounts, `/jackin/host/` is "read-only views" per AGENTS.md), so the backend currently **downgrades ro to rw silently** — a real security regression vs the Docker backend. **Action (in this PR)**: add a mode to the mount tuple, emit `--mount type=bind,source=…,target=…,readonly` for ro mounts, and report enforcement as host-enforced in the session contract.
4. **No resource limits.** No `--cpus`/`--memory` despite 1.0.0 support and the partial-ballooning behavior (peak memory held until stop). A runaway `npm install` in one session permanently claims host RAM until eject. **Action**: defaults (4 CPUs / 4–8 G, configurable; precedent: Sandboxy 4/4g, smolvm 4/8GiB), surfaced in the session contract; ties into the declarative-resource-limits roadmap item.
5. **host.sock transport: use `--publish-socket`.** The PR bind-mounts the whole socket dir (`socket_dir → /jackin/run`, `apple_container.rs:290-295`), exposing `agent.toml` + any future sockets wholesale. 1.0.0's `--publish-socket host_path:container_path` publishes exactly one Unix socket. **Action**: keep the dir mount for `agent.toml` if needed, but deliver `host.sock` via `--publish-socket ~/.jackin/sockets/<n>/host.sock:/jackin/run/host.sock`; smaller surface and survives socket-file recreation semantics better than a bind-mounted inode. Validate in Phase 0 (the flag's semantics under reconnect need empirical confirmation).
6. **Session contract claims need updating to 1.0.0 reality.** It should report: actual default capability set (reduced since 0.12.0) + any `--cap-add`, mount modes (ro/rw, host-enforced), memory/cpu limits, network name + `--internal` status, and replace "v0.11.0 rough edges" phrasing. `inner_docker_enabled: false` hardcode (`apple_container.rs:337`) stays until Phase 0.
7. **No per-session network.** Default network for every container = all jackin❯ sessions reachable from each other on macOS 26 (container-to-container works there). **Action**: `container network create jackin-<id>` per session (mirrors the Docker backend's per-agent network), `--internal` once the egress item lands; `network rm` on purge.
8. **`container stop` semantics**: 1.0.0 added `--stop-signal` (run) and `stop -s` — wire capsule's graceful-shutdown signal explicitly instead of relying on default SIGTERM behavior under vminitd. Also session finalization is unwired on this backend (`apple_container.rs:175-187`) — known Phase 2 gap, now worth a TODO(apple-container) entry referencing the upstream stop-signal support.
9. **DNS check is validated as permanent.** Upstream shipped no sleep/wake fix through 1.0.0 and closed #1321 as not-planned — the post-attach `nslookup` probe (`apple_container.rs:78-105`) and reconnect hint graduate from "workaround for v0.11.0 rough edge" to "standing mitigation". Keep; consider also probing pre-attach on reconnect.
10. **Roadmap/docs corrections** (same PR or immediate docs follow-up): apple/container is **not** "built into macOS 26 / zero install friction" — it is a separate signed-pkg/brew install (still the lowest-friction first-party option); v0.11.0 limitation tables must be re-dated to 1.0.0 (this document, §4.3, has the deltas); `container machine`, Sandboxy, smolvm-1.0, and OrbStack-2.2 developments belong in the respective roadmap items (apple-container-backend, smolvm-backend-research "when to revisit" triggers, orbstack-isolated-machine-backend, network-egress-policy, agent-isolation-architecture Part 6).
11. **Phase 0 script refresh** (`scripts/phase0-apple-container.sh`): pin to 1.0.0; add checks for `--mount readonly` enforcement, `--memory/--cpus` effect, `--publish-socket` round-trip, `network create --internal` reachability matrix, `container ps/inspect` JSON capture; and add the **machine-mode track** — `container machine create` from a role-image derivative with systemd + docker-ce, home-mount `none`, validate dockerd + `docker build` + Testcontainers inside the machine VM (§5.2). The DinD decision gate becomes: rootless-DinD-in-`run` **or** dockerd-in-`machine` — either unblocks Phase 2.

---

## 9. Proposals — creative, prioritized

### 9.1 Security

- **S1 — Egress allowlist proxy (Sandboxy pattern), the headline follow-up.** Per-session `--internal` network + Rust CONNECT proxy in the host process + proxy-env steering + per-agent default allowlists in role manifests + allow/deny audit into diagnostics. Fail-closed by construction. This converts Layer 4 from "future research" to "documented mechanism with an Apple reference implementation" — and it is the *only* differentiator Docker Sandboxes and smolvm currently hold over the jackin❯+apple-container stack. *(§6.2; new roadmap detail for network-egress-policy.)*
- **S2 — Host-enforced read-only mounts now** (gap #3). Cheapest real security win in the PR itself.
- **S3 — Capability transparency**: report the reduced default cap set in the session contract; hardened profile adds `--cap-drop` to minimum; every `--cap-add` is a contract line (the telemetry spec in the roadmap item already demands this).
- **S4 — Reframe the CAP_SYS_ADMIN gate in the threat model**: in-VM caps weaken in-VM layering only, not the host boundary — document this in agent-isolation-architecture so Phase 0's "if apple/container allows CAP_SYS_ADMIN the isolation story weakens" worry is scoped correctly. The host boundary is the hypervisor either way. *(analysis)*
- **S5 — Keep jackin-exec as the credential story and say why**: Sandboxy forwards raw env vars (printenv-visible in-VM); Docker Sandboxes proxies API-key injection but only for known agent APIs; jackin-exec's operator-picked, per-command, redacted-output model is stronger than both. Add this comparison to the container-credential-exposure / jackin-exec items.
- **S6 — `--ssh` agent forwarding as an opt-in launch flag** (host-side key custody for git push), mirroring the OrbStack item's "ssh_agent = off|ask|on" design — now trivially available on apple-container.

### 9.2 Performance

- **P1 — Default resource envelope** (`--cpus 4 --memory 4g`-class, configurable per workspace/role) + **memory-hygiene policy**: because guest-freed pages are not returned to the host, long-lived idle sessions should be `container stop`'d (eject) rather than left running; consider an idle-autostop option in the console resource panel roadmap item.
- **P2 — Warm-start path**: Phase 0 measures cold `run`→capsule-ready (<5 s budget); add stopped→`start`→ready measurement — reconnect latency is the operator-felt number. `container machine`'s persistence (V2) and, longer-term, ASIF/DiskImageKit images are upstream accelerants to watch.
- **P3 — Track smolvm's fork/CoW as the benchmark**: "new session from warm template in <1 s" is the bar; if apple/container grows snapshot/clone (Sandboxy already uses rootfs `clonefile`), adopt immediately for role-image first-boot amortization.
- **P4 — virtio-fs benchmarking in Phase 0** (large-repo `git status`, `npm install`, cargo build) vs OrbStack baseline, so the backend's session contract can state honest expectations; OrbStack's FS sharing is its best-in-class feature and operators will compare.

### 9.3 Productivity

- **Pr1 — The machine-mode V2 track** (§5.2): persistent per-instance `container machine` with systemd-supervised capsule + dockerd, home-mount `none`, explicit volumes when upstream ships them. Resolves DinD differently, gives operators a "real Linux box per session" mental model, and aligns jackin❯ with Apple's flagship feature. Add as an explicit phase/option in the apple-container-backend roadmap item now, gated on upstream volume support + Phase 0b.
- **Pr2 — Port publish for agent dev servers**: `-p` flag plumbed through workspace config so "agent starts vite on :5173" is reachable from the Mac browser — concrete daily-use win; session contract lists exposed ports (the network-privacy alert upstream added in 0.10.0 will surface — handle in launch UX).
- **Pr3 — `container cp` for artifact extraction** (new in 1.0.0): `jackin` could expose copy-out of build artifacts from ejected sessions without restarting them.
- **Pr4 — Structured `inspect` everywhere**: once the 1.0.0 JSON schema is pinned, `hardline --inspect` for apple-container gets real data parity with the Docker backend (manifest + live `inspect` merge).
- **Pr5 — Rosetta flag passthrough** for amd64-only role images (`--rosetta`), with a launch-summary warning — answers the per-arch question the OrbStack item left open, natively.
- **Pr6 — socktainer as an escape hatch, not a dependency**: if a role genuinely needs the Docker *API* against the apple-container backend before DinD validates, socktainer (partial Engine API v1.51) exists; document as community option, do not build on it (single-purpose shim, "experimentation" maturity).

### 9.4 Sequenced action plan

| # | Action | Where | Size |
|---|---|---|---|
| 1 | Re-validate + re-pin everything against 1.0.0: ps/inspect schema (gap 1), version gate ≥1.0.0 (gap 2), phase0 script refresh incl. machine-mode track + readonly/memory/publish-socket/`--internal` checks (gap 11) | This PR (script+client) | M |
| 2 | Read-only mount mode end-to-end + session-contract line (gap 3, S2) | This PR | S |
| 3 | `--cpus`/`--memory` defaults + contract lines (gap 4, P1) | This PR or follow-up | S |
| 4 | `--publish-socket` for host.sock (gap 5) — after Phase 0 confirms semantics | Follow-up | S |
| 5 | Per-session named network + purge cleanup (gap 7) | Follow-up | S–M |
| 6 | Roadmap/docs sync: 1.0.0 reality, machine V2 track, Sandboxy precedent, smolvm 1.0 + OrbStack 2.2 status updates, "built into macOS" correction (gap 10) | This PR's docs or docs follow-up | M |
| 7 | Egress allowlist proxy (S1) — full design lands in network-egress-policy item with the Sandboxy mechanism as reference | New PR (Layer 4) | L |
| 8 | Machine-mode Phase 0b empirical run + decision (Pr1) | After operator runs phase0 on macOS 26 HW | empirical |

---

## 10. Where this is heading — forward outlook *(analysis, signals cited)*

- **Apple**: 1.0.0 + WWDC session + `container machine` + vscode example + Sandboxy + promised versioned XPC API + DiskImageKit = sustained first-party investment in "Linux environments and sandboxed workloads as a macOS platform feature". Plausible next steps: Sandboxy concepts graduating into supported surface (agent definitions, allowlist proxy), machine volumes/networking docs, snapshot/clone primitives. Each one directly de-risks or accelerates a jackin❯ roadmap item — proximity to this stack compounds.
- **Docker**: Sandboxes is GA, paid, closed, agent-list-driven — confirms the market for "run agents safely" while staying a product jackin❯ competes with on operator experience, not a substrate. ECI stays enterprise-tier.
- **OrbStack**: now competing with `container machine` head-on (persistent Linux machines on Mac); without a per-VM-kernel answer it trends toward "best Docker engine + best shared-kernel machines", which keeps it as the jackin❯ *default Docker backend* engine and nothing more.
- **smolvm**: fastest-moving independent; watch for (a) third-party benchmarks of its <200 ms/elastic-memory claims, (b) whether VM fork + OCI-index turn into an agent-template product. Trigger for re-evaluation as primary: apple/container stagnating on memory reclaim or snapshot/clone for >2 release cycles while smolvm holds 1.x stability.
- **jackin❯ positioning**: "the only platform for agent CLIs in containers" now means: Docker backend for compatibility today, apple-container backend for the kernel boundary on macOS 26+, jackin-exec + egress proxy + TUI approval as the layers Apple/Docker/smolvm each only partially ship — with role authoring and multi-agent orchestration as the moat none of them touch.

---

## 11. Open questions / unverified

1. `--publish-socket` semantics under stop/start/reconnect (and whether it predates 1.0.0) — Phase 0.
2. `container machine` networking/IP/port semantics + extra-volume support — undocumented upstream; blocks machine-mode V2.
3. Whether vminitd remains PID 1 in machine mode (image `/sbin/init` as child) or hands off — affects capsule supervision design; Phase 0b.
4. Rootless DinD / dockerd-in-machine inside apple/container VMs — the standing empirical gate (now two-track).
5. smolvm performance figures are vendor-self-reported; no third-party benchmarks exist for any contender (incl. apple/container virtio-fs throughput) — Phase 0 measures locally.
6. Docker Sandboxes pricing specifics and macOS hypervisor framework — not published.
7. macOS 27 "Golden Gate" details are press-reported (MacRumors et al.); apple/container's macOS 27 posture unstated (README pins macOS 26 support).
8. WebFetch summarization caveat: quotes marked verbatim were reproduced through a fetch summarizer for a minority of sources; all load-bearing claims (release dates/versions/flags/Sandboxy mechanics) were verified against raw GitHub API/files or by 3-vote adversarial verification.

## 12. Primary sources

- github.com/apple/container — releases API (0.8.0…1.0.0), `docs/container-machine.md`, `docs/technical-overview.md`, `docs/command-reference.md` (tag 1.0.0), PR #1662, PR #1674, issue #66, issue #1321, PR #1504
- github.com/apple/containerization — README, releases API, tag 0.33.4, PR #607 (`examples/sandboxy/`: `RunAgentCommand.swift`, `HostProxy.swift`, `AgentDefinition.swift`, README)
- developer.apple.com/videos/play/wwdc2026/389/ — "Discover container machines"
- docs.orbstack.dev/release-notes, docs.orbstack.dev/machines/isolated, orbstack.dev/pricing, github.com/orbstack/orbstack#2469
- docs.docker.com/desktop/release-notes, docs.docker.com/ai/sandboxes/ (+ get-started/agents/CLI ref), docker.com Sandboxes launch blog
- github.com/smol-machines/smolvm (+ releases v1.0.0/v1.0.1), smolmachines.com
- github.com/socktainer/socktainer
- PR #527 branch sources: `crates/jackin-runtime/src/runtime/apple_container.rs`, `crates/jackin-runtime/src/apple_container_client.rs`, `crates/jackin-runtime/src/exec_host.rs`, `crates/jackin-capsule/src/{exec,mcp_server}.rs`, `scripts/phase0-apple-container.sh`; roadmap items under `docs/content/docs/reference/roadmap/`
