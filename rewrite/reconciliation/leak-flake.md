# Flake: `lifecycle::providers_profiles_and_detail_follow_the_gateway_contract`

Source: `/private/tmp/claude-501/plans/push-j0.log` (lines 1300 and 2617), the pre-push gate on the merge `da955bb1f` of P9.3 J0a and J0b. One failure; the test passed 4 of 4 when run alone. Branch `wt/leak-flake`, base `da955bb1f`.

```
panicked at crates/jackin-testkit/tests/gateway/lifecycle.rs:187:17:
<the test secret> leaked into [SandboxHandle { cell: Cell { name: SandboxName("jackin-work-p1uw"), dir: TempDir { ... } ...
```

## Result

Not a secret leak. The test's detector gave a false positive. The exact test secret was not in the text. A three-character fragment of it matched a random identifier that the `Debug` output of the sandbox handle prints. Product code was not affected. The flake needs no change of J0 to explain it: the test and its fixtures predate J0 and the chance per run is constant. I did not bisect.

## Root cause

`leaks` (`crates/jackin-testkit/tests/gateway/lifecycle.rs:22`) reports a leak when any three-character window of the secret appears anywhere in the text. That is a sound way to catch a truncated or re-encoded secret. It is only sound when no window can appear by chance.

Both fixtures had windows made of letters and digits only. Six per secret. The handle's `Debug` prints three random strings made of letters and digits:

| Text | Source | Alphabet |
|---|---|---|
| `LaunchId("...")`, 26 characters | `LaunchId::generate`, `crates/jackin-core/src/ids.rs:679` | `a-z`, `2-7` |
| the name suffix, 4 characters | `NameSuffix`, `crates/jackin-core/src/ids.rs:660` | `0-9`, `a-z` |
| `TempDir { path: ".../jackin-fake-sandbox-XXXXXX" }`, 6 characters | the `tempfile` crate | `A-Z`, `a-z`, `0-9` (inferred from the sample in the log and from the default of the crate) |

Every run draws a new launch ID and a new temp directory name, and each draw has a small chance to contain one of the fixture windows. The chance per run is near one in a thousand, so the test passes alone and fails once in a long gate.

The gate log shows it: the matching window sits inside the launch ID `5d6yqvxet5zeapgnpzil3pojcq` of the failing handle, as a three-character substring that is a window of the refresh fixture.

## Evidence

The test binary was run under 12 busy `yes` processes (18 CPUs), started and killed by PID. The script `analyze.py` took each panic line, recomputed which windows matched, and printed the text around the match.

| Run | Iterations | Failures |
|---|---|---|
| single test, nextest `--stress-count`, before the fix | 4000 | 4 |
| whole `jackin-testkit` binary, `--stress-count`, before the fix | 120 | 1 |
| single test, after the fix | 4000 | 0 |
| whole `jackin-testkit` binary, after the fix | 120 | 0 |
| sibling `jackin-openshell` gateway tests `exec`, `providers`, `connect` (23 tests), after the fix | 300 | 0 |

The five failures: three matched inside a launch ID, one inside the temp directory name, one inside a launch ID in the whole-binary run. In every one the match was a single window, and the exact secret was not in the text. The rate is about 0.1 percent per run, as computed above.

Is there a real leak behind the noise? A temporary probe (not kept) put the provider, created a sandbox that used it, and printed the `Debug` of the fake, the handles, the provider spec, the sandbox detail and the stored provider record while the provider still existed. None of the five texts held the exact secret or its first eight characters.

## Product code

Not affected. In `jackin-openshell`, `ProviderMaterial::ChatgptLogin` (`crates/jackin-openshell/src/gateway.rs:118`) holds both tokens as `Secret`, whose `Debug` prints `Secret(<redacted>)` (`crates/jackin-core/src/secret.rs:25`). The session types (`crates/jackin-session/src/credentials.rs:16`) hold `Secret` as well. The gRPC code copies the value into the request only inside `provider.rs` and builds a `Redactor` from the same secrets for its errors (`crates/jackin-openshell/src/grpc/provider.rs:254`); the existing test `providers.rs:209` checks the error text and `Debug`.

The fake keeps the tokens as plain `String` in `ProviderRecord` (`crates/jackin-testkit/src/gateway.rs:84`). Its `Debug` is written by hand and leaves them out (line 94), so nothing prints them today. Holding them as `Secret` would remove the chance that a later `derive(Debug)` prints them, but `Secret` has no `Clone` or `PartialEq`, and the record derives both. That change is a separate decision and is not part of this fix.

## Why the structure allowed it

The detector's soundness depended on a property of the fixtures that nothing stated or checked, while the text it scans holds random identifiers. The same two fixtures were copied into other tests, so the property was lost without notice.

## Fix

In `crates/jackin-testkit/tests/gateway/lifecycle.rs`:

- The two fixtures changed so that every three-character window holds a symbol, and a symbol never appears in a generated ID, a name suffix or a temp directory name. A window cannot match by chance any more.
- A `const` assertion (`every_window_has_a_symbol`) fails the build when a fixture has a window of only letters and digits. The property is now checked at compile time.
- `leaks` and the assertion are unchanged. The check stays as strict as before: it still flags any three-character fragment.

## Related sites

- `crates/jackin-openshell/tests/gateway/exec.rs:58`, `providers.rs:50` and `connect.rs:28` use the same window check with the same old fixtures (`exec.rs:27`, `providers.rs:15`, `pki.rs:11`). Their texts are error strings and `Debug` of values with fixed names, not of random IDs. 300 stress iterations of the 23 tests passed. They were not changed. If one of them ever starts to print a generated ID, give it a fixture with the same property.
- The assertion at `lifecycle.rs` runs after the provider is deleted, so it does not cover the stored provider record. The probe above did. A check before the delete would cover it. That change needs a test plan from `test-reviewer`, so it is not made here.
