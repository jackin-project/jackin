# Issue Evidence

## Governing principles (apply to every fix)

**Judge by correctness, not ROI.** Known-wrong is never acceptable via "low-value / marginal / edge case / not worth it." "Competitor also gets it wrong" = gap, not justification.

Only valid stop: provably cannot be done (demonstrated tool/model limit). "Hard," "heavy," "expensive" never stops work — "proven impossible/blocked" does. Unsure → try it, measure it, prove it.

**Bug diagnosis rule.** Every bug = architecture *permitted* it. Before fixing, ask: why did the architecture allow this class of bug? Prefer the fix that removes the structural condition (so the class cannot recur) over a symptom-layer patch. Symptom patch only when root-cause fix is provably infeasible or belongs in a separate change — name the deferred root cause.

---

## Issue 1 — Claude OAuth usage HTTP 429 across all parallel instances

### Observed symptoms

Three parallel jackin' capsules started simultaneously. All three showed the Usage panel with every bucket returning `Claude OAuth usage HTTP 429 Too Many Requests`:

```
│  Session
│  Claude OAuth usage HTTP 429 Too Many Requests
│
│  Weekly
│  Claude OAuth usage HTTP 429 Too Many Requests
│
│  Daily Routines
│  Claude OAuth usage HTTP 429 Too Many Requests
│  Detail Claude OAuth usage HTTP 429 Too Many Requests
```

Footer state on all three: `stale`. After some time the third instance self-healed (backoff cleared).

### The three instances

| Footer ID | Account | Plan | Status |
|---|---|---|---|
| `c8pqe1vf` / `18bce00a6154c128` | `azhokhov@scentbird.com` | Enterprise | 429, stale |
| `ky1vwbra` / `18bce00f3bd5cb40` | `azhokhov@scentbird.com` | Enterprise | 429, stale |
| `es5sbb0q` / `18bce006cc87d3a8` | `alexey@chainargos.com` | Max | 429 initially → self-healed |

Instances 1 and 2 share the same Enterprise OAuth token (`azhokhov@scentbird.com`). Instance 3 uses a separate personal token.

### Code path

**Endpoint:** `https://api.anthropic.com/api/oauth/usage` (one GET per provider per refresh cycle)  
**Source:** `crates/jackin-capsule/src/usage.rs:2601–2618` (`fetch_claude_oauth_usage`)

**Refresh schedule:**
- `USAGE_REFRESH_BASE_INTERVAL = 5 min`
- `USAGE_REFRESH_JITTER = 1 min` (per-instance per-provider, hash-derived offset)
- `USAGE_REFRESH_BACKOFF_CAP = 30 min`

**On-startup behavior** (`should_refresh_with_cooldown_dir`, lines 489–507):
```rust
None => {
    self.next_due.insert(key, now);
    true   // ← first lookup: fire immediately, no delay
}
```
First call with no cached `next_due` → immediate fetch. All instances exhibit this on startup.

**Cross-instance coordination** (`shared_usage_cooldown_dir`, line 582):
```
~/.jackin/data/daemon/usage-cooldowns/
```
This is under `HOME` (host home, mounted into every container) → genuinely shared across all capsule instances.

**Critical gap:** `write_shared_usage_cooldown_marker` is called **only on 429**, never on success (`mark_refreshed_with_cooldown_dir`, lines 518–544):
```rust
if let Some(error) = view.last_error.as_deref()
    && usage_error_is_rate_limited(error)
{
    write_shared_usage_cooldown_marker(...)  // ← only on 429
} else {
    // success: only updates in-process next_due, writes NOTHING to shared fs
    self.next_due.insert(key.clone(), now + refresh_interval_for_key(&key));
}
```

### Root-cause analysis

**Structural condition that permits the bug:** the shared cooldown file is exclusively a *failure* artifact. It encodes "last 429" but never "last successful fetch." So when multiple instances start:

1. All read the shared cooldown → empty (no prior failure)
2. All have empty in-process `next_due` → all `should_refresh` returns `true` immediately
3. All fire `fetch_claude_oauth_usage` concurrently with the same (or rate-limited) token
4. Anthropic returns 429 to all
5. All write failure cooldown markers → too late, already rate-limited

This is a classic read-check-fetch TOCTOU race. The shared state is asymmetric: failures get shared, successes do not. The architecture has the right coordination mechanism (`shared_usage_cooldown_dir`) but applies it to only half of the states it needs to cover.

**Why a single-instance path doesn't exhibit this:** with one instance, the in-process `next_due` prevents re-fetching. Only multi-instance startup exposes the gap.

**Why instances with different tokens also hit 429:** Anthropic's `/api/oauth/usage` endpoint appears to rate-limit per-token or per-IP. Instance 3 (`alexey@chainargos.com`) self-healed faster, suggesting it hit its per-account limit from an earlier session fetch, not from this thundering herd alone.

### Required fix (structural)

The shared cooldown must cover **successful fetches** as well as rate-limit failures, with TTL = `USAGE_REFRESH_BASE_INTERVAL`. This ensures any instance that has fetched successfully within the base interval blocks other instances from re-fetching the same provider — eliminating the thundering herd at startup.

Two parts:
1. After a successful fetch, write a shared success marker (`~/.jackin/data/daemon/usage-cooldowns/`) with TTL = base refresh interval. Other instances starting fresh see "recently fetched, skip."
2. A fresh instance that sees an active cooldown (from another instance's success) should seed its in-process cache from a shared snapshot file (e.g., `~/.jackin/data/daemon/usage-cache/{provider}.json`) rather than showing "refreshing" forever.

**Symptom-layer patch (not sufficient alone):** random startup jitter reduces collision probability but does not eliminate it and does not address the shared-state asymmetry that is the root cause.

---

## Issue 2 — Auth in Usage panel must match synced auth for the active folder/role

### Observed correct behavior (what must always hold)

Each project folder uses its own Claude account via `auth_forward = "sync"`. The Usage panel MUST show the same credentials that were synced to the container — never the host account unless that IS the synced account.

Confirmed correct instances:

**scentbird project** (`c8pqe1vf`):
```
Anthropic                             azhokhov@scentbird.com
Auth: OAuth · /jackin/claude/credentials.json    Enterprise
```
Synced enterprise credentials (`/jackin/claude/credentials.json`) won — correct.

**scentbird AI project** (`ky1vwbra`):
```
Anthropic                             azhokhov@scentbird.com
Auth: OAuth · /jackin/claude/credentials.json    Enterprise
```
Same — correct.

**jackin project** (`es5sbb0q`):
```
Anthropic                             alexey@chainargos.com
Auth: OAuth · ~/.claude/.credentials.json        Max
```
Host home credentials won — correct for this project.

### Credential resolution order (`claude_snapshot`, usage.rs:1023–1028)

```rust
let oauth_candidates = [
    config.join(".credentials.json"),              // 1. $CLAUDE_CONFIG_DIR/.credentials.json
    home_path(".claude/.credentials.json"),        // 2. ~/.claude/.credentials.json (host home)
    home_path(".claude.json"),                     // 3. ~/.claude.json (host home)
    PathBuf::from(CLAUDE_HANDOFF_CREDENTIALS_PATH), // 4. /jackin/claude/credentials.json (synced)
];
```

The synced credentials (`/jackin/claude/credentials.json`) are **last** in priority. They win only when options 1–3 are all absent.

### Structural concern

This priority order means: if host home is mounted into a container AND `~/.claude/.credentials.json` exists on the host (true for the jackin project instance), option 2 wins over the synced credentials (option 4) — regardless of which account the role was configured to sync.

Currently this happens to produce the right result because:
- scentbird containers apparently do not have the host `~/.claude/.credentials.json` visible at `HOME`
- The jackin container does

But this is **fragile**: the correctness depends on the container mount layout, not on an explicit "use the synced credentials when sync mode is active" policy. If mount layout changes (e.g., host home is mounted for all containers), scentbird containers would start showing `alexey@chainargos.com` instead of the synced `azhokhov@scentbird.com` — breaking the invariant silently.

### Required fix (structural)

_Awaiting user confirmation of whether this is the exact issue they see, or a different failure mode. More description needed._

The correct structural fix is: when `auth_forward = "sync"` is active, the capsule should prefer `/jackin/claude/credentials.json` over any host-side credential path. The sync mode exists precisely to override the host account; the resolution order should reflect that intent.

---

## Issue 3

_Waiting for user description._
