// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import JackinUsageBridge
import SwiftUI
import XCTest

/// Static architecture checks: Swift tree must not grow provider probe logic.
final class ArchitectureTests: XCTestCase {
    private var sourcesRoot: URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()  // Tests/JackinUsageBridgeTests
            .deletingLastPathComponent()  // Tests
            .deletingLastPathComponent()  // native
            .appendingPathComponent("Sources")
    }

    private func handwrittenSwiftFiles() throws -> [URL] {
        let enumerator = FileManager.default.enumerator(
            at: sourcesRoot,
            includingPropertiesForKeys: nil
        )
        var files: [URL] = []
        while let url = enumerator?.nextObject() as? URL {
            if url.pathExtension == "swift", !url.lastPathComponent.contains("jackin_usage_ffi") {
                files.append(url)
            }
        }
        XCTAssertFalse(files.isEmpty, "expected Swift sources under native/Sources")
        return files
    }

    func testSwiftSourcesHaveNoProviderProbeImports() throws {
        // Probe/API machinery tokens (substring match).
        let machinery = ["URLSession", "Process(", "SecItem"]
        // Non-jackin providers: whole-token only so `eventCursor` / comments about
        // cursors are not false positives for the Cursor product.
        let providers = ["Gemini", "Copilot"]
        let cursorAsProvider = try NSRegularExpression(pattern: #"\bCursor\b"#)
        for file in try handwrittenSwiftFiles() {
            let text = try String(contentsOf: file, encoding: .utf8)
            let full = NSRange(text.startIndex..., in: text)
            for token in machinery {
                XCTAssertFalse(
                    text.contains(token),
                    "\(file.lastPathComponent) must not contain probe/API token \(token)"
                )
            }
            for token in providers {
                XCTAssertFalse(
                    text.contains(token),
                    "\(file.lastPathComponent) must not contain non-jackin provider \(token)"
                )
            }
            XCTAssertEqual(
                cursorAsProvider.numberOfMatches(in: text, range: full),
                0,
                "\(file.lastPathComponent) must not mention Cursor provider"
            )
            XCTAssertFalse(
                text.contains("URL(string: \"http"),
                "\(file.lastPathComponent) must not perform HTTP probes"
            )
            XCTAssertFalse(
                text.contains("URL(string: \"https://api."),
                "\(file.lastPathComponent) must not perform HTTPS API probes"
            )
        }
    }

    func testLatestOnlySourcesHaveNoMacOSCompatibilityBranches() throws {
        for file in try handwrittenSwiftFiles() {
            let text = try String(contentsOf: file, encoding: .utf8)
            XCTAssertFalse(
                text.contains("#available(macOS"),
                "\(file.lastPathComponent) must not retain a pre-26 compatibility lane"
            )
        }
    }

    func testProductionHasNoCustomGlassEffects() throws {
        for file in try handwrittenSwiftFiles() {
            let text = try String(contentsOf: file, encoding: .utf8)
            XCTAssertFalse(
                text.contains("glassEffect") || text.contains("GlassEffectContainer"),
                "\(file.lastPathComponent) must let standard controls own Liquid Glass"
            )
        }
    }

    func testProductionHasNoHandPaintedSystemMaterial() throws {
        let regex = try NSRegularExpression(
            pattern:
                #"\.background\(\.(bar|material)\)|\.(ultraThin|thin|regular|thick|ultraThick)Material"#
        )
        for file in try handwrittenSwiftFiles() {
            let text = try String(contentsOf: file, encoding: .utf8)
            let range = NSRange(text.startIndex..., in: text)
            let hits = regex.numberOfMatches(in: text, range: range)
            XCTAssertEqual(hits, 0, "\(file.lastPathComponent) must not paint custom material")
            XCTAssertFalse(
                text.contains("NSVisualEffectView"),
                "\(file.lastPathComponent) must not imitate system material"
            )
        }
    }

    func testDesktopSourcesContainNoForbiddenUsagePresentation() throws {
        // N3/B4: independent display-literal ban (token prices, spend/usage trends,
        // histories, aggregate-spend/ranking chrome) across every handwritten source.
        let forbidden = [
            "$/token", "$/mtok", "cost of session", "spend over time", "usage trend",
            "token history", "spend history", "aggregate spend", "top model",
            "30-day token", "30-day spend",
        ]
        for file in try handwrittenSwiftFiles() {
            let text = try String(contentsOf: file, encoding: .utf8).lowercased()
            for token in forbidden {
                XCTAssertFalse(
                    text.contains(token),
                    "\(file.lastPathComponent) must not surface \(token) — limits-only (N3/B4)"
                )
            }
        }
    }

    func testNoSwiftPercentArithmeticOnDisplayStrings() throws {
        // Heuristic: handwritten UI must not invent percentages via string
        // interpolation of computed used/remaining math into Text(...).
        // Gauge uses Rust-provided remaining only; forbid Text("…\(…)%…").
        let regex = try NSRegularExpression(
            pattern: #"Text\s*\(\s*"[^"]*\\\([^)]*\)[^"]*%"#
        )
        for file in try handwrittenSwiftFiles() {
            let text = try String(contentsOf: file, encoding: .utf8)
            let range = NSRange(text.startIndex..., in: text)
            let hits = regex.numberOfMatches(in: text, range: range)
            XCTAssertEqual(
                hits,
                0,
                "\(file.lastPathComponent) must not interpolate computed % into Text("
            )
        }
    }

    func testSeverityTintUsesBrandAndSystemWarningColors() {
        XCTAssertEqual(severityTint("danger"), Color.red)
        XCTAssertEqual(severityTint("warn"), Color.orange)
        XCTAssertEqual(severityTint("normal"), Color.jackinPhosphor)
        XCTAssertNotEqual(severityTint("normal"), Color.accentColor)
    }

    // ProviderMarks live in JackinDesktopUI — covered by DesktopSoT / visual harness.

    func testJackinPhosphorTokensMatchBrandGuide() {
        XCTAssertEqual(JackinBrand.phosphorDarkSRGB.r, 0x5C / 255.0, accuracy: 0.0001)
        XCTAssertEqual(JackinBrand.phosphorDarkSRGB.g, 0xF0 / 255.0, accuracy: 0.0001)
        XCTAssertEqual(JackinBrand.phosphorDarkSRGB.b, 0x7A / 255.0, accuracy: 0.0001)
        XCTAssertEqual(JackinBrand.phosphorLightSRGB.r, 0x0B / 255.0, accuracy: 0.0001)
        XCTAssertEqual(JackinBrand.phosphorLightSRGB.g, 0x77 / 255.0, accuracy: 0.0001)
        XCTAssertEqual(JackinBrand.phosphorLightSRGB.b, 0x4E / 255.0, accuracy: 0.0001)
    }

    /// Overview is a native comparison table without custom meter geometry.
    func testOverviewHasNoOrphanOverviewLevelProgress() throws {
        let overview =
            sourcesRoot
            .appendingPathComponent("JackinDesktop/UsageWindow/OverviewListView.swift")
        let text = try String(contentsOf: overview, encoding: .utf8)
        XCTAssertFalse(
            text.contains("ProgressView"),
            "OV-11: Overview must not ship ProgressView as Overview-level chrome"
        )
        XCTAssertFalse(
            text.contains("LinearProgress"),
            "OV-11: no LinearProgress Overview-level chrome"
        )
        XCTAssertTrue(text.contains("Table("), "Overview must use native Table")
        XCTAssertFalse(text.contains("Capsule"), "Overview must not paint custom meters")
    }

    func testOverviewPrimaryValuesUseSystemForeground() throws {
        let overview =
            sourcesRoot
            .appendingPathComponent("JackinDesktop")
            .appendingPathComponent("UsageWindow/OverviewListView.swift")
        let text = try String(contentsOf: overview, encoding: .utf8)

        XCTAssertTrue(
            text.contains(
                "Text(row.planOrStatusLabel)\n"
                    + "                            .foregroundStyle(.primary)")
        )
        XCTAssertTrue(
            text.contains(
                "Text(row.resetLabel)\n"
                    + "                        .foregroundStyle(.primary)")
        )
        XCTAssertFalse(text.contains("Color("))
    }

    func testFinalCaptureMatrixBuildsCleanBranchHeadApp() throws {
        let visualQARoot =
            sourcesRoot
            .deletingLastPathComponent()
            .appendingPathComponent("Scripts/VisualQA")
        let script = visualQARoot.appendingPathComponent("capture-final-matrix.sh")
        let text = try String(contentsOf: script, encoding: .utf8)
        let capture = try String(
            contentsOf: visualQARoot.appendingPathComponent("capture.sh"),
            encoding: .utf8
        )
        let focusDrive = try String(
            contentsOf: visualQARoot.appendingPathComponent("focus-drive.swift"),
            encoding: .utf8
        )
        let windowResolver = try String(
            contentsOf: visualQARoot.appendingPathComponent("window-id.swift"),
            encoding: .utf8
        )

        XCTAssertTrue(text.contains("git -C \"$repo\" status --porcelain"))
        XCTAssertTrue(text.contains("mise -C \"$repo\" run desktop-build"))
        XCTAssertTrue(text.contains("mise -C \"$repo\" run desktop-verify"))
        XCTAssertTrue(text.contains("final evidence requires the canonical branch-head app"))
        XCTAssertTrue(text.contains("CAPTURE_INACTIVE_APP:-"))
        XCTAssertTrue(text.contains("unset CAPTURE_INACTIVE_APP"))
        XCTAssertTrue(text.contains("capture_with_relaunch"))
        XCTAssertTrue(text.contains("capture retries exhausted after $attempt launches"))
        XCTAssertTrue(text.contains("--fixture \"$fixture\" --ui-test --open-usage"))
        XCTAssertTrue(text.contains("--fixture \"$fixture\" --ui-test --open-popover"))
        XCTAssertTrue(text.contains("FOCUS_DRIVE_TOOL=$focus_tool"))
        XCTAssertTrue(text.contains("\"WINDOW_LAYER_MODE=all\""))
        XCTAssertTrue(text.contains("native/.build/visual-qa/final"))
        XCTAssertTrue(text.contains("\"$output/$file\" \"jackin❯ desktop\""))
        XCTAssertTrue(capture.contains("WINDOW_LAYER_MODE=transient"))
        XCTAssertTrue(capture.contains("fullyContainedOnScreen"))
        XCTAssertTrue(capture.contains("--pid \"$pid\""))
        XCTAssertTrue(windowResolver.contains("let transientOnly = layerMode == \"transient\""))
        XCTAssertTrue(windowResolver.contains("isFullyContainedOnScreen"))
        XCTAssertTrue(windowResolver.contains("pid: requestedPID"))
        XCTAssertTrue(capture.contains("\"$FOCUS_TOOL\" \"$pid\" 0"))
        XCTAssertTrue(capture.contains("application exited before reaching requested"))
        XCTAssertTrue(capture.contains("CAPTURE_SETTLE_DELAY_SECONDS:-3"))
        XCTAssertTrue(capture.contains("prepare_swift_tool"))
        XCTAssertTrue(capture.contains("[ \"$source\" -nt \"$tool\" ]"))
        XCTAssertTrue(capture.contains("window_id_override=${WINDOW_ID_TOOL:-}"))
        XCTAssertTrue(capture.contains("perform action \"AXPress\" of targetButton"))
        XCTAssertFalse(capture.contains("to click first button of toolbar"))
        XCTAssertTrue(text.contains("CAPTURE_TOOLBAR_BUTTON_POST_DESCRIPTION=Show Sidebar"))
        XCTAssertTrue(capture.contains("toolbar_state_matches"))
        XCTAssertTrue(focusDrive.contains("activate(options: [.activateAllWindows])"))
        XCTAssertTrue(windowResolver.contains("if filter != nil"))
    }

    func testFixtureUsageWindowSurvivesForeignFullScreenHosts() throws {
        let controller =
            sourcesRoot
            .appendingPathComponent("JackinDesktop")
            .appendingPathComponent("UsageWindowController.swift")
        let text = try String(contentsOf: controller, encoding: .utf8)

        XCTAssertTrue(text.contains(".canJoinAllSpaces"))
        XCTAssertTrue(text.contains(".canJoinAllApplications"))
        XCTAssertTrue(text.contains(".fullScreenAuxiliary"))
        XCTAssertTrue(text.contains("if elevatesFixtureWindow"))
        XCTAssertTrue(text.contains("window.level = .floating"))
        XCTAssertTrue(text.contains("window.makeKeyAndOrderFront(nil)"))
        XCTAssertTrue(text.contains("window.orderFrontRegardless()"))
        XCTAssertTrue(text.contains("window.collectionBehavior.insert(.moveToActiveSpace)"))

        let delegate =
            sourcesRoot
            .appendingPathComponent("JackinDesktop")
            .appendingPathComponent("DesktopAppDelegate.swift")
        let delegateText = try String(contentsOf: delegate, encoding: .utf8)
        XCTAssertFalse(
            delegateText.contains(
                "if visualQALaunchOptions.elevatesFixtureWindow {\n            return .accessory"
            )
        )
        XCTAssertTrue(
            delegateText.contains(
                "if visualQALaunchOptions.openUsage || visualQALaunchOptions.openPopover"
            )
        )

        let options =
            sourcesRoot
            .appendingPathComponent("JackinDesktop")
            .appendingPathComponent("VisualQALaunchOptions.swift")
        let optionsText = try String(contentsOf: options, encoding: .utf8)
        XCTAssertTrue(
            optionsText.contains(
                "elevatesFixtureWindow: rawFixture != nil && arguments.contains(\"--ui-test\")"
            )
        )
    }

    /// SB-5 vs FB1-6: bar stays template mono (no severity tint).
    ///
    /// Urgency color
    /// on chip chrome is SB-P4 OPEN — not silently met as full SB-5.
    func testStatusBarIsTemplateMonoWithoutSeverityTint() throws {
        let label =
            sourcesRoot
            .appendingPathComponent("JackinDesktop/StatusItemLabel.swift")
        let text = try String(contentsOf: label, encoding: .utf8)
        XCTAssertTrue(
            text.contains("isTemplate = true") || text.contains("isTemplate=true"),
            "status icons must be template mono (FB1-6)"
        )
        XCTAssertTrue(
            text.contains("no severity tint") || text.contains("FB1-6"),
            "StatusItemLabel must document FB1-6 / no severity tint on bar"
        )
    }

    /// SB-13: ranked id order change forces status-item rebuild.
    func testStatusBarOrderRequiresRebuildOnRankChange() {
        XCTAssertFalse(
            statusBarOrderRequiresRebuild(
                previous: ["codex", "claude", "amp"],
                next: ["codex", "claude", "amp"]
            )
        )
        XCTAssertTrue(
            statusBarOrderRequiresRebuild(
                previous: ["codex", "claude", "amp"],
                next: ["claude", "codex", "amp"]
            ),
            "SB-13: swap of rank 1/2 must rebuild visual bar order"
        )
        XCTAssertTrue(
            statusBarOrderRequiresRebuild(
                previous: ["codex", "claude"],
                next: ["codex", "claude", "amp"]
            )
        )
    }

    /// SB-3/19 fixture filter for status-bar membership (QI / defensive path).
    func testSelectStatusBarGlanceRowsHidesZeroAndCapsThree() {
        func row(id: String, pct: UInt8?) -> PresentationStore.GlanceProviderRow {
            PresentationStore.GlanceProviderRow(
                surfaceId: id,
                iconKey: id,
                fallbackGlyph: "?",
                usageURL: nil,
                displayLabel: id,
                accountLabel: "",
                planLabel: nil,
                glanceRemainingPercent: pct,
                barLabel: pct.map { "\($0)%" } ?? "–",
                headline: pct.map { "\($0)% left" } ?? "–",
                resetLabel: nil,
                compactResetLabel: nil,
                exactReset: nil,
                statusWord: "fresh",
                isRefreshing: false,
                statusLabel: "fresh",
                severity: "normal",
                updatedLabel: "now",
                activityLabel: "Updated now",
                activityKind: "idle",
                accessibilityLabel: id,
                lastError: nil,
                dimmed: false
            )
        }
        let inventory = [
            row(id: "claude", pct: 12),
            row(id: "codex", pct: 0),
            row(id: "amp", pct: 100),
            row(id: "grok", pct: 72),
            row(id: "kimi", pct: 45),
        ]
        let bar = selectStatusBarGlanceRows(from: inventory, maxCount: 8)
        XCTAssertEqual(bar.count, 3)
        XCTAssertEqual(bar.map(\.surfaceId), ["claude", "amp", "grok"])
        XCTAssertFalse(bar.contains(where: { $0.surfaceId == "codex" }))
    }

    func testPackageSwiftUsesBinaryTargetNotHostDylib() throws {
        let package =
            sourcesRoot
            .deletingLastPathComponent()
            .appendingPathComponent("Package.swift")
        let text = try String(contentsOf: package, encoding: .utf8)
        XCTAssertTrue(
            text.contains(".binaryTarget("),
            "Package.swift must consume the static XCFramework via binaryTarget"
        )
        XCTAssertTrue(
            text.contains("JackinUsageFFI"),
            "binary target name must match the boltffi FFI module JackinUsageFFI"
        )
        XCTAssertFalse(
            text.contains("target/release"),
            "Package.swift must not link host target/release dylib path"
        )
        XCTAssertFalse(
            text.contains("linkedLibrary(\"jackin_usage_ffi\")"),
            "Package.swift must not dynamically link libjackin_usage_ffi"
        )
    }

    func testDesktopSourcesDoNotComposePercentOrResetLiterals() throws {
        let desktop = sourcesRoot.appendingPathComponent("JackinDesktop")
        let enumerator = FileManager.default.enumerator(
            at: desktop, includingPropertiesForKeys: nil)
        var files: [URL] = []
        while let url = enumerator?.nextObject() as? URL {
            if url.pathExtension == "swift" {
                files.append(url)
            }
        }
        XCTAssertFalse(files.isEmpty, "expected JackinDesktop sources")
        // Usage-string tokens: ban on display surfaces only. SettingsView may use
        // "% left"/"% used" as preference *chrome* (format picker labels) — those
        // are not composed usage numbers; Rust still owns every gauge/status string.
        let usageStringTokens = ["% left", "% used", "resets "]
        // Always ban format composition everywhere under JackinDesktop.
        let alwaysBanned = ["String(format:"]
        // Preference chrome only (S6 format pickers); never render usage data.
        let preferenceChromeFiles: Set<String> = ["SettingsView.swift", "VisualQAFixtures.swift"]
        for file in files {
            let text = try String(contentsOf: file, encoding: .utf8)
            let name = file.lastPathComponent
            for token in alwaysBanned {
                XCTAssertFalse(
                    text.contains(token),
                    "\(name) must not compose display string \(token) — use Rust FFI"
                )
            }
            if preferenceChromeFiles.contains(name) {
                continue
            }
            for token in usageStringTokens {
                XCTAssertFalse(
                    text.contains(token),
                    "\(name) must not compose display string \(token) — use Rust FFI"
                )
            }
        }
    }

    /// The three Usage-window views render Rust `UsageDetailPresentation` mechanically.
    ///
    /// They must not split/index/join
    /// usage strings, read raw buckets, use label-based identity, or invent field
    /// copy; and must consume the shared model's rows/lines/ids.
    func testUsageWindowRendersSharedDetailModel() throws {
        let usageDir =
            sourcesRoot
            .appendingPathComponent("JackinDesktop")
            .appendingPathComponent("UsageWindow")
        let files = ["UsageWindowRoot.swift", "OverviewListView.swift", "ProviderDetailView.swift"]
        let banned = [
            "splitPaceLabel",
            "displaySegments",
            "bucketMetricPrimaryLabel",
            "statusItemPercentToken",
            "surface.buckets",
            "ForEach(surface.buckets)",
            "\"Auth: \"",
            "\"Accounts\"",
            "\"— No data\"",
            "overviewNumericBucketCap",
            "sidebarSubtitle",
            "surfaceRemainingSubtitle",
            "openSettings",
        ]
        for file in files {
            let text = try String(
                contentsOf: usageDir.appendingPathComponent(file),
                encoding: .utf8
            )
            for token in banned {
                XCTAssertFalse(
                    text.contains(token),
                    "\(file) must not use \(token) — render the Rust detail model verbatim"
                )
            }
        }
        let provider = try String(
            contentsOf: usageDir.appendingPathComponent("ProviderDetailView.swift"),
            encoding: .utf8
        )
        XCTAssertTrue(provider.contains("content.detail.rows"))
        XCTAssertTrue(provider.contains("layoutLines"))
        let root = try String(
            contentsOf: usageDir.appendingPathComponent("UsageWindowRoot.swift"),
            encoding: .utf8
        )
        XCTAssertTrue(root.contains("UsageWindowModel"))
        XCTAssertTrue(root.contains("case .none:\n                    return"))
        XCTAssertFalse(root.contains("case .overview, .none:"))
        let overview = try String(
            contentsOf: usageDir.appendingPathComponent("OverviewListView.swift"),
            encoding: .utf8
        )
        XCTAssertTrue(overview.contains("UsageWindowModel.emptyHint"))
    }

    func testScreenShareProbeLivesOnlyInPresentationStore() throws {
        for file in try handwrittenSwiftFiles() {
            let text = try String(contentsOf: file, encoding: .utf8)
            let has = text.contains("CGSessionCopyCurrentDictionary")
            if file.lastPathComponent == "PresentationStore.swift" {
                XCTAssertTrue(has, "PresentationStore must own screen-share detection")
            } else {
                XCTAssertFalse(
                    has,
                    "\(file.lastPathComponent) must not call CGSessionCopyCurrentDictionary"
                )
            }
        }
    }

    func testPopoverHasNoGaugeAndSurfaceCardGone() throws {
        let desktop = sourcesRoot.appendingPathComponent("JackinDesktop")
        XCTAssertFalse(
            FileManager.default.fileExists(
                atPath: desktop.appendingPathComponent("SurfaceCard.swift").path),
            "SurfaceCard.swift must be deleted after glance popover rewrite"
        )
        let popover = desktop.appendingPathComponent("PopoverRoot.swift")
        let text = try String(contentsOf: popover, encoding: .utf8)
        XCTAssertFalse(text.contains("Gauge("), "popover must not render capacity gauges")
        XCTAssertFalse(text.contains("SurfaceCard"), "popover must not reference SurfaceCard")
    }

    func testProductIdentityUsesNativeNoninteractivePlacements() throws {
        let desktop = sourcesRoot.appendingPathComponent("JackinDesktop")
        let usageRoot = try String(
            contentsOf: desktop.appendingPathComponent("UsageWindow/UsageWindowRoot.swift"),
            encoding: .utf8
        )
        let splitController = try String(
            contentsOf: desktop.appendingPathComponent(
                "UsageWindow/UsageWindowSplitController.swift"),
            encoding: .utf8
        )
        let popover = try String(
            contentsOf: desktop.appendingPathComponent("PopoverRoot.swift"),
            encoding: .utf8
        )
        let usageController = try String(
            contentsOf: desktop.appendingPathComponent("UsageWindowController.swift"),
            encoding: .utf8
        )

        XCTAssertTrue(usageRoot.contains("struct UsageWindowDetailAccessory: View"))
        XCTAssertTrue(splitController.contains("NSSplitViewItemAccessoryViewController"))
        XCTAssertTrue(
            splitController.contains("accessory.view.setAccessibilityIdentifier")
                && splitController.contains("\"usage.detail-pane\"")
        )
        XCTAssertTrue(splitController.contains("NSSplitViewItem(sidebarWithViewController:"))
        XCTAssertTrue(splitController.contains("sidebarItem.allowsFullHeightLayout = true"))
        XCTAssertTrue(splitController.contains("[.toggleSidebar, .sidebarTrackingSeparator]"))
        XCTAssertTrue(usageController.contains("usage.brand-title"))
        XCTAssertFalse(usageRoot.contains("Text(\"jackin❯ desktop\")"))
        XCTAssertFalse(usageRoot.contains(".toolbar(removing: .sidebarToggle)"))
        XCTAssertFalse(usageRoot.contains("usage.sidebar-toggle"))
        XCTAssertFalse(usageRoot.contains("UsageWindowNavigationState"))
        XCTAssertTrue(popover.contains("JackinBrandSignature(width: 92, height: 24)"))
        XCTAssertTrue(popover.contains(".frame(maxWidth: .infinity)"))
        XCTAssertFalse(popover.contains("popoverBrandHeader.background"))
    }

    func testFixtureWindowVisibilityLeaseEndsOnlyWithWindowLifecycle() throws {
        let controller = try String(
            contentsOf: sourcesRoot.appendingPathComponent(
                "JackinDesktop/UsageWindowController.swift"),
            encoding: .utf8
        )

        XCTAssertTrue(controller.contains("while true"))
        XCTAssertFalse(controller.contains("for _ in 0..<120"))
        XCTAssertTrue(controller.contains("fixtureVisibilityTask?.cancel()"))
        XCTAssertTrue(controller.contains("visibilityDesired = false"))
    }

    func testVisualQAStateRestoresLiquidGlassAfterDarkMode() throws {
        let script = try String(
            contentsOf:
                sourcesRoot
                .deletingLastPathComponent()
                .appendingPathComponent("Scripts/VisualQA/state.sh"),
            encoding: .utf8
        )
        let start = try XCTUnwrap(script.range(of: "KEYS='"))
        let end = try XCTUnwrap(
            script.range(of: "'\n\nread_value", range: start.upperBound..<script.endIndex)
        )
        let keys = script[start.upperBound..<end.lowerBound]
            .split(separator: "\n")
            .map(String.init)

        XCTAssertEqual(
            Array(keys.suffix(2)),
            ["SystemEvents|darkMode", "NSGlobalDomain|NSGlassDiffusionSetting"]
        )
    }

    /// Cold launch: the AppKit delegate must open the host runtime without a menu click.
    func testApplicationDelegateOpensRuntimeOnLaunch() throws {
        let delegate =
            sourcesRoot
            .appendingPathComponent("JackinDesktop")
            .appendingPathComponent("DesktopAppDelegate.swift")
        let text = try String(contentsOf: delegate, encoding: .utf8)
        XCTAssertTrue(
            text.contains("applicationDidFinishLaunching"),
            "DesktopAppDelegate must initialize the runtime during application launch"
        )
        XCTAssertTrue(
            text.contains("store.openForLaunch(launchConfiguration)"),
            "DesktopAppDelegate must open the configured runtime on cold launch"
        )
    }

    func testSettingsHydrationDoesNotPersistClampedFloor() throws {
        let settings =
            sourcesRoot
            .appendingPathComponent("JackinDesktop/SettingsView.swift")
        let text = try String(contentsOf: settings, encoding: .utf8)

        XCTAssertTrue(text.contains("@State private var isHydrating = false"))
        XCTAssertTrue(text.contains("guard !isHydrating else { return }"))
        XCTAssertTrue(text.contains("isHydrating = true"))
        XCTAssertTrue(text.contains("DispatchQueue.main.async { isHydrating = false }"))
    }

    func testDesktopSourcesHaveNoHardcodedProviderDisplayNames() throws {
        let desktop = sourcesRoot.appendingPathComponent("JackinDesktop")
        let enumerator = FileManager.default.enumerator(
            at: desktop, includingPropertiesForKeys: nil)
        let banned = ["\"OpenAI\"", "\"Anthropic\"", "\"xAI\"", "\"Z.AI\""]
        while let url = enumerator?.nextObject() as? URL {
            guard url.pathExtension == "swift" else { continue }
            if url.lastPathComponent == "VisualQAFixtures.swift" { continue }
            let text = try String(contentsOf: url, encoding: .utf8)
            for token in banned {
                XCTAssertFalse(
                    text.contains(token),
                    "\(url.lastPathComponent) must not hardcode provider display name \(token)"
                )
            }
        }
    }
}
