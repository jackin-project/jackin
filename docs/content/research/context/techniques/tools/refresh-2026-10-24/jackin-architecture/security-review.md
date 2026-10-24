# Security, recovery and state isolation review

Date: 2026-10-24. Jackin source baseline: `6e266406b49da2ab3c55355f3b7d33d1643ee6cd`. Evidence is source inspection plus isolated dispatch/constraint probes and referenced product fixtures. A proposed context broker is not yet implemented; no claim of completed adversarial runtime qualification is made.

## Existing authority versus proposed extension

Current session launch supplies per-instance home, OS UID/GID, session roots and capability, and removes ambient account/folder/capability environment before applying admitted values. Linux process isolation validates scope, drops privileges and applies filesystem restrictions. Secure execution has separate credential admission/redaction. Control authorization treats configured session identities and capabilities specially; it must not be summarized as an all-purpose proof that every arbitrary peer is denied.

The current six-host entrypoint deliberately bypasses native prompts/sandbox modes. That existing choice does not authorize an optimizer to weaken Jackin isolation, expose credentials or retry a denied command. Record actual baseline permission flags in every task manifest.

The new worker/recovery broker requires its own authenticated, admitted contract. Reusing a socket, MCP transport or process wrapper does not automatically supply complete authorization, cancellation, root isolation or retention. Current `jackin-exec` cancellation notifications are ignored; general retrieval/result replacement is outside that server's current scope.

## Threat and lifecycle matrix

| Failure or adversary | Proposed required behavior | Evidence now / decisive missing test |
|---|---|---|
| Hook payload forges repository, worktree, UID or another session | Broker derives scope from trusted supervisor/peer metadata; rejects mismatch | Current session/capability source inspected; worker admission protocol not implemented |
| Recovery handle is guessed or equal-content hash crosses consumers | Unguessable reference plus broker ACL; digest verifies bytes but grants no access | RTK/Headroom content hashes are not sufficient session authorization; attempt cross-scope retrieval in broker test |
| Secret was redacted or operation denied | Archive/recover only authorized observed content; preserve denial/redaction | Current secure-exec source distinguishes these outcomes; test no optional-transform fallback exposes pre-redaction content |
| Worker crashes, times out or returns malformed data | Return the original already-captured authorized observation; never rerun producer | Product formatter failures demonstrate need; real host/stream/cancellation contract still unexecuted |
| Storage cannot acknowledge write | Emit original, no lossy replacement or marker | Headroom schema-loss and LemonCrow unusable-spill fixtures reproduce narrower upstream gaps; broker disk-full/crash tests still required |
| Stored artifact is missing, corrupted or expired | Report integrity/unavailability; do not fabricate or substitute current file/other session | Product recovery limits inspected; broker hash/ACL/retention fault corpus pending |
| Disk budget reached with active references | Stop admitting new loss; retain referenced artifacts under declared lease/horizon | Proposed contract; no pressure/lease implementation yet |
| Branch/worktree/cwd changes or concurrent roots | Canonical admitted request root and source generation; no global last-cwd routing | LemonCrow Lite explicit parallel roots passed; legacy/full/global state remains separate; Jackin adapter tests pending |
| Symlink swap, hard link, secret/ignored file or changed permissions | OS read roots plus point-of-use identity/authorization; ignore files are not the sole security boundary | Linux and Lean path checks inspected; adversarial TOCTOU/inode/permission tests not executed |
| MCP schema hides execution/edit tool | Server dispatcher rejects nonallowed calls and worker lacks write/exec authority | LemonCrow hidden grep remained callable; Lean steering-off still advertises execution/edit. Facade allowlist is a distinct positive control. |
| Compaction, clear, resume, fork, subagent or unknown delivery | New exposure epoch unless particular retained content continuity is proven; no omission based only on warm source cache | Lean same-consumer cold stub and different-consumer/marker controls reproduced; actual host lifecycle remains to verify |
| Off, upgrade or rollback with prior handles | Stop new transformations, preserve needed broker/artifacts through declared recovery horizon | Proposed migration contract; cannot retroactively erase past compressed context |
| Malicious source/tool text becomes memory | Treat it as data, retain provenance; do not promote it into authoritative policy or execute embedded instructions | Design requirement; memory injection/poisoning fixtures pending |
| Shared immutable index reveals another tenant's private files | Require identical admitted security domain/source/configuration or separate storage | Source index sharing is a proposed optimization, not current permission |
| Provider proxy rewrites established history/reasoning | Separate provider-specific auth/streaming/prefix/preserved-reasoning qualification | Current native/provider docs impose distinct contracts; no proxy activated |
| Upstream telemetry, self-updater or warmer sends private data/calls provider | Explicit package/config manifest, disabled unrequested egress and enforced worker policy | Current product default inspection identifies these paths; no broad packet-capture claim |

## Isolation granularity

One transform owner per observation, one selected index owner per declared root/configuration, one exposure owner per consumer/epoch and one effective instruction-asset owner prevent nested markers and contradictory state. They do not imply one physical database. Raw originals, indexes, notes and measurement receipts need different retention and access policies.

A subprocess is the conservative first boundary for LeanCTX's global environment/runtime and LemonCrow's historical shared state. Process separation alone is not a sandbox: launch it with narrow filesystem roots, explicit configuration, private state and no inherited credentials. Native edit receipts, stale-write checks and provider authentication remain with the host unless separately replaced and qualified.

## Acceptance and unresolved scope

Zero new critical correctness, authorization, isolation or recovery failures is a hard gate. Statistical uncertainty about aggregate acceptance is not permission to ship a known critical defect. The smallest useful next test is a single real host/worker observation round trip with denied operation, failed archive, worker timeout and epoch reset, then two simultaneous worktrees/consumers and the complete task protocol. Source-level defenses and existing unit tests are not a substitute for those boundaries.

## Post-cutoff integration-base overlay

Main advanced to `c98f5f5fdd831e6075e38fde93b9713575406140` during the research. All 26 original source-audit files remain byte-identical. The separate [overlay review](main-overlay-review.md) records new checked-install descriptors, non-activating format3 consistency inspection, container-root preparation guards and cooperative Prepared/Committed account-marker recovery. Preserve their existing failures and foreign-state protections when implementing optimizer packaging. These controls do not establish model-exposure identity, supply-chain trust or a completed observation broker; no overlay runtime test was run.
