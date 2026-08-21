# Swift Project Readiness — Unified Agent Usage

Status: REMEDIATED — 2026-08-20 (two standing exceptions below)

Audit date: 2026-08-20

Mode: the original audit below is preserved as the historical record. The
remediation ledger records the never-broken slices that closed every approved
gap under `tailrocks-swift-project-setup` remediate mode.

## Remediation ledger

Remediation ran 2026-08-20 on branch `chore/roadmap-unified-agent-usage`
(PR #898), in the doc's own remediation order. Every slice kept format, lint,
Rust, and Swift gates green before commit.

| Gap | Closed by | Evidence |
|---|---|---|
| Two-lane toolchain contract (P0) | `e7f66979` | All four values in `native/AGENTS.md` + `native/project.yml` comments; `PlatformLaneTests` reject `UIDesignRequiresCompatibility` and unguarded post-26.0 symbols. |
| Xcode agent bridge setup/boundary (P0) | `e7f66979` | `native/README.md` "Xcode agent bridge": setup, no-screenshot/no-automation boundary, unavailable-never-pass rule, no-secret verification checklist. |
| Agent responsibility ownership (P1) | `e7f66979` | One-owner table in `native/README.md` "Agent responsibility ownership". |
| One-way bridge package boundary (P0) | `3ea08dc2` | `JackinUsageBindings` (generated only) → `JackinUsageBridge` (sole handwritten importer, typed facade — `RefreshScheduler.run` is private) → UI targets; `BridgeBoundaryTests` enforce the import/handle rules. |
| Generated-binding drift gate (P0) | `09934ad7` | `cargo xtask desktop bindings-check` / `mise run desktop-bindings-check`: staging regeneration, byte-compare of both trees, stale/missing/extra fixtures unit-tested. |
| Swift unit-test count proof + all five harnesses (P0) | `508fe9b4` | `cargo xtask desktop test-swift`: dual proof — XCTest `All tests` summary (SwiftPM 6.3 writes xUnit for Swift Testing only) plus `-swift-testing.xml`; zero/missing/corrupt results fail. All five declared harness products run. |
| Local/CI/release parity (P0) | `c449025f` | Cadence graph `desktop-ci` (PR) → `desktop-merge` (+UI) → `desktop-scheduled` (+dead-code); release.yml invokes the exact `mise run desktop-*` names; xtask contract tests prove the graph, the release wiring, and that generated `ci.yml` only delegates the native lane; TESTING.md contradiction reconciled. |
| Release symbols (P1) | `dc32dc3f` | `[profile.desktop-release]` (thin LTO, 1 codegen unit, line tables, no strip) drives the static library/bindings/XCFramework; build archives the dSYM beside the app and proves correspondence via matching arm64 UUIDs; release CI uploads dSYM + compressed unstripped `.a` (90-day retention). dSYM verified to carry Rust function names and source lines. |
| SwiftLint debt + unit-test policy (P1) | `fd04d8c4` | Root disables only the four verified swift-format conflicts; six size rules re-enabled at defaults with legacy overages in nested per-directory configs (SwiftLint has no per-rule path exclusion; `--config` would silently disable nested discovery, so `desktop-lint` drops it) carrying owner + deletion condition; `native/Tests/.swiftlint.yml` added; `LintPolicyTests` prove force rules stay error outside test trees. |
| Apple agent knowledge governance (P1) | `f8324d6` + `105b73a` | Standing blocker: Xcode 26.6 (17F113) ships no exportable skill documents — nothing reviewable to vendor. Recorded with probe date, refresh rule, and non-execution policy in `native/README.md`; `VendorProvenanceTests` require PROVENANCE.md if a vendor tree ever appears. |

Standing exceptions (both dated, both nonblocking):

1. **Forward-validation lane absent.** No Xcode 27 runner image exists; the
   exception is owned by Release Engineering in `native/README.md` and exits
   when the runner image is available. Shipping lane unchanged.
2. **Apple agent skills export unsupported.** Recorded blocker above; re-probe
   on every shipping Xcode change.

Scheduled-cadence note: the readiness implementation listed an accessibility
audit and a visual-state matrix under merge/scheduled cadences. The
accessibility audit runs inside `desktop-test-ui` (merge cadence); no separate
visual-state-matrix task exists yet — adding one is a follow-up, not a
regression.

## Original audit (2026-08-20, preserved)

## Proven baseline

| Area | Result | Repository evidence |
|---|---|---|
| Declarative app project | Pass | `native/project.yml:1-153` defines the application, library, unit-test, and UI-test targets. The generated project is ignored. |
| Synchronized sources | Pass | Every source-bearing target uses `type: syncedFolder` at `native/project.yml:30-32`, `46-48`, `63-65`, `105-107`, and `120-122`. |
| Deployment floor | Pass | macOS 26.0 is explicit in `native/project.yml:5-6,20`, the app property list at `81`, and `native/Package.swift:8-10`. |
| Language and concurrency | Pass | Swift 6, complete strict concurrency, and warnings-as-errors are explicit at `native/project.yml:16,22-24`. |
| Local signing | Pass | Ad hoc identity and disabled hardened runtime are explicit at `native/project.yml:88-91`; `cargo xtask desktop build` signs the copied app at `crates/jackin-xtask/src/desktop.rs:648-657`. |
| Derived data | Pass | Build and UI-test paths stay under `native/DerivedData`, never a temporary directory: `crates/jackin-xtask/src/desktop.rs:596-609` and `native/Scripts/run-ui-tests.sh:8,79`. |
| Format gate | Pass | The Xcode-bundled formatter runs with `--strict` at `mise.toml:128-130`; generated boltffi source is excluded. |
| Lint gate | Pass in configuration | SwiftLint is exactly pinned at `mise.toml:32` and invoked with `--strict` at `132-134`. It was unavailable on the audit host because this checkout's mise configuration is not trusted; the direct formatter remained available. |
| Unit and UI targets | Pass | XcodeGen owns populated unit and UI targets at `native/project.yml:102-127`; SwiftPM owns the importable test target at `native/Package.swift:66-70`. |
| UI zero-test defense | Pass | The driver enumerates nonzero test methods, uses exact XCTest selectors, reads each result bundle, and requires exactly one passed test per invocation at `native/Scripts/run-ui-tests.sh:10,53-65,73-80,104-140`. |
| Accessibility audit wiring | Pass with code-review gaps | Running-host audits exist at `native/UITests/JackinDesktopUITests.swift:425-519`; exception quality is evaluated in `SwiftBestPracticesReview.md`. |
| Pinned bridge | Pass | boltffi crate and CLI are both exactly `0.30.1`: `crates/jackin-usage-ffi/Cargo.toml` and `mise.toml`. |
| Ordered Rust packaging | Pass | `cargo xtask desktop build` builds the XCFramework before XcodeGen and Xcode: `crates/jackin-xtask/src/desktop.rs:569-614`. |
| Architecture decision | Pass as a recorded product decision | The app, XCFramework, verifier, artifact name, and README consistently specify Apple Silicon only: `native/project.yml:13`, `native/README.md:26,132,141-145`, and `crates/jackin-xtask/src/desktop.rs:636-643`. Universal packaging is intentionally outside the current product contract. |

Audit-host observations:

- Xcode 26.6 build `17F113`.
- Apple Swift 6.3.3 targeting `arm64-apple-macosx26.0`.
- Xcode-bundled `swift-format` 6.3.0.
- XcodeGen 2.46.0.
- SwiftLint, xcbeautify, and Periphery were not on the unmanaged host `PATH`.

## Audit execution ledger

Revision: `b0c2abbd58b7177c6bc9942116af50dfbff3fda7`. Commands ran on
2026-08-20 from repository root unless a working directory is named. Generated
build products remain under `target/`, `native/DerivedData`, `native/dist`, or
ignored project output; regeneration produced no tracked binding diff.

| Exact command | Result | Evidence boundary |
|---|---|---|
| `xcodebuild -version` | PASS — Xcode 26.6, build `17F113` | Shipping compiler host only; no forward lane. |
| `swift --version` | PASS — Apple Swift 6.3.3, arm64 macOS 26 target | Host toolchain only. |
| `xcrun swift-format --version` | PASS — 6.3.0 | Formatter availability. |
| `xcodegen --help` | PASS — XcodeGen 2.46.0 was installed and accepted the current CLI | Availability only. The build command below performed generation. |
| `find Sources Tests UITests Tools Scripts -name '*.swift' ! -name 'jackin_usage_ffi.swift' -print0 \| xargs -0 xcrun swift-format lint --configuration .swift-format --strict --parallel` from `native/` | PASS — exit 0, no findings | Strict handwritten formatting; generated boltffi excluded deliberately. |
| `rtk cargo xtask desktop test` | PASS | Rust/FFI nextest plus `StatusItemChipHarness`, `DesktopArchitectureLint`, and `DesktopParityMatrixHarness`. It confirms the audit defect: two other declared harness products were not run. |
| `rtk swift test -c release` from `native/` | PASS — 65 XCTest plus 2 Swift Testing tests, zero failures | Current Swift package tests; runner does not machine-enforce the count. |
| `rtk cargo xtask desktop build --version 0.6.0 --build 1` | PASS — XCFramework, boltffi generation, XcodeGen, arm64 Release build, ad hoc sign, app assembly | Mutating build pipeline passed. It is not a nonmutating binding-drift gate. |
| `rtk git diff -- native/Generated native/Sources/JackinUsageBridge/jackin_usage_ffi.swift` after build | PASS — empty | Current regeneration matched tracked bindings. This manual observation does not replace CI drift enforcement. |
| `rtk cargo xtask desktop verify native/dist/JackinDesktop.app` | PASS — ad hoc/PR verification | Local artifact shape only; no Developer ID/notary/public-download proof. |
| `rtk mise run desktop-test-ui` | UNAVAILABLE — mise required trusting repository config; trust was declined because it writes host state | Canonical wrapper did not start. |
| `rtk proxy xcodebuild test -project native/JackinDesktop.xcodeproj -scheme JackinDesktop -destination 'platform=macOS' -parallel-testing-enabled NO -only-testing:JackinDesktopUITests/JackinDesktopUITests/testOverviewPassesAccessibilityAudit -derivedDataPath native/DerivedData -resultBundlePath native/.build/test-results/goal-accessibility-overview.xcresult` | FAIL — one test executed; activation failed after 61.977 s because app state remained `Running Background` | The accessibility audit body did not execute. This is stronger failure evidence than a zero-test timeout, not an accessibility pass. |
| `command -v swiftlint xcbeautify periphery` | UNAVAILABLE — all absent | Strict lint, canonical UI-test reporting, and dead-code scan could not run unmanaged. |
| `rtk cargo xtask desktop bindings --help` | INCOMPLETE — only mutating `bindings` exists | No nonmutating drift-check command exists. |

Context7 tools were also absent from the live tool registry; Apple API research
used official primary sources and records that exception in
`research/agent-usage-platform/02-apple-native-design.md`.

The skill-supplied comparison baseline, not a value recorded by this project, is
shipping Xcode 26.6/macOS 26.5 SDK/Swift 6.3 and a nonblocking Xcode 27 beta
forward-validation lane. The installed audit host matches those shipping values.
The project records Xcode 26.6, macOS 26.0 deployment, and arm64 at
`native/README.md:139-145`; it does not record the shipping SDK or compiler
release, and the forward lane is absent.

## Required remediation

### P0 — record the complete two-lane toolchain contract

Current condition:

- The project records the deployment floor and shipping Xcode in the README,
  but `native/AGENTS.md` does not state all four required values.
- `native/project.yml` has no shipping/forward lane comments.
- No scheduled nonblocking Xcode 27/macOS 27 build is visible in repository
  configuration.

Implementation:

1. Add one compact platform block to `native/AGENTS.md` and matching comments to
   `native/project.yml`:
   - minimum deployment target: macOS 26.0;
   - shipping lane: Xcode 26.6, macOS 26.5 SDK, Swift 6 mode with complete strict concurrency;
   - forward-validation lane: Xcode 27 beta/macOS 27 SDK, nonblocking and scheduled;
   - unavailable forward API behavior: guard every post-26.0 symbol, ship a
     decided native fallback, and name the minimum-target bump that removes it.
2. Preserve the repository rule that `.github/workflows/ci.yml` is generated.
   Add the forward lane at the owning `velnor-actions` native-workflow source,
   regenerate the consumer, and do not hand-edit the generated workflow.
3. Until that runner lane exists, record a dated exception in `native/README.md`
   owned by Release Engineering: shipping remains Xcode 26.6; forward failures
   do not gate release; the exception exits when the Xcode 27 runner image is
   available.
4. Add an architecture test that rejects `UIDesignRequiresCompatibility` and
   rejects an unguarded post-26.0 API listed in the component map.

Acceptance:

- All four values are committed in agent instructions and the manifest.
- The scheduled lane invokes a repository-owned task and is explicitly
  nonblocking.
- No beta toolchain becomes the shipping lane.

### P0 — make local and CI commands identical and complete

Current condition:

- `mise.toml` is the version authority and exposes strong desktop tasks.
- `desktop-ci` regenerates bindings, formats, lints, runs Rust/harness tests, and
  runs SwiftPM tests (`mise.toml:163-173`). It does not generate the Xcode project,
  build the app, verify the app, run XCUITests, or assert a Swift unit-test count.
- `TESTING.md:167` calls the native lane a PR gate while `TESTING.md:184-187`
  says generated PR CI has no native Swift lane. The source of truth contradicts
  itself and therefore cannot prove coverage.
- `DesktopSoTParityHarness` and `ProviderMarksHarness` are declared products at
  `native/Package.swift:17-18`, but `cargo xtask desktop test` runs only three
  other harnesses at `crates/jackin-xtask/src/desktop.rs:178-190`.
- Release CI restates `cargo xtask desktop build/verify` directly at
  `.github/workflows/release.yml:469-479` instead of invoking the same mise tasks.
- The reusable PR workflow is external, so this checkout alone cannot prove
  which local task names it runs.

Implementation:

1. Split canonical tasks by cadence while keeping one definition per command:
   - PR: Rust format/clippy/tests → refactored bridge/XCFramework pack into
     staging → nonmutating `desktop-bindings-check` → `desktop-generate` →
     `desktop-format-check` → `desktop-lint` → app build → counted Swift unit
     tests → app verify.
   - Merge: PR gates plus `desktop-test-ui` and accessibility audit.
   - Scheduled: merge gates plus `desktop-deadcode`, forward SDK build, and the
     visual-state matrix.
2. Make CI and release invoke these exact `mise run desktop-*` task names. Any
   workflow orchestration remains in the governing Rust xtask or generated
   workflow source, not new shell logic.
3. Replace the bare `swift test -c release` step with an xtask-owned test driver
   that parses machine-readable results and rejects zero, missing, corrupt, or
   partial test results. Keep the UI driver's existing per-selector count proof.
4. Add a project-baseline test that reads the generated CI contract or its
   checked-in generation metadata and proves the expected mise task sequence.
5. Run all five declared harnesses, or move their assertions into counted unit
   tests and remove the unused executable products. Reconcile the contradictory
   native-lane statements in `TESTING.md` in the same change.

Acceptance:

- A local task and CI execute the same command graph.
- Deliberately mistyping a Swift unit-test selector fails because executed count
  is zero, even when the underlying tool exits successfully.
- Release assembly calls the same build and verify task definitions as local use.

### P0 — connect and bound the Xcode agent bridge

Current condition:

- Repository instructions do not explain how to enable Xcode external-agent
  access, which project must be open, or how to verify the commands exposed by
  the pinned shipping Xcode.
- No repository text records that the bridge cannot capture screenshots or drive
  interface automation. That missing boundary can produce a false visual-QA
  claim.

Implementation:

1. Add a short, shipping-Xcode-specific setup section to `native/README.md`:
   enable external agents in Xcode Intelligence settings, generate and open
   `native/JackinDesktop.xcodeproj` in a running Xcode 26.6, then enumerate the
   bridge's actual tools before depending on a command name.
2. Record that the Xcode bridge supplies project context/build/test/preview
   operations only. It supplies neither running-app screenshots nor UI
   automation; `native/Scripts/VisualQA` and XCUITest remain the owners of those
   capabilities.
3. Add a checked-in, no-secret verification checklist with expected project,
   scheme, Xcode build, and observed tool list. Re-probe it whenever the shipping
   Xcode pin changes.
4. Keep this manual integration separate from CI. Xcode must be running with the
   project open, so absence on a headless worker is unavailable, never pass.

Acceptance:

- A fresh operator can enable the bridge, open the generated project, enumerate
  its current tools, and run one build/test operation from an external agent.
- Documentation cannot imply that a preview or bridge result is running-app
  visual evidence.

### P0 — add a generated-binding drift gate

Current condition:

- Generated Swift, header, and module maps are committed.
- `generate_bindings` overwrites them at
  `crates/jackin-xtask/src/desktop.rs:400-437`.
- No repository task regenerates and then fails on a dirty binding diff.

Implementation:

1. Add `cargo xtask desktop bindings-check`. Generate into an ignored staging
   directory under `native/.build`, normalize with the same function, and compare
   every committed output byte-for-byte.
2. Compare both `native/Generated/*` and the generated Swift copy under
   `native/Sources/JackinUsageBridge/`; reject missing, extra, or changed files.
3. Add `mise run desktop-bindings-check` before format/build in the PR task and
   test the checker with stale, missing, and extra fixture files.

Acceptance:

- A Rust boundary change without regenerated Swift fails CI.
- A no-change regeneration leaves the worktree clean.

### P0 — enforce the one-way bridge package boundary

Current condition:

- Only `jackin_usage_ffi.swift` imports the generated C module, which is good.
- Generated bindings, `RefreshScheduler`, `PresentationStore`, and presentation
  value types nevertheless share the single `JackinUsageBridge` module
  (`native/Package.swift:21-34`). The generated handle is therefore module-visible
  across handwritten presentation code, and `RefreshScheduler.run` exposes it to
  arbitrary closures.

Implementation:

1. Split the package/Xcode targets in one buildable change:
   `JackinUsageBindings` (generated only) → `JackinUsageBridge` (the sole
   handwritten importer/owner) → `JackinDesktopPresentation` or existing desktop
   UI target.
2. Replace public generic `RefreshScheduler.run` with a typed facade whose methods
   are the only places allowed to name `UsageMenuBarBridge`.
3. Add an architecture gate that permits `import jackin_usage_ffiFFI` only in
   generated source, permits `UsageMenuBarBridge` only in the handwritten facade,
   and rejects FFI symbols in views, fixtures, and presentation models.
4. Keep all provider selection, account deduplication, refresh policy, cache, and
   display strings in Rust.

Acceptance:

- Views and the presentation store cannot compile if they attempt a generated
  FFI call.
- One facade owns bridge creation, serialization, invalidation, and typed errors.

### P1 — preserve release symbols for the Rust static library

Current condition:

- The workspace release profile enables thin LTO but strips symbols and does not
  preserve debug information (`Cargo.toml:288-290`).
- The desktop release pipeline does not archive Rust symbols beside the app dSYM.

Implementation:

1. Add a dedicated `desktop-release` Cargo profile inheriting release with thin
   LTO, one codegen unit, line-table debug information, and symbol stripping
   disabled. Do not weaken the CLI/capsule release profile.
2. Use that profile for the desktop static library and boltffi generation.
3. Archive the Rust symbol artifact and Xcode dSYM in release CI; verify they
   correspond to the published app bytes.

Acceptance:

- A fixture Rust panic from the signed validation app symbolizes to Rust function
  names and source lines without changing production error containment.

### P1 — narrow SwiftLint debt and add the unit-test policy

Current condition:

- `.swiftlint.yml:65-86` disables complexity, file/function/type length,
  parameter count, and several layout rules repository-wide. The comments name
  broad categories, not narrow paths, owners, or removal conditions.
- `native/UITests/.swiftlint.yml` correctly confines force-operation exemptions
  to UI tests. `native/Tests/.swiftlint.yml` is absent, so unit tests do not have
  the documented test-only policy.

Implementation:

1. Restore maintainable global thresholds and remove formatter-owned duplicates
   only where the formatter truly conflicts.
2. If legacy files still exceed a rule, use the narrowest path-scoped exclusion
   with reason, owner, and deletion condition; never weaken application-wide force
   operation errors.
3. Add `native/Tests/.swiftlint.yml` inheriting the root config and applying only
   the reviewed test-tree force-operation exceptions.
4. Add configuration tests proving application source still fails force unwrap,
   cast, and try while both test trees follow their nested policy.

Acceptance:

- No complexity/size rule is silently disabled for new production files.
- Every remaining debt exception is attributable and shrink-only.

### P1 — vendor and govern agent knowledge

Current condition:

- `native/Vendor/AppleAgentSkills` is absent.
- `native/README.md` has no one-owner skill table or pin/provenance record.
- Project-local third-party agent skills are not installed, so there is no
  third-party dependency to approve in this audit.

Implementation:

1. Export the Apple agent knowledge supplied by shipping Xcode 26.6, review the
   complete export, and commit it read-only under `native/Vendor/AppleAgentSkills`.
2. Record Xcode build, export date, file hashes, unsupported-export caveat, and
   refresh rule. Refresh only with a reviewed shipping-Xcode change.
3. Add the responsibility table to `native/README.md`: framework correctness,
   material policy, visual direction, rendering/verification, project mechanics,
   and design tokens each have one owner. Explicit invocation remains required
   for overlapping aesthetic skills.
4. Add a hash/provenance test and make the vendor tree read-only by policy; never
   execute unreviewed bundled scripts or network steps.

Acceptance:

- Every installed skill is pinned or tied to an exact Xcode build.
- Exactly one owner exists per responsibility.

## False-green defenses

- Format trap: already defended by `--strict`; retain a gate test that injects a
  malformed fixture and expects nonzero status.
- Test-selection trap: UI tests defend exact executed count; extend the same
  result-count rule to Swift unit tests and any future `-only-testing` lane.
- Pipeline trap: every piped Xcode invocation must use `pipefail`, or avoid a
  pipeline and parse the raw result bundle as the current UI driver does.
- Visual trap: detached snapshots cannot approve Liquid Glass. Only running-app,
  window-ID captures count.

## Remediation order

1. Record platform lanes, Xcode-agent integration, and agent ownership without
   changing behavior.
2. Split the generated/handwritten bridge boundary with tests bracketing the move.
3. Establish the ordered gate graph: Rust format/clippy/tests, bridge/XCFramework
   pack, nonmutating binding drift, Xcode generation, strict Swift format/lint,
   app build, counted Swift unit tests, app verification, then UI/accessibility.
4. Make local, PR, merge, scheduled, and release tasks share that command graph.
5. Add symbol-preserving desktop packaging.
6. Add the nonblocking forward lane through the workflow generator owner.
7. Run the complete graph and record every pass or explicit unavailable result.

## Audit verdict

The current project is reproducible and buildable, but it does not yet satisfy
the full setup skill gate. Load-bearing gaps are forward-lane recording and
execution, CI/local command parity, Swift unit-test count proof, generated-binding
drift detection, strict generated/handwritten module separation, release symbols,
governed Apple agent knowledge, and Xcode-agent bridge setup/boundary verification.
