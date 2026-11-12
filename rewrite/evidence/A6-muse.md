# A6 live proof T04 (Muse in sandbox)

Verdict: FAIL

## Versions
- jackin 0.1.0 built from HEAD 7c30d78c4 in temp worktree /tmp/jackin-a6-t04 (removed after proof). Proof-only change there: T04 TupleState::Unavailable -> Supported in crates/jackin-core/src/agent.rs. No change in main checkout.
- OpenShell gateway jackin-local Connected (https://localhost:17670). Provider jackin-muse-live-muse (type muse, 1 credential key) pre-existing; launch reported "provider: skipped, the provider exists".
- Muse Code 1.4.14 (host /opt/homebrew/bin/muse; same version in sandbox, `muse --version` inside sandbox).
- Tuple T04: agent muse, provider muse, model muse-spark-1.2, credential env META_API_KEY, profile endpoints allow only api.meta.ai.

## Account/provider
- Account muse-live (muse host-login, ~/.config/muse/auth.json). Host auth.json metadata: storage keychain, subs_active true, api_base_url https://api.meta.ai/v1 (metadata only; no secret read or printed).
- Sentinel role /Users/donbeave/Projects/jackin-project/jackin-sentinel branch feature/openshell-native. Temp workspace a6-sentinel registered for launch, removed after.

## Launch and vehicle
- Launch: DOCKER_CONTEXT=default <worktree jackin> load sentinel /Users/donbeave/Projects/jackin-project/jackin-sentinel --agent muse --provider muse --account muse-live --gateway jackin-local --task "Answer exactly: PONG. Then use one tool to list files in the current directory."
- Image build passed. Sandbox jackin-a6-sent-6kba created, ready; credentials/upload/supervisor/hooks/start all done.
- Vehicle: `openshell sandbox exec -g jackin-local jackin-a6-sent-6kba -- muse exec <prompt>` (one-shot flag found via `muse --help` / `muse exec --help` inside sandbox: `muse exec [PROMPT]`, `--json` available).

## Run output (FAIL)
Attempt 1 exact error: `malformed settings file at /sandbox/.config/muse/settings.json: invalid type: sequence, expected a settings JSON object at line 1 column 0`
The launch wrote /sandbox/.config/muse/settings.json (54 bytes) as TOML-style `[projects."/sandbox/project"] trust_level = "trusted"`, which Muse 1.4.14 rejects as a JSON settings object. This is a jackin defect: the generic trust writer emits TOML into a JSON settings file for T04.
Attempt 2 (settings replaced in-sandbox only with `{"schema_version":1,"provider":"meta","model":"muse-spark-1.2"}`) exact error: `Agent Definition filesystem source failed: IoError` (exit 1; also with `--preset empty`). Two failures with different causes, so the proof stopped per instructions. No PONG/tool-call output was obtained from the sandbox.

## Sandbox hygiene (PASS part)
- /sandbox/.config/muse/auth.json is 31 bytes: `{"META_API_KEY": "placeholder"}`; `grep -c placeholder` = 1. No real token in sandbox (verdict only; no token printed).

## Host concurrency (PASS part)
- While sandbox jackin-a6-sent-6kba was still running, host `/opt/homebrew/bin/muse exec "Answer exactly: HOST-PONG"` in /tmp printed `HOST-PONG`, exit 0. Host auth.json metadata after: storage keychain, subs_active true (unchanged).

## Second-account isolation
- Stated from code/tests only per instructions; not live-proven here. One line: account isolation is covered by existing account-isolation tests, no second live account was run.

## Refresh limitation
- Host keychain entry holds access_token + api_key and NO refresh token; gateway refresh cannot be configured. Token is opaque; no expiry claim is made. No refresh proof is claimed.

## Billing basis
- Host subscription: subs_active true. The sandbox run did not complete, so no gateway-held credential consumption through api.meta.ai is proven by this round; profile endpoints allow only api.meta.ai, no paid fallback host.

## Cleanup
- Sandbox purged, temp workspace a6-sentinel removed, worktree /tmp/jackin-a6-t04 removed (see report).

---

# Round 2

Verdict: FAIL

## What changed since round 1
- HEAD ab7d7dcd8 renders Muse settings.json as JSON. Sandbox jackin-a6-sent-plce (worktree /tmp/jackin-a6-t04b, T04 flipped to Supported there only).
- Settings fix CONFIRMED in sandbox: `/sandbox/.config/muse/settings.json` = `{"model":"muse-spark-1.2","provider":"meta","schema_version":1}` (valid JSON object with provider meta). Round-1 failure 1 is fixed.
- auth.json hygiene PASS: `{"META_API_KEY": "placeholder"}` only.

## Drive result
- `openshell sandbox exec -g jackin-local jackin-a6-sent-plce -- muse exec "Answer exactly: PONG. Then use one tool to list files in the current directory."` still prints exactly `Agent Definition filesystem source failed: IoError`. No PONG, no tool call.

## Diagnosis (inside sandbox, in order)
- (a) `/sandbox/.config/muse/` held only auth.json + settings.json at launch. No MUSE_* env vars. HOME=/sandbox. `muse init --dry-run` only scaffolds AGENTS.md. `muse skills list --source all` works (built-in skills listed).
- (b) Created `/sandbox/.config/muse/agents`, `/sandbox/.config/muse/skills`, `/sandbox/.local/share/muse/agents`: error unchanged.
- (c) `muse init` in /sandbox/project: AGENTS.md already exists; error unchanged before and after.
- (d) `muse exec --preset empty "hi"`: same IoError. Also tried: project `.muse/agents`+`.muse/skills`, `/sandbox/agents`, empty `agents.json` (`[]`) in config and data dirs, alternate HOME=/tmp/fake with copied config, non-git cwd /tmp/plain: all same IoError. Removing the created dirs: same.
- Extra finding: at launch there is NO `/sandbox/.config/muse/trust.json`. Creating it with a wrong shape gave `malformed trust store ... missing field decision`; correct shape is `{"schema_version":1,"projects":{"/sandbox/project":{"decision":"trusted"}}}` (host format). With correct trust.json the session log shows `trust.resolve outcome="trusted" source="user_config"`, but the Agent Definition IoError persists, so missing trust is a second jackin gap, not the root cause of this error.
- Host control: clean fake HOME on host does NOT reproduce the Agent Definition error (it fails later on credentials/catalog), so the failing filesystem source is sandbox-environment-specific, not a missing user-level agents/skills directory in the config dir. The exact source path could not be determined from inside the sandbox (no strace; muse logs only session.bind + trust.resolve before failing).

## Required jackin change (precise)
- FAIL because a manual in-sandbox action does not fix it; the root source is still unidentified. Next step for jackin: reproduce the muse 1.4.14 Linux binary under strace (or with its local-tracing at debug) in an environment matching the sandbox image and record the failing openat path. Then create that path in the role image / launch setup.
- Separately, jackin must write `/sandbox/.config/muse/trust.json` at launch with content `{"schema_version":1,"projects":{"/sandbox/project":{"decision":"trusted"}}}` (mode 0600, alongside settings.json), since Muse expects a trust store there and jackin previously emitted trust as TOML into settings.json.

## Host concurrency
- While sandbox was still up, host `muse exec "Answer exactly: HOST-PONG"` printed HOST-PONG. Host auth.json metadata unchanged (no secret read or printed).

## Cleanup
- Sandbox jackin-a6-sent-plce purged (`deleted sandbox jackin-a6-sent-plce`; sandbox list empty), workspace a6-sentinel removed (0 matches in config), worktree /tmp/jackin-a6-t04b removed. No commit, no push.

---

# Round 3

Verdict: FAIL

## Setup
- Worktree /tmp/jackin-a6-t04c at HEAD ab7d7dcd8, T04 flipped to Supported there only; built with `mise exec -- mbx +1.99.1 build -p jackin`. Sandbox jackin-a6-sent-vpqp launched from sentinel role with --agent muse --provider muse --account muse-live --gateway jackin-local; workspace a6-sentinel registered for launch. Sandbox kept up for the whole diagnosis. Muse 1.4.14 in sandbox.
- Settings at launch confirmed: /sandbox/.config/muse/settings.json = `{"model":"muse-spark-1.2","provider":"meta","schema_version":1}`; auth.json hygiene PASS (placeholder only, no secret printed).

## Step outputs (exact, inside sandbox)
1. `id` = uid=1000(sandbox) gid=1000(sandbox). pwd=/sandbox. Env (filtered): HOME=/sandbox, USER=1000 (numeric, no LOGNAME), OPENSHELL_SANDBOX=1, META_API_KEY present as gateway placeholder reference (value not printed), CA-bundle env vars pointing at /run/openshell-supervisor-ca. No MUSE_* or XDG_* vars. /sandbox rwxr-xr-x sandbox; /sandbox/.config drwx------; /sandbox/.config/muse drwx------ containing only auth.json (31B, 0600) + settings.json (63B, 0600) at launch. /sandbox/project is a real directory; /sandbox/project/.git is a directory (not a gitdir file), git status/log/ls-files all work inside the sandbox.
2. Session logs: per-session cli log (/sandbox/.local/share/muse/sessions/2026/11/06/<id>/cli-*.log) contains only two lines before failure: `session.bind host="cli" mode="exec"` and `trust.resolve outcome="untrusted" source="missing_store"` (later `outcome="trusted"` once trust.json existed / with --trust-workspace). Bootstrap log (/sandbox/.local/share/muse/local-tracing/bootstrap/cli-*.log, gate.resolve lines omitted) ends at `credential.status provider="meta" source="environment" available=true` — no agent/definition/source line is ever logged. The failure `Agent Definition filesystem source failed: IoError` appears only on stderr of `muse exec`, exit 1. Grep for agent|definition|source over sessions + local-tracing found no other diagnostic (matches were the permission-init retained frames and gate.resolve lines only).
3. Write tests: `touch /sandbox/.config/muse/probe` -> config-writable; `mkdir -p /sandbox/.local/share/muse && touch .../probe` -> data-writable.
4. `muse exec --provider echo "hi"` -> same `Agent Definition filesystem source failed: IoError`, exit 1. Also `--provider local`: same.
5. cwd: `cd /tmp && muse exec "hi"` and `cd /sandbox && muse exec "hi"` -> same IoError both. Non-git /tmp/plain2 also same. /sandbox/project/.git is a directory; target checks N/A.
6. Env override: HOME=/tmp/mh with XDG_CONFIG_HOME=/tmp/mh/.config XDG_DATA_HOME=/tmp/mh/.local/share, settings+auth copied -> same IoError. Also bare HOME=/tmp/bare with --provider echo -> same. Clean HOME + clean non-git cwd -> same.
7. RUST_LOG=trace muse exec -> no extra output, same one-line error.

## Tracing attempts (why the exact path is still unproven)
- No strace/ltrace in the image; no compiler, no sudo, apt unusable as uid 1000, sandbox network restricted.
- Cross-compiled (host zig) static ptrace tracer uploaded via `openshell sandbox upload`: PTRACE_TRACEME returns `Operation not permitted` in the sandbox (seccomp). A seccomp USER_NOTIF tracer also fails: seccomp() `Operation not permitted`. muse is a static ELF (no PT_INTERP), so LD_PRELOAD interposition is impossible. User namespaces: `unshare -rm` -> Operation not permitted.
- inotify watcher (cross-compiled, watches all dirs under /sandbox and /usr,/etc) does work and shows successful opens only. The last successful filesystem activity before the error, in order: read /sandbox/project/.git/config, read /sandbox/.config/muse/auth.json, read /sandbox/.config/muse/trust.json (when present), then directory opens of /sandbox/project (x3), /sandbox/.config (x1), /sandbox/.config/muse (x2) + IN_ACCESS on /sandbox/.config/muse, then session-dir opens under /sandbox/.local/share/muse/sessions and the .msp-maintenance-owner.lock open, then exit. No child binary is opened after ld.so.cache activity (no subprocess spawn on the failing path; PATH shims for bwrap/fd/python3/node/gh were never called). A failed open (ENOENT/EACCES) generates no inotify event, so the failing path itself is not directly observable this way.
- Only two directories in the whole image are unreadable by uid 1000: /root and /etc/ssl/private (find over /etc /usr /var /opt /srv /home /root /sandbox). /etc/xdg, /etc/muse, /usr/share/muse, /usr/local/share/muse, /var/lib/muse do not exist; XDG_CONFIG_DIRS/XDG_DATA_DIRS pointed at created /tmp/xdg-{config,data}/muse/agents did not change the error. Running a copy of the muse binary from /tmp/mb with exe-relative agent dirs created (/tmp/mb/agents, /tmp/share/muse/agents, etc.) did not change the error.

## Fix attempts that did NOT change the error
- trust.json correct shape (below) created -> trust.resolve becomes trusted, IoError persists.
- Creating, in various combinations: /sandbox/.config/muse/{agents,skills,plugins,workflows,memory,model-catalog,feature-config,runtime,pace-guardrail,sessions,local-tracing,agent-definitions,definitions,catalog,presets,hooks,rules,commands,output-styles}; same set under /sandbox/.local/share/muse; /sandbox/project/.agents/{agents,skills,workflows}, /sandbox/project/.muse/{agents,skills}, /sandbox/project/.claude/agents, /sandbox/.agents/{agents,skills,memory,workflows}, /sandbox/.claude/{agents,skills,projects}, /sandbox/.codex/{agents,skills}, /sandbox/.muse/{projects,agents}; agents.json (`{}`, `[]`) at config/project/home locations; /sandbox/AGENTS.md and /tmp/AGENTS.md; resetting the whole data dir /sandbox/.local/share/muse to empty; flags --disable-reminders, --disable-sandbox, --yolo, --no-foreign-personal-context, --trust-workspace, --preset empty, --agents "[]", --agents "{}". All: same one-line IoError, exit 1. No PONG, no tool call in any run.

## Required jackin change (precise, from round 2, reconfirmed)
- jackin must add /sandbox/.config/muse/trust.json at launch with content `{"schema_version":1,"projects":{"/sandbox/project":{"decision":"trusted"}}}` mode 0600 owner sandbox (alongside settings.json). Proven in-sandbox this round: with that file the session log shows trust.resolve outcome="trusted" (source user_config / run_flag); without it, outcome="untrusted" source="missing_store". This is necessary but NOT sufficient: the Agent Definition IoError persists with trust in place.

## Verdict reasoning
- FAIL: no manual in-sandbox change made `muse exec` print PONG or make a tool call, so there is no proven jackin filesystem change for the IoError this round. The error is environment-specific to the sandbox image (host muse with a clean fake HOME does not hit it, round 2), happens before any provider/network work (echo provider, bare HOME, non-git cwd all fail identically), and the failing path could not be observed because ptrace/seccomp/userns are all blocked in the sandbox and inotify cannot see failed opens.
- Next step for jackin (outside this sandbox's restrictions): run the muse 1.4.14 Linux binary under strace on a host/VM matching the role image (or in an OpenShell sandbox variant with ptrace permitted) and record the failing openat path; prime suspects from the inotify sequence are a path enumerated immediately after /sandbox/project and /sandbox/.config/muse, or a system-level path among /etc/xdg/muse, /etc/muse, /usr/local/share/muse, /root (EACCES). Then create that path in the role image / launch setup and re-run this proof.

## Cleanup
- Sandbox jackin-a6-sent-vpqp purged, workspace a6-sentinel removed, worktree /tmp/jackin-a6-t04c removed (see report). No commit, no push.

---

# Round 4

Verdict: FAIL — foreign-context hypothesis REFUTED

## Hypothesis tested
`--no-foreign-personal-context` ("Exclude foreign personal rules and skills from this run") plus creating foreign personal dirs would bypass the Agent Definition filesystem source IoError.

## Setup
- Worktree /tmp/jackin-a6-t04d at HEAD ab7d7dcd8, T04 flipped to Supported there only; built `mise exec -- mbx +1.99.1 build -p jackin`. Sandbox jackin-a6-sent-lm0w launched from sentinel role with --agent muse --provider muse --account muse-live --gateway jackin-local; workspace a6-sentinel registered for launch, removed after. Muse 1.4.14 in sandbox. Sandbox kept up through all tests.

## In-sandbox tests (exact outputs, in order)
1. Baseline `muse exec "hi"` -> `Agent Definition filesystem source failed: IoError`, exit 1.
2. `muse exec --no-foreign-personal-context "Answer exactly: PONG. Then use one tool to list files in the current directory."` -> same `Agent Definition filesystem source failed: IoError`, exit 1. No PONG, no tool call.
3. Created `~/.claude/agents ~/.codex ~/.agents ~/.config/muse/agents ~/.local/share/muse/plugins` (mkdir succeeded). Retry 1 -> same IoError exit 1. Retry 2 -> same IoError exit 1.
4. `muse exec --help` Safety section recorded: safety flags are --yolo, --trust-workspace, --disable-approval, --disable-sandbox, --sandbox-network, --disable-write, --disable-shell, --enable-shell-tool; `--no-foreign-personal-context` is listed under main Options. `muse exec --agents '[]' "hi"` -> same IoError exit 1. `muse exec --preset native-basic "hi"` -> same IoError exit 1.

## Hygiene / host concurrency
- auth.json placeholder-only: `{"META_API_KEY": "placeholder"}`, grep -c placeholder = 1. No secret printed.
- While sandbox was up, host `muse exec "Answer exactly: HOST-PONG"` printed HOST-PONG, exit 0.

## Verdict reasoning
- FAIL: every variant fails identically. The flag and the foreign dirs do not change the error, confirming round 3 (which had also tried --no-foreign-personal-context). The failing filesystem source is not the foreign personal context source, or it is enumerated before that exclusion applies.
- Next step remains round 3's: strace the muse 1.4.14 Linux binary in an environment matching the role image with ptrace permitted, record the failing openat.

## Cleanup
- Sandbox jackin-a6-sent-lm0w purged, workspace a6-sentinel removed, worktree /tmp/jackin-a6-t04d removed (see report). No commit, no push.
