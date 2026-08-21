# 08 — Surface seams and release tooling

Vetted: 2026-08-21
Questions: Where must each planned usage surface attach, what existing owners must remain authoritative, and which repository commands prove implementation, rendering, signing, and publication?
Informs: unified-agent-usage
Method: direct source inspection, task-definition inspection, release-workflow inspection, and active branch/PR verification on 2026-08-21

## Delivery boundary

Every implementation plan produced from this research must execute on the existing `chore/roadmap-unified-agent-usage` branch and existing draft PR #898. No plan may create, recommend, or require another branch or PR. The branch and PR were verified directly with `rtk git branch --show-current` and `rtk gh pr list --head chore/roadmap-unified-agent-usage --json number,state,isDraft,headRefName,url` on 2026-08-21. This is an operator-set delivery constraint, not an architectural inference. (confidence: HIGH)

## Surface ownership map

### CLI

The current `jackin usage` command is not the settled bare host-wide command. `UsageArgs` still requires a flattened `UsageScope`; host access is nested under `UsageScope::Host`, and the help describes Capsule cache, cache mutation, host snapshot, and verification paths. `run`, `run_host`, `run_host_snapshot`, `run_cache`, `run_accounts`, and `run_verify` own dispatch and rendering today. JSON uses the generic `OutputEnvelope`, while human host rows are printed directly from the current snapshot view. Planning must replace this command grammar at the command boundary, retain explicitly selected instance inspection/verification as the separate Capsule-scoped path, and introduce one renderer over the canonical broker projection rather than layering a new renderer over `host snapshot`. — `crates/jackin/src/cli/usage.rs:90-160`, `crates/jackin/src/cli/usage.rs:160-297`, `crates/jackin/src/cli/usage.rs:297-424`, `crates/jackin/src/cli/format.rs:14-41` (confidence: HIGH)

Plan-relevant owners:

| File / symbol | Planning implication |
|---|---|
| `crates/jackin/src/cli/usage.rs:90-160` — `UsageArgs`, `UsageScope`, host/account arguments | Redesign the public grammar around bare host-wide output while preserving explicit Capsule inspection/verification without a host cache bypass. |
| `crates/jackin/src/cli/usage.rs:160-297` — `run`, `run_host`, `run_host_snapshot` | Make the broker projection the sole host read/refresh authority and freeze human/JSON exit behavior here. |
| `crates/jackin/src/cli/usage.rs:297-424` — cache/account/verify and row printing | Remove or relocate host-side cache mutation; retain Capsule verification; replace duplicated row formatting with canonical Rust strings. |
| `crates/jackin/src/cli/format.rs:14-41` — `OutputEnvelope` | Do not mistake generic CLI envelope versioning for the canonical usage projection schema. |
| `crates/jackin/src/cli/usage/tests.rs`, `crates/jackin/src/cli/usage/store/tests.rs` | Rewrite command, JSON, partial-failure, empty, and no-bypass regression coverage at the existing command seam. |

### Host Console TUI

There is no usage screen in `jackin-console` today. The central manager is `ManagerState`; routes are represented by `ConsoleManagerStage` and `ConsoleManagerStageRoute`; screen modules currently cover workspaces, editor, edit-save, and settings. Shared frame ownership already exists in `view.rs`, `layout.rs`, `split.rs`, `components/brand_header.rs`, and `components/footer_hints.rs`. The host adapter/effect boundary is separate under `crates/jackin/src/console/`. Planning must add Usage as a real top-level console route through this state/update/view architecture and request broker operations through effects/services. It must not embed provider I/O in a view or copy Capsule modal orchestration wholesale. — `crates/jackin-console/src/tui/state.rs:6-8`, `crates/jackin-console/src/tui/state.rs:230-319`, `crates/jackin-console/src/tui/model/stage.rs:12-57`, `crates/jackin-console/src/tui/screens.rs:1-10`, `crates/jackin-console/src/tui/view.rs:228-317`, `crates/jackin-console/src/tui/components/brand_header.rs:10-45`, `crates/jackin-console/src/tui/components/footer_hints.rs:1-62`, `crates/jackin/src/console/adapter.rs:1-20`, `crates/jackin/src/console/effects.rs:1-21` (confidence: HIGH)

Plan-relevant owners:

| File / symbol | Planning implication |
|---|---|
| `crates/jackin-console/src/tui/model/stage.rs:12-57` — stage and route enums | Add Usage route/navigation in the central typed route model. |
| `crates/jackin-console/src/tui/state.rs:230-319` — `ManagerState` | Own Usage selection, projection, refresh, notice, and scroll state centrally. |
| `crates/jackin-console/src/tui/screens.rs:1-10` and sibling `screens/*` modules | Add a dedicated Elm-style model/message/update/view screen, not conditionals spread across existing screens. |
| `crates/jackin-console/src/tui/view.rs:228-317`, `layout.rs:1-18`, `split.rs` | Preserve the shipped console header/content/footer and split-pane geometry. |
| `crates/jackin-console/src/tui/components/brand_header.rs:10-45`, `components/footer_hints.rs:1-62` | Reuse brand/header and active-hint owners; extend them only through shared contracts. |
| `crates/jackin/src/console/adapter.rs`, `effects.rs`, `services.rs` | Put host broker I/O and subscriptions behind the existing adapter/effect boundary. |
| `crates/jackin-console/src/tui/view/tests.rs` and screen-local snapshots | Add 80×24 plus wide/narrow state fixtures for the confirmed console screen contract. |

### Capsule TUI and runtime relay

Capsule already has a read-only Usage modal. `Dialog::Usage` stores a `FocusedUsageView` plus `UsageDialogTab`; `dialog/usage.rs` owns Overview/provider state, tab targeting, meter selection, and constructors; `dialog_widgets/usage.rs` owns geometry and line composition. The compositor consumes the focused usage snapshot for persistent chrome. These are the closest shipped visual grammar and must be adapted to the new resolved-agent/account contract, not replaced by a second modal. — `crates/jackin-capsule/src/tui/components/dialog.rs:147-223`, `crates/jackin-capsule/src/tui/components/dialog/usage.rs:9-55`, `crates/jackin-capsule/src/tui/components/dialog/usage.rs:101-235`, `crates/jackin-capsule/src/tui/components/dialog_widgets/usage.rs:18-188`, `crates/jackin-capsule/src/tui/components/dialog_widgets/usage.rs:286-430`, `crates/jackin-capsule/src/tui/daemon/compositor.rs:138-140` (confidence: HIGH)

The runtime relay is the security boundary between a Capsule and the host-only broker. `UsageRelayLaunch`, `PreparedUsageRelay`, forwarded-source validation, Docker/Apple tunnel launch, `start`, `run_listener`, `handle_connection`, and the operation allowlist already isolate the container socket at `usage.sock`. Planning must change its projection/membership contract and operation allowlist in lockstep with the protocol; it must not forward arbitrary broker operations, secrets, or host inventory into a Capsule. — `crates/jackin-runtime/src/usage_relay.rs:35-52`, `crates/jackin-runtime/src/usage_relay.rs:125-215`, `crates/jackin-runtime/src/usage_relay.rs:281-390`, `crates/jackin-runtime/src/usage_relay.rs:422-528` (confidence: HIGH)

Plan-relevant tests are the existing dialog snapshots and interaction tests under `crates/jackin-capsule/src/tui/components/dialog/tests.rs`, the usage-widget tests/snapshots beside `dialog_widgets/usage.rs`, and relay allowlist/tunnel tests in `crates/jackin-runtime/src/usage_relay/tests.rs`. The roadmap's Overview, account-only multi-account tabs, focused-entry selection, lifecycle coexistence, stale/error, and refresh-join states need explicit additions there. (confidence: HIGH)

### Rust FFI projection and generated Swift boundary

`UsageMenuBarBridge` is the native entry point. It already opens the runtime, lists surfaces, refreshes, projects inventory, returns an atomic desktop projection, and sets account selection. `DesktopProjectionDto` is described as one immutable native generation, but it has a generation field rather than the roadmap's versioned canonical cross-surface envelope. Planning must add the semantic fields and stable destinations in Rust DTOs/projection first, regenerate bindings, then simplify Swift adapters. Generated BoltFFI output is never edited directly. — `crates/jackin-usage-ffi/src/bridge.rs:29-142`, `crates/jackin-usage-ffi/src/bridge.rs:208-284`, `crates/jackin-usage-ffi/src/dto.rs:251-390`, `native/Sources/JackinUsageBindings/BoltFFI/JackinUsageFfiBoltFFI.swift:1012-1048`, `mise.toml:112-119` (confidence: HIGH)

The bridge tests at `crates/jackin-usage-ffi/src/bridge/tests.rs:135-152` already assert joined refresh generations, while `crates/jackin-usage-ffi/src/bridge/tests.rs:515-519` asserts the atomic provider/glance projection. Those are direct extension points for version, ordering, account identity, partial-provider state, and no-duplicate fixtures. (confidence: HIGH)

### Production macOS app, popover, and Settings

Swift production already has the right shell ownership. `PresentationStore` receives immutable projections and owns navigation/persisted presentation preferences; `RefreshScheduler` submits intent while Rust owns coalescing; `UsageWindowModel` adapts projection rows; `OverviewInventory.tree` constructs current Swift navigation rows. Current navigation is still provider-oriented in `UsageWindowSidebar`, so canonical account destinations and removal normalization belong in models before views. — `native/Sources/JackinUsageBridge/PresentationStore.swift:18-319`, `native/Sources/JackinUsageBridge/PresentationStore.swift:774-880`, `native/Sources/JackinUsageBridge/RefreshScheduler.swift:24-96`, `native/Sources/JackinUsageBridge/UsageWindowModel.swift:8-175`, `native/Sources/JackinUsageBridge/OverviewInventory.swift:8-81`, `native/Sources/JackinDesktop/UsageWindow/UsageWindowRoot.swift:9-85` (confidence: HIGH)

`UsageWindowController` is the single production window-metrics/titlebar owner and already freezes default 1000×680 and minimum 800×520 plus centered titlebar identity. `UsageWindowSplitController` and `UsageWindowToolbar` own the AppKit split/sidebar toggle/standard toolbar Refresh. `OverviewListView` and `ProviderDetailView` own authored content. Planning must preserve those AppKit owners, apply the blessed dark-only content/layout, and avoid simulated glass or duplicated titles. — `native/Sources/JackinDesktop/UsageWindowController.swift:8-10`, `native/Sources/JackinDesktop/UsageWindowController.swift:23-66`, `native/Sources/JackinDesktop/UsageWindowController.swift:115-203`, `native/Sources/JackinDesktop/UsageWindow/UsageWindowSplitController.swift:10-76`, `native/Sources/JackinDesktop/UsageWindow/UsageWindowSplitController.swift:76-141`, `native/Sources/JackinDesktop/UsageWindow/OverviewListView.swift:8-138`, `native/Sources/JackinDesktop/UsageWindow/ProviderDetailView.swift:10-188` (confidence: HIGH)

`StatusBarController` owns one real `NSPopover`, status items, clicked-button routing, and anchoring. `StatusPopoverFocus` maps the click to a provider focus; `PopoverRoot` owns the 380×520 glance content and official brand header. This is the exact seam for display-local/rightmost-item fixes and canonical-account focus; no alternate popup window should be introduced. Settings remains the existing app destination through `AppMainMenu` and `SettingsView`. — `native/Sources/JackinDesktop/DesktopAppDelegate.swift:16-75`, `native/Sources/JackinDesktop/DesktopAppDelegate.swift:192-284`, `native/Sources/JackinUsageBridge/StatusPopoverFocus.swift:10-49`, `native/Sources/JackinDesktop/PopoverRoot.swift:42-147`, `native/Sources/JackinDesktop/AppMainMenu.swift:14-32`, `native/Sources/JackinDesktop/AppMainMenu.swift:171-229`, `native/Sources/JackinDesktop/SettingsView.swift:8-166` (confidence: HIGH)

### Blessed prototype

The prototype is a reference, not production architecture. `PRODUCTION_MAPPING.md` explicitly assigns typed destination/account projection to Rust and `PresentationStore`, native window/split/toolbar/popover ownership to AppKit, content adaptation to Swift, and forbids copying fixture store, shell, scenario harness, or fixture DTOs. Its implementation order requires Rust/FFI semantics first, production presentation models second, shell behavior third, Usage content fourth, and atmosphere/accessibility substitutions last. — `native/Design/Prototypes/UnifiedAgentUsage/PRODUCTION_MAPPING.md:1-47` (confidence: HIGH)

The signoff records build/test commands and passed F00–F29 human review at 800×520, 1000×680, and 1200×760. `DESIGN_AUDIT.md` records the dark-only contract and canonical captures. Planning should cite `SIGNOFF.md` and `PRODUCTION_MAPPING.md` for acceptance, then implement in production files above; it must never treat `ProtoStore` or `ReferenceModels` as production DTOs. — `native/Design/Prototypes/UnifiedAgentUsage/SIGNOFF.md:1-35`, `native/Design/Prototypes/UnifiedAgentUsage/DESIGN_AUDIT.md:243-266`, `native/Design/Prototypes/UnifiedAgentUsage/Sources/UnifiedAgentUsageProto/Domain/ProtoStore.swift`, `native/Design/Prototypes/UnifiedAgentUsage/Sources/UnifiedAgentUsageProto/Domain/ReferenceModels.swift` (confidence: HIGH)

## Exact verification command graph

The repository task definitions, not prose aliases, are authoritative. Run through `rtk` per repository operator rules.

| Purpose | Exact command | Proven owner / qualification |
|---|---|---|
| Rust formatting | `rtk mise run fmt` | `mise.toml:99-101`; nonmutating. |
| Unified Rust tests | `rtk mise run test` | `mise.toml:91-93`; invokes fast `cargo xtask ci --only tests`. |
| Unified Rust lint | `rtk mise run lint` | `mise.toml:95-97`; invokes fast lint gate. |
| Focused crate tests during iteration | `rtk cargo nextest run -p jackin-usage -p jackin-usage-ffi -p jackin-runtime -p jackin-capsule -p jackin-console -p jackin` | Repository uses nextest in CI; package set corresponds to all changed usage surfaces. Final proof still runs unified tasks. |
| Generated binding drift | `rtk mise run desktop-bindings-check` | `mise.toml:116-119`; never edit generated Swift manually. |
| Regenerate native project | `rtk mise run desktop-generate` | `mise.toml:124-126`; writes generated Xcode project as intended. |
| Swift formatting check | `rtk mise run desktop-format-check` | `mise.toml:134-136`. |
| SwiftLint | `rtk mise run desktop-lint` | `mise.toml:138-142`. |
| Native Rust/FFI/parity tests | `rtk mise run desktop-test` | `mise.toml:177-180`. |
| Native PR graph | `rtk mise run desktop-ci` | `mise.toml:183-196`; bindings, project, formatting, lint, tests, build, Swift tests, app verification. |
| Native merge graph | `rtk mise run desktop-merge` | `mise.toml:197-203`; adds real-host UI tests. |
| Native scheduled graph | `rtk mise run desktop-scheduled` | `mise.toml:205-211`; adds dead-code scan. |
| Prototype build | `rtk mise run desktop-prototype-build` | `mise.toml:227-239`. |
| Prototype tests | `rtk swift test --package-path native/Design/Prototypes/UnifiedAgentUsage` | `native/Design/Prototypes/UnifiedAgentUsage/SIGNOFF.md:11-15`. |
| Prototype scenario | `rtk mise run desktop-prototype -- F02 1000x680 dark` | `mise.toml:267-284`; supports F00–F29 and fixed size/appearance arguments. |
| Production deterministic captures | `rtk native/Scripts/VisualQA/capture-final-matrix.sh native/dist/JackinDesktop.app` | `native/README.md:137-146`, `native/Scripts/VisualQA/capture-final-matrix.sh:1-12`; requires macOS WindowServer and Screen Recording permission, and temporarily changes app/system presentation state. |
| Local app build | `rtk mise run desktop-build -- 0.6.0 1` | `mise.toml:152-159`; produces app/dSYM using fixture version/build. |
| Local fail-closed verify | `rtk mise run desktop-verify -- native/dist/JackinDesktop.app 0.6.0 1` | `mise.toml:160-175`; secret-free for ad-hoc validation. |
| Release-mode verify | `rtk mise run desktop-verify -- native/dist/JackinDesktop.app 0.6.0 1 --release` | `mise.toml:160-175`; only succeeds for Developer ID signed, notarized, stapled, Gatekeeper-accepted app. |
| Sign/notarize/staple | `rtk mise run desktop-sign-notarize -- native/dist/JackinDesktop.app <out-zip> <version> <build>` | `mise.toml:300-315`; credential-dependent; secret values never enter plans/docs. |
| Read-only release/cask reconciliation | `rtk mise run desktop-release-state -- <version> --repo jackin-project/jackin --tap jackin-project/homebrew-tap` | `mise.toml:317-324`, `.github/workflows/release.yml:500-508`; network/auth may be required, no publication write. |
| Credential bootstrap | `rtk mise run desktop-bootstrap-secrets` | `mise.toml:326-328`; external operator authorization and GitHub/Apple credential material required. |

The desktop capture script currently enumerates light fixtures despite the settled dark-only product contract (`native/Scripts/VisualQA/capture-final-matrix.sh:123-157`). Implementation planning must update the canonical production matrix so dark-only proof cannot silently pass via obsolete light cases, while retaining accessibility contrast/transparency/motion evidence. This is a concrete verification-gap fix, not permission to restore light mode. (confidence: HIGH)

## Signing, notarization, and publication boundary

Secret-free CI can build, ad-hoc verify, prove release verification rejects the ad-hoc artifact, run offline reconciliation fixtures, and repeat read-only release state. Credentialed publication is restricted to the `release-macos` environment on GitHub-hosted macOS; it imports Developer ID and App Store Connect material, validates certificate/team identity, signs/notarizes/staples, removes temporary credential material, and only then creates checksum, Sigstore bundle, SBOM, provenance attestation, release archive, and symbol archives. — `.github/workflows/release.yml:400-432`, `.github/workflows/release.yml:470-508`, `.github/workflows/release.yml:510-639` (confidence: HIGH)

Plans may name only these required secret types/locations: the five `release-macos` secret names and the two repository variable names recorded in `native/README.md:166-181`. They must not contain values. Developer ID signing/notarization, first public artifact publication, tap PR creation/merge, and clean-machine Homebrew cask installation remain credential/operator-dependent acceptance work; local ad-hoc success cannot satisfy them. `desktop-release-state` is the read-only preflight/reconciliation seam and release CI is the publication owner. (confidence: HIGH)

## Planning conclusions

1. Build the canonical projection and broker contract before any surface-specific rendering. All surfaces consume the same Rust-ranked providers, canonical accounts, windows, lifecycle, freshness, issues, and strings.
2. Implement host CLI at `crates/jackin/src/cli/usage.rs`; implement Console as a new typed route/screen with effects; adapt the existing Capsule modal and allowlisted relay; extend Rust FFI DTOs before generated Swift and production presentation models.
3. Preserve AppKit ownership of status items, `NSPopover`, Usage window, split view, toolbar, centered identity, and Settings. Swift adapts layout and interaction only.
4. Treat the blessed prototype as visual/interaction acceptance evidence. Never copy its fixtures, store, or reference DTOs into production.
5. Each implementation slice must name focused tests plus the final unified/native gates it enables. Cross-surface parity, terminal snapshots, FFI fixture parity, real-host UI tests, deterministic captures, release verification, and clean-machine cask proof are separate gates.
6. Remove obsolete light capture cases from the production verification matrix while preserving dark accessibility variants.
7. Execute every slice, correction, verification update, and release-readiness document change on `chore/roadmap-unified-agent-usage` in PR #898. No new branch or PR is permitted.

## Remaining uncertainties for planning

- The Console Usage screen has no existing module, so the plan must freeze its new message/effect ownership and broker subscription lifecycle rather than cite a nonexistent usage implementation.
- Credentialed signing, notarization, publication, tap mutation, and clean-machine install cannot be proven without the named external credentials and operator/repository authority. Plans must make those explicit acceptance inputs, never silently downgrade them to local ad-hoc proof.
- The production visual capture script's current light cases conflict with the settled dark-only contract and must be changed before it can serve as final visual evidence.
