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

### Fix — shipped in commit `0d206728`

Both parts implemented in `crates/jackin-capsule/src/usage.rs`:

**Part 1 — success cooldown marker.** After every successful fetch, `mark_refreshed_with_cooldown_dir` now writes a shared cooldown marker in `~/.jackin/data/daemon/usage-cooldowns/` with TTL = `USAGE_REFRESH_BASE_INTERVAL`. Fresh instances (startup path, `next_due == None`) see the marker and skip the fetch.

**Success vs. rate-limit semantics.** Rate-limit cooldowns are mandatory: they block all refreshes including user-triggered `mark_due`. Success cooldowns are advisory: they block fresh-startup probes only. The distinction is encoded in the cooldown file reason line — `"ok"` = success (advisory), any other string = rate-limit (mandatory). `should_refresh_with_cooldown_dir` now checks `shared_usage_rate_limit_cooldown_active` in the scheduled/`mark_due` path and `shared_usage_cooldown_active` (both types) in the startup path.

**Part 2 — shared snapshot for seeding.** After a successful fetch, the view is serialized to JSON in `~/.jackin/data/daemon/usage-snapshots/usage-{hash}.snapshot.json`. In `refresh_active_account_snapshots`, when a target is blocked by cooldown and the in-process cache has no entry for it, the shared snapshot is read and inserted — so the instance shows data instead of "refreshing" for the cooldown duration.

**Part 3 — pre-fetch advisory marker (commit `TBD`).** The post-fetch success cooldown still left a race window equal to the HTTP fetch duration (~1-3 s): parallel instances that all check cooldown before any one completes still race. Fixed by writing an advisory `"ok"` marker with TTL = `PROVIDER_PROBE_TIMEOUT` (35 s) for each due target immediately before dispatching HTTP requests — closing the window from ~HTTP-latency to ~RAM-operation latency (µs). On completion `mark_refreshed` overwrites with the permanent success marker (5 min) or the 429 backoff marker.

**Thundering herd fully resolved.** All three mechanisms working together:
1. Success cooldown (post-fetch, 5 min) — blocks 5-minute refresh cycles from racing
2. Pre-fetch advisory marker (pre-dispatch, 35 s) — closes the startup race window
3. Shared snapshot seeding — instances blocked by cooldown show data instead of "refreshing"

---

## Issue 2 — Auth plan label wrong for Enterprise/Team accounts; container starts unauthenticated

### Observed symptoms

**Expected:** each project shows its correct Claude account tier in the TUI header and Status tab:
- jackin project → `Claude Max` ✓ (already correct)
- scentbird project → `Claude Enterprise`
- scentbird AI project → `Claude Team`

**Actual:**
```
▝▜█████▛▘  Sonnet 4.6 · API Usage Billing          ← wrong tier label
   Auth token:       none                            ← unauthenticated
   cwd:              /Users/donbeave/Projects/scentbird/scentbird
```

Both scentbird and scentbird-AI instances showed `API Usage Billing` and `Auth token: none`. The actual Claude Code agent outside the container authenticated correctly (`Login method: Claude Enterprise account`, `Organization: Scentbird`, `Email: azhokhov@scentbird.com`).

### Root-cause analysis — Bug 2a: wrong plan label

`claude_snapshot` derived the plan label from `claudeAiOauth.subscriptionType` in `.credentials.json`. For Enterprise and Team accounts, Anthropic stores the **billing model** here (`"API Usage Billing"`), not the account tier. The account tier is stored in `oauthAccount.organizationType` in `.claude.json`:

```json
// ~/.claude.json (confirmed from real Max account)
{
  "oauthAccount": {
    "organizationType": "claude_max",  ← tier ("claude_enterprise", "claude_team", …)
    "organizationName": "alexey@chainargos.com's Organization",
    "billingType": "stripe_subscription",  ← billing model (different field)
    ...
  }
}
```

**Structural condition that permitted the bug:** `claude_snapshot` read from `claudeAiOauth` (credentials file) for the plan label. The credentials file is the right place for the access token; it is not the right place for the account tier. `oauthAccount` in the account file (`.claude.json`) is the authoritative source for the tier, but was only read for the email address, not the tier.

### Fix 2a — shipped in commit `9193e555`

Added `claude_organization_type_from_value` that reads `oauthAccount.organizationType` from the same candidate files already scanned by `resolve_identity`. Priority in `plan_label`:

```
organizationType (account tier, most reliable)
OR subscription_type (credentials file fallback when account file unavailable)
```

Result:
- Max: `organizationType: "claude_max"` → `"Claude Max"` ✓
- Enterprise: `organizationType: "claude_enterprise"` → `"Claude Enterprise"` ✓
- Team: `organizationType: "claude_team"` → `"Claude Team"` ✓

### Root-cause analysis — Bug 2b: container starts unauthenticated

`setup_claude()` in `runtime_setup.rs` copied credentials to `~/.claude/.credentials.json` only when `is_first_seed()` returned true. If the first seed ran **without** credentials available (Keychain read failed at container creation time, or source folder wasn't configured), the agent home was seeded with empty auth state. On all subsequent launches, `is_first_seed()` returned false (home already has content) → credential copy was skipped → agent permanently unauthenticated, even after `/jackin/claude/credentials.json` became available.

**Structural condition that permitted the bug:** the design intent ("never overwrite a token a later tab refreshed") was implemented as an unconditional gate: `if is_first_seed() { copy creds }`. This conflated two distinct states: "in-container token was refreshed (preserve it)" and "in-container has no token (always seed)." The absence of credentials in the container was not a checked condition.

### Fix 2b — shipped in commit `9193e555`

Added a fallback path in `setup_claude()`: when NOT first-seed AND `~/.claude/.credentials.json` is absent AND `/jackin/claude/credentials.json` is present, copy the forwarded credentials. Also re-seeds `account.json` from the forwarded copy if the container's copy is still the empty `{}` skeleton (since `account.json` carries `organizationType` for the plan label fix).

```rust
} else if !credentials_path.exists() && forwarded_creds.is_file() {
    // Non-first-seed but credentials absent: re-seed from forwarded copy.
    copy_file_with_mode(forwarded_creds, &credentials_path, 0o600)?;
    if forwarded_account.is_file()
        && fs::read_to_string(&account_path).map_or(true, |s| s.trim() == "{}")
    {
        copy_file_with_mode(forwarded_account, &account_path, 0o600)?;
    }
}
```

This does NOT overwrite existing in-container credentials (the guard `!credentials_path.exists()` ensures it only runs when the file is absent), preserving the original design intent for the normal case.

---

## Issue 3

_Waiting for user description._
