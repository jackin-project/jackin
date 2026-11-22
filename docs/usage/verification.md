# Native usage verification

The [reference matrix](reference-parity.md) and [extension matrix](extension-parity.md) identify each provider’s data source and its boundary fixtures. The [terminal walkthrough](terminal-walkthrough.md) shows the keyboard interactions. The [CLI guide](cli.md) describes the offline snapshot format.

## Deterministic checks

| Interface | Evidence |
| --- | --- |
| Sign-in and account replacement | Loopback callback tests check state, pasted responses and listener cancellation. Device and grant tests exercise polling, refusal, token exchange and identity. Registry tests reopen the database after a failed vault write, cancellation, renewal or replacement. [Browser](../../crates/usage/jackin-usage/tests/browser.rs) · [Devices](../../crates/usage/jackin-usage/tests/device_login.rs) · [Registry](../../crates/usage/jackin-usage/tests/connected_accounts.rs) |
| Account isolation and hubs | Two independent fault injections verified that the lease and origin-binding fixtures fail when a refresh can run during replacement or metadata can redirect the management key. Both tests passed on the implementation. [Lease and origin checks](../../crates/usage/jackin-usage/tests/connected_accounts/leases.rs) |
| Priority and alerts | Fixtures use saved accounts and controlled time. They check grouped limits, independent currencies, pinning and hiding, thresholds, resets and alert recovery. Four deliberate behavioral changes failed the corresponding checks. [Dashboard fixtures](../../crates/usage/jackin-usage/tests/dashboard/tests.rs) |
| Provider history | The MiMo fixture checks independently refreshed billing and consumption results, exact integer counts and fixed-point money. It failed when a successful history read was retried on the failed-read schedule. OpenRouter fixtures cover a standard key, management-key pagination, BYOK and partial history. [MiMo](../../crates/usage/jackin-usage/tests/extension_connections.rs) · [OpenRouter](../../crates/usage/jackin-usage/tests/openrouter_connection.rs) |
| Offline CLI | The public CLI fixture uses a closed provider endpoint and accounts without a usable credential. It checks empty, pending, zero and stale snapshots, provider and UUID filters, rejected filters and a private receipt excluded from stdout. [CLI fixture](../../crates/jackin/tests/cli/usage_snapshot.rs) |

## Repository and platform checks

- The locked workspace suite ran 2,589 tests with all passing and one additional test skipped. A subsequent independent run of 37 lifecycle, reset, export and CLI checks passed after moving credential operations off the terminal runtime.
- The published console dependency is pinned to `22d2ebc59e06b0306f174f01e73231edccc7d1bd`. All 1,198 functional tests passed on this revision, including 149 jackin-console journeys. One additional test was skipped. All 946 non-functional tests passed. Two fixtures initially failed intermittently on both the original console and the feature branch. The environment editor fixture now waits for the Save focus indicator. The narrow export fixture now waits for the application to redraw at 80 columns. Delaying the original application reproduced the initial 1,426-cell screenshot difference. Both corrected fixtures passed five consecutive runs on the original baseline and passed under load on the published revision.
- A Debian container using a real session bus and GNOME Keyring stored a binary credential containing zero and non-UTF-8 bytes, reopened it, replaced it, deleted it twice and observed the missing-credential result. The public documentation build passed.
- G-FULL passed formatting, strict Clippy, workspace tests, dependency checks, repository checks and the documentation build. The release product check and console branch ancestry check also passed. This migration adds one Cargo deny advisory exception, explained below.

## Live verification and platform limits

- A ChatGPT subscription account completed jackin’s browser sign-in on macOS. An explicit CLI refresh for its saved account ID returned the plan, weekly usage, credits and reset inventory. A separate offline snapshot for the same ID returned a fresh reading with the same fetch timestamp. Other providers’ authenticated usage reads, live token renewal and reset redemption remain unverified. Automated provider tests use synthetic responses.
- Live device authorization returned user codes through the jackin CLI for Muse, OpenCode, Grok and ChatGPT. The ChatGPT check occupied the loopback callback port to exercise its device fallback. Grok initially rejected xAI’s verification host. After allowing the exact HTTPS host `accounts.x.ai` for Grok, the CLI displayed the provider’s challenge. Each flow was cancelled before approval, returned exit code 130 and left its isolated account list empty.
- The three MiMo Token Plan regional endpoints returned HTTP 401 for unauthenticated requests. This verifies reachability and the requirement for authentication. It provides no evidence for a successful quota read. [MiMo endpoint and regional evidence](extension-parity.md)
- Gemini requires Google consent and an entitled or eligible Code Assist project. The optional Kimi and MiniMax device flows require a usable OAuth registration. Each provider's candidate grant and regional quota reader have separate fixtures. [Extension contracts](extension-parity.md)
- A loopback OAuth callback requires a browser that can reach jackin’s listener. ChatGPT supports a device fallback when its fixed listener is unavailable. Linux builds need pkg-config and libsecret development headers (libsecret-1-dev on Debian or Ubuntu). At runtime Linux requires libsecret-1, a session bus and an unlocked default Secret Service collection supporting encrypted transfers, and macOS requires access to the login Keychain. [Account connection guide](cli.md)
- The prescribed OpenShell end-to-end run initially found the local gateway offline. A retry connected over mutual TLS to OpenShell 0.1.7 on port 17670 using an isolated gateway database. The execution gate stopped at its A1 credential prerequisite because the isolated test login directory contained no ChatGPT execution login. No authenticated gateway journey ran.

## Dependency review for chrono

RustSec classified all published versions of `chrono` as unmaintained on November 17, 2026. The advisory lists no patched release. Four jackin crates originally depended directly on chrono. Three now use the already locked `time 0.3.55` for dates and timestamps. The session clock uses `jiff 1.0.2` to reload the system time zone in long-running processes. Jiff caches the detected zone for up to five minutes and applies its daylight-saving rules at each timestamp. The final inverse dependency tree contains three external parents. [Advisory](https://rustsec.org/advisories/RUSTSEC-2026-0543.html) · [Maintainer announcement](https://github.com/chronotope/chrono/issues/1768)

The migration checks cover UTC date boundaries, fractional seconds, signed half-hour offsets, weekday validation and RFC 3339 separators. The negative half-hour and invalid-separator cases failed on the first replacement parser. Independent checks compared New York winter, summer and transition offsets. On Debian 12 with glibc 2.36, a running process started with `TZ` unset and a UTC `/etc/localtime`. After switching that file to UTC+7 and waiting 306 seconds, chrono and Jiff returned +25,200 seconds. The `time` local-offset call still returned zero. The public jackin session clock also observed the new offset in a same-process macOS test. [Public date and backoff tests](../../crates/usage/jackin-usage/tests/usage/rate.rs)

On Linux and macOS, the pinned OpenShell SDK enables core telemetry and depends on OAuth2 5. A separate minimal Cargo project depending only on the latest stable OpenShell SDK, v0.1.9 at `ce58cf8e9b953281352d98a637e41616295b1318`, resolved both paths to `chrono 0.4.45` with `default-features = false`. Core telemetry uses chrono for timestamps. OAuth2 exposes chrono types in its token API. A core-only change would leave the OAuth2 dependency. Both projects have proposed migrations. [OpenShell SDK manifest](https://github.com/NVIDIA/OpenShell/blob/ce58cf8e9b953281352d98a637e41616295b1318/crates/openshell-sdk/Cargo.toml) · [OpenShell migration](https://github.com/NVIDIA/OpenShell/pull/5763) · [OAuth2 migration](https://github.com/ramosbugs/oauth2-rs/pull/355)

The published `russh-util 0.52.0` adds another edge for WebAssembly. The SSH library’s tagged source already uses `web-time`, but the published utility crate still uses chrono. Switching native SSH transports would leave both OpenShell paths. The optional SQLx SQLite chrono feature is inactive in this workspace. [Published russh-util manifest](https://docs.rs/crate/russh-util/0.52.0/source/Cargo.toml.orig) · [Tagged upstream fix](https://github.com/Eugeny/russh/blob/v0.65.4/russh-util/Cargo.toml)

Inspect the remaining parents after upgrading any of these dependencies:

```sh
mise exec -- mbx +1.99.1 tree --locked --all-features --target aarch64-apple-darwin -i chrono --depth 2
mise exec -- mbx +1.99.1 tree --locked --all-features --target x86_64-unknown-linux-gnu -i chrono --depth 2
mise exec -- mbx +1.99.1 tree --locked --all-features --target all -i chrono --depth 2
```

The cargo-deny exception names only `RUSTSEC-2026-0543`. Remove it when the all-target tree no longer resolves chrono. Other advisories, including future security advisories against chrono itself, retain the repository's normal policy.
