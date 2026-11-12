# A14 live proof T08 (Kimi Code with Kimi), T07 (MiMo Code with MiMo)

Verdicts: T08 FAIL (not proven: the ordered 1Password credential source stayed unavailable for the whole run window; no sandbox was launched). T07 FAIL (same cause). No tuple is claimed Supported by this file. Never report SKIP as PASS.

## What is proven and prepared (key-independent part, all done)

- Flipped binary: temp worktree /tmp/jackin-wt-live-native, detached HEAD df7674dd4 ("test(session): use T14 for tuple support ordering after T10 flip"). Proof-only diff: T07, T08 `TupleState::Unavailable` -> `Supported`, reason `Some("not yet proven on a real gateway")` -> `None` in crates/jackin-core/src/agent.rs (4 insertions, 4 deletions, verified by `git diff --stat`). Built with the A14 defect 3 cache-bypass habit (`touch crates/jackin-core/src/agent.rs`, mbx then reported a fresh compile; `Finished dev profile in 11.45s`). No change in the main checkout, no commit, no push.
- T07 credential env name confirmed from the code, as the brief ordered: `MIMO_CODE_SPEC` credential env `MIMO_API_KEY` (crates/jackin-core/src/agent.rs:804), profile asset `t07-mimo.yaml` (env_vars `[MIMO_API_KEY]`, host `mimo.mi.com:443`, binary `/usr/local/bin/mimo`, checksum-pinned MiMo-Code v0.1.16 arm64 artifact). T08: `KIMI_API_KEY` (agent.rs:833), profile `t08-kimi.yaml` (host `api.kimi.com:443`, binary `/usr/local/bin/kimi`, kimi-code 2.5.1 arm64 pinned).
- Agent names for the CLI confirmed from code, as the brief ordered (`jackin load --help` + `parse_agent`): `--agent kimi` and `--agent mimo` (`Agent::as_str()`; KimiCode -> "kimi", MimoCode -> "mimo"). `account add` defaults Kimi->KimiCode and Mimo->MimoCode (`default_agent`), so the planned commands were `jackin account add kimi --name kimi-native --agent kimi --credential env:KIMI_API_KEY --gateway jackin-local` and `jackin account add mimo --name mimo-native --agent mimo --credential env:MIMO_API_KEY --gateway jackin-local`.
- Role: git clone of jackin-sentinel feature/openshell-native at 89b2ee8 into /tmp/jackin-wt-sentinel2 (a clone, not a `git worktree`, so the sandbox receives a real `.git` directory and the export/purge gate works). Uncommitted additions, A8 pattern (55d2d14): `[agents.kimi] model = "kimi-for-coding" instructions = "instructions/kimi.md"`, `[agents.mimo] model = "mimo-v2.6-pro" instructions = "instructions/mimo.md"`, plus the two instruction files copied from the instructions-omp style, agent-appropriate. `jackin role check .` run in the clone with the flipped binary: exit 0, no findings. Nothing committed in jackin-sentinel, per the brief.
- Gateway jackin-local (OpenShell 0.1.7, mTLS https://localhost:17670) reachable, no sandboxes of ours; provider list has NO pre-existing native `jackin-*-kimi`/`jackin-*-mimo` providers, so credential attribution for the proof would have been direct (A14 defect 2 cannot interfere). The A14-left providers jackin-omp-kimi-omp / jackin-omp-minimax-omp / jackin-omp-mimo-omp are untouched.
- Workspace `a14n-sentinel` -> /tmp/jackin-wt-sentinel2 added with the flipped binary; `~/.config/jackin/config.toml` backed up to /tmp/jackin-a14n-config-backup.toml (1722 bytes, same size as the A14/A16 baseline). Default gateway in config is jackin-docker, so every command carries `--gateway jackin-local`.
- Image pipeline exercised key-independent: `jackin build sentinel` built and unpacked a derived image (sha256:e820b95f1222e8618673c78941a3eae8223cec0c0390057a8381856227338d19). A throwaway probe sandbox from it showed the image layer cache is codex-only: per-launch agent sets drive the derived layer (kimi/mimo binaries download at the `load --agent kimi|mimo` launch, as in the A4 omp launches). Probe sandbox deleted. Two foreign sandboxes jackin-a14zai-p8ca / jackin-a14zai-2qbi appeared on the gateway during the run (another lane, names not ours); left untouched.

## Blocker: 1Password credential source unavailable for the whole run window

Ordered sources (brief): Kimi `op read 'op://ChainArgos/Kimi/Test'` (A14/A16-proven, 72 chars), MiMo `op item get n5nq2lz4hvqhqsdifkbes644r4 --fields TokenPlan` (A14 T12-proven, 51 chars, `tp-` prefix). Verbatim failures, all at "error initializing client" (the CLI-to-desktop-app handshake), never a vault/permission error:

- 2026-11-09T00:28:02Z `op read` authorization timeout (then an internal retry at 00:29:02Z: same)
- 00:29:16Z `op whoami`: `[ERROR] account is not signed in` (account list sees zhokhov.1password.com; 1Password.app 8.12.44 running; group-container agent sockets present; no OP_ env vars; no `1Password CLI` keychain item)
- 00:30:24Z after `open -a 1Password`: authorization timeout
- 00:31:50Z `response: promptError`; 00:32:00Z `response: promptError` (fast fail)
- 00:34:10Z, 00:35:50Z authorization timeout (bounded retry loop; a third loop attempt was cut by the loop deadline)
- 00:36Z-00:50Z background watcher (60 s cadence): 7 further authorization timeouts; flag never reached "unlocked"

Interpretation: the desktop app requires an interactive confirmation (unlock/allow) that nobody provided; the secret never left the vault, no key was printed or written anywhere. The T07/T08 blocker files stay accurate in their main-tree wording ("no host plan key") only for a host WITHOUT the vault; on this host the keys are one unlock away, so the dated gap is the authorization, not the absence of the keys. No other reachable store holds these keys (host Kimi login is OAuth with empty api_key; host omp store holds only a Z.AI record; OpenShell providers do not return stored credential values).

## Planned live sequence (unexecuted; run as-is after the vault unlocks)

1. `export KIMI_API_KEY=$(op read 'op://ChainArgos/Kimi/Test')` and `export MIMO_API_KEY=$(op item get n5nq2lz4hvqhqsdifkbes644r4 --fields TokenPlan)` — environment only.
2. `jackin account add` commands as named above; expect accounts `kimi-native kimi env` and `mimo-native mimo env`, providers `jackin-kimi-native-kimi` and `jackin-mimo-native-mimo` at resource version 1 (A16 sync_provider republishes on each launch).
3. `DOCKER_CONTEXT=default jackin load sentinel /tmp/jackin-wt-sentinel2 --agent kimi --provider kimi --account kimi-native --gateway jackin-local --task "Answer exactly: PONG. Then use one tool to list files in the current directory."` (and the mimo twin). Non-TTY load returns the sandbox id without attaching (known A4 behavior), so drive via `openshell sandbox exec -g jackin-local <sbx> -- sh -c 'cd /sandbox/project && kimi -p "<same task>"'`.
4. Hygiene per A14/A16: uid 1000(sandbox); `sudo -n true` rc 127; `grep -c placeholder` on the jackin-written config/auth files (`/sandbox/.kimi-code/config.toml`, `/sandbox/.kimi-code/credentials/kimi-code.json`, `/sandbox/.mimo/config.toml`, `/sandbox/.mimo/auth.json`); raw-key absence by prefix-shaped grep plus stdin-fed exact grep (never exec argv — A16 proof-hygiene incident); brokered-token fingerprint differs from the raw key; process tree `<agent>` under `jackin-start <agent>` PPID 1.
5. Export/purge gate (T10/A16 pattern): commit a marker on the sandbox branch; guarded `jackin purge <sbx>` must refuse with "not exported"; `jackin export <sbx>` publishes `refs/heads/jackin/<sbx>/<branch>`; guarded purge then deletes.
6. Cleanup: purge sandboxes, remove accounts, remove workspace, restore config byte-identical, `git update-ref -d` exported refs in the clone.

## Cleanup (state at hand-over)

- Config restored byte-identical from /tmp/jackin-a14n-config-backup.toml right after the workspace removal (cmp passed at that moment); `jackin account ls` and `workspace ls` matched the pre-proof rows (no kimi-native/mimo-native, no a14n-sentinel). A concurrent lane on this host later added its own rows to the shared config (roles sentinel-kimi and sentinel-mm, workspace a14mm); a diff against the backup afterwards shows exactly those three blocks and nothing from this proof.
- No sandbox of ours remains on jackin-local; probe sandbox deleted; nothing committed or pushed in any repository; gateway providers unchanged.
- Worktrees kept: /tmp/jackin-wt-live-native (flips + this evidence) and /tmp/jackin-wt-sentinel2 (role additions, uncommitted).

## Defects and notes

1. No jackin defect observed: every failure in this run is the host 1Password authorization, upstream of jackin.
2. Tooling note: `openshell sandbox create` client timed out streaming after 300 s while the sandbox itself reached Ready; state checks and exec still worked. Not a jackin surface.
3. Reminder for the re-run: `jackin build` pre-warms only the default-agent (codex) derived image; the kimi/mimo derived layers build during their launches.
