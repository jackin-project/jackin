# A14 T09 re-proof (omp 18.18.1 pin) and T01 gate run

Date: 2026-11-08. Jackin main checkout at HEAD `bab554313` "fix(account): make omp API-key publish agent-aware (A11)", branch `feature/openshell-native`, clean tree, no code edited, nothing committed. Role `../jackin-sentinel` at HEAD `89b2ee8` "fix(hooks): scan selected index blobs for secrets" (same branch). Gateway `jackin-local` (OpenShell 0.1.7, mTLS, https://localhost:17670). No secret read, printed, or written; no credential file content inspected.

## Verdict

- T09 sandbox omp pin 18.18.1: **PASS** (version string plus byte-exact sha256 match below).
- T01 / G-E2E gate at current HEADs: **FAIL** — `RESULT PASS=4 FAIL=13 SKIP=2 elapsed=228.5s a1_host_login=unchanged`. Deterministic cross-repo contract skew, not an environment defect: the sentinel probe emits `schema: 2`, jackin still pins `schema: 1`. Details and citations below.

## Gate run (exact command from the brief)

```
mise exec -- mbx +1.99.1 xtask e2e --gateway jackin-local --role ../jackin-sentinel --builder orbstack --platform linux/arm64 --engine-port 2376 --engine-certs ~/.local/share/jackin-e2e-engine/client
RESULT PASS=4 FAIL=13 SKIP=2 elapsed=228.5s a1_host_login=unchanged
```

Counts (journeys J00–J17 plus CLEAN):

- PASS 4: J01, J02, J03, J17 (probe-independent journeys; J17 "workspace beats role beats default; usage has A1 windows and typed fixture reasons").
- FAIL 13: J00, J04, J05, J06, J07, J08, J09, J10, J13, J14, J15, J16, CLEAN. J05–J10 fail with "the shared probe failed: the probe output breaks probe-schema.json"; J11/J12 are SKIP "blocked by J07 FAIL".
- SKIP 2: J11, J12.
- `a1_host_login=unchanged` — host login untouched.

Full journey log preserved at `/tmp/a14-e2e-run.log` during the run.

## Root cause of the gate FAIL (evidence)

1. Sentinel probe emits schema 2: `../jackin-sentinel/probe/jackin-sentinel-probe` line 2221 — `'{schema: 2, probe: "jackin-sentinel-probe", mode: $mode, docker: $docker, steps: add}'`. Introduced by sentinel commit `33cf977` "fix(probe)!: omit process arguments from reports" (2026-11-07 07:48:48 +0700, marked breaking).
2. Jackin still pins schema 1: `rewrite/probe-schema.json` — `"schema": {"const": 1, "description": "The version of this contract. A breaking change raises it."}`, loaded by `crates/xtask/src/e2e/probe.rs:17` (`SCHEMA_TEXT: &str = include_str!("../../../../rewrite/probe-schema.json")`).
3. No jackin commit raises the const: `git log --all --oneline -S '"const": 2' -- rewrite/probe-schema.json` returns nothing at HEAD `bab554313`.
4. Every failure message quotes `/schema: 1 was expected`, including the shared probe J00 itself; the J07 facts shown in the error (codex_exec answered, exit 0) indicate the sandbox side worked — only the contract check rejects the report.

So the sentinel role shipped a breaking probe bump without the matching `probe-schema.json` raise in jackin. Fix belongs in jackin (`rewrite/probe-schema.json` to schema 2, with the schema-2 shape changes from `33cf977`: process arguments omitted); that is a code change, out of scope for this no-code-edit proof run. Re-run the gate after the raise.

## T09 omp 18.18.1 pin proof

- Spec at jackin HEAD: `crates/jackin-core/src/agent.rs` `OMP_SPEC` — version `"18.18.1"`, release URL `https://github.com/can1357/oh-my-pi/releases/download/v{version}/omp-linux-{target}`, arm64 sha256 `3bd7699fe7ea63a8eb14f27290d9d3c6a56e55388bef6c560cb139d809330528` (amd64 `2a75e16550676783b3bc8be0e06513478dd5881efc647954642ab30db8f9be06`), auth file `/sandbox/.omp/auth.json`, config `/sandbox/.omp/config.toml`, binary `/usr/local/bin/omp`. The A11 bump (plan row: "omp pin in c539ca4f3") is present at HEAD.
- Sandbox proof (a sandbox remained after the run, so the brief's first path applies): `jackin-a14-sen-2j75` (sentinel role, created 2026-11-08 15:24:16Z — after the A11 bump merged; launched by a concurrent A14 lane, image built from the same branch spec):
  - `openshell -g jackin-local sandbox exec jackin-a14-sen-2j75 -- sh -c '/usr/local/bin/omp --version'` → `omp/18.18.1`, exit 0.
  - `sha256sum /usr/local/bin/omp` → `3bd7699fe7ea63a8eb14f27290d9d3c6a56e55388bef6c560cb139d809330528` — byte-exact match with the OMP_SPEC arm64 pin.
  - Hygiene (metadata only, A9 pattern): `/sandbox/.omp/config.toml` mode 600, 167 bytes, 1 placeholder line; `/sandbox/.omp/auth.json` mode 600, 55 bytes, 1 placeholder line; binary mode 755, 241,814,520 bytes. No key material read.
- Host omp: 18.18.1 (host and sandbox pin agree at this pin level).
- Quota: this proof ran no omp agent turn, so no Z.AI quota was consumed by this file's checks. Baseline recorded before the gate (2026-11-08T15:31:57Z, percentages only): Zai plan `max`, Zread monthly 0.0%, 5h window 48.0%, weekly 46.0%.

## D24 engine fixture rebuild (was down; prerequisite of the gate)

- The A9 `jackin-engine` OrbStack machine no longer existed (`orbctl list` empty; `https://localhost:2376` refused). Rebuilt fresh from the A9-engine.md procedure: `orbctl create -a arm64 ubuntu:24.04 jackin-engine`, Docker CE 29.9.3 (docker-ce via download.docker.com noble repo) — newer than the A9-recorded 29.9.2 per the latest-stable rule.
- Difference from A9: dockerd runs as the **system** (rootful) service with TLS, not rootless. Reason: in this fresh machine the rootless daemon's child namespace could not read files under `/home/donbeave` (reproducible `could not read CA certificate ... no such file or directory` from `dockerd-rootless.sh` while the same path read fine in login shells; the OrbStack home mount flapped between invocations). Moving the daemon and certs fully off `/home` was not enough for the user unit, so the boring rootful unit was used. External contract unchanged and verified: TLS on `tcp://0.0.0.0:2376`, server cert SANs `DNS:localhost, DNS:host.openshell.internal, DNS:jackin-engine, DNS:jackin-engine.orb.local, IP:127.0.0.1, IP:192.168.139.177`.
- The old Mac client certs were signed by a CA whose key lived only in the deleted machine, so a fresh CA was generated in-machine; fresh client certs installed at the same path `~/.local/share/jackin-e2e-engine/client/{ca,cert,key}.pem` (dir 0700, files 0600). CA sha256 prefix `5d0a4c38050a79b9`.
- Verified from the Mac: `curl https://localhost:2376/version` → Docker Engine 29.9.3, linux/arm64, ApiVersion 1.56.

## CLEAN findings and residue handling

- The gate's CLEAN journey FAILed with two complaints:
  1. "leaked container e2e-4dc0 in the engine on port 2376 (the sandbox engine variables are unknown)". Handled: both containers were already gone when checked; the dangling `hello-world` image left in the engine was removed by ID; the engine now has zero images and zero containers.
  2. "the OpenShell registry /Users/donbeave/.config/openshell changed during the run (D03: jac…" — the message is truncated by xtask itself at ~320 bytes, full reason not recoverable from the log. Observed concurrent-lane traffic that can explain a registry change inside the run window: sandbox `jackin-a14-sen-2j75` created 15:24:16Z by another A14 lane, `[accounts.omp-minimax]` added to `~/.config/jackin/config.toml` during the run window (mtime 15:37Z; all 16 pre-run sections intact, none removed), and an `a14-kimi-proof` provider on the gateway. Not investigated further; likely cross-lane contamination of the D03 check rather than a leak from this run.
- Sandboxes after the run: the ledger removed the run's own sandboxes (`openshell -g jackin-local sandbox list` shows only the other lane's `jackin-a14-sen-2j75`, which was left untouched).
- No commit, no push; this evidence file is the only main-tree write.

## Re-run 2026-11-08 after the schema-2 fix: PASS=12

Date: 2026-11-08, run window ≈15:53Z–16:02Z. Jackin HEAD `831d46210` "fix(e2e): raise probe contract to schema 2": `rewrite/probe-schema.json` `"const": 1` → `"const": 2` and the report `args` field removed (matching sentinel `33cf977`), with `crates/xtask/tests/e2e/b_probe.rs`, `crates/xtask/tests/e2e/data/probe_host.json`, and `crates/xtask/src/e2e/scenario/report.rs` updated. Sentinel unchanged at `89b2ee8`, clean tree. No code edited in this re-run; nothing committed.

Same gate command as the first run:

```
mise exec -- mbx +1.99.1 xtask e2e --gateway jackin-local --role ../jackin-sentinel --builder orbstack --platform linux/arm64 --engine-port 2376 --engine-certs ~/.local/share/jackin-e2e-engine/client
RESULT PASS=12 FAIL=5 SKIP=2 elapsed=569.2s a1_host_login=unchanged
```

Command exit code 1 (FAIL nonzero). Result versus the first run (PASS=4 FAIL=13 SKIP=2): nine previously-failing journeys flipped to PASS — J00, J04, J05, J06, J10, J13, J14, J16, CLEAN — and J17 flipped PASS→FAIL, net 12/5/2. The schema raise is proven effective: every journey that failed with "the probe output breaks probe-schema.json" now passes, and a live sandbox probe report `{"schema":2,...}` is accepted by the host (J15 shell transcript validates J04 and J15 check sets end-to-end).

### The five FAILs — all environmental, none implicate the schema change or jackin code

- J07, J08, J09, J17 share one cause: the A1 ChatGPT login is expired. J17 usage JSON reports all three accounts `{"state":"unavailable","reason":{"kind":"expired_login"}}` (`J17-usage-stdout.log`). J08 TUI transcript: `Error: account/read failed during TUI bootstrap: account/read failed: workspace routing discovery unauthorized (401) (code -32603)` then `jackin-start: codex exited with code 1 (attempt 2)` (`J08-reflection-transcript.log`); the reflection check times out because the TUI never bootstraps. J07: `jackin-start` supervisor `Exited {attempt: 1, code: 1}` with no codex child. J09: `codex_reaches_api` — `codex exec exit 1 gave no answer` (the J09 `plain`/`wildcard` stderr policy rejections are the journey's own negative cases, not the failure). J17 passed in the first run, so the login went stale between the runs; `a1_host_login=unchanged` — the gate did not touch it. Fix: refresh the A1 login, then re-run.
- J15: transient engine fault. `docker run -i --rm busybox cat` failed with `Post "https://host.openshell.internal:2376/v1.56/images/create?fromImage=docker.io%2Flibrary%2Fbusybox&tag=latest": EOF` (exit 125), and the very next command in the same journey, `docker run -d busybox sleep 120`, pulled busybox and ran (`J15-shell-transcript.log`). First-pull EOF against the D24 engine, self-healed within seconds.
- SKIP 2: J11, J12, "blocked by J07 FAIL".

### CLEAN verdict: PASS, uncontaminated

Pre-run (15:52:21Z): `openshell -g jackin-local sandbox list` → no sandboxes (the other lane's `jackin-a14-sen-2j75` was already gone); `~/.config/openshell` mtime 2026-11-06 11:49:27; engine on 2376 zero containers and zero images. Post-run (16:02:31Z): no sandboxes, registry mtime byte-identical (D03 clean — this time no cross-lane registry writes inside the window), engine zero containers. CLEAN itself: "5 sandboxes, 3 providers removed; registry unchanged" — all five sandboxes were this run's own (`jackin-e2e-694-hbjo`, `jackin-e2e-694-ctvh`, and the J08/J14 set), since the pre-run list was empty. No other-lane traffic touched the gateway during the window.

Residue handled: J15 left `busybox:latest` (`a46257e06c82`) and a dangling `hello-world:<none>` (`c567b231f656`) in the engine; removed by ID, engine back to zero images and zero containers.

Run logs preserved at `target/xtask-e2e/6948/`. No commit, no push; this appended section is the only main-tree write of the re-run.
