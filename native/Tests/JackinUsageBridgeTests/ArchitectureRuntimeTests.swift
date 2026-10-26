// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import Foundation
import JackinUsageBridge
import XCTest

extension ArchitectureTests {
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
        XCTAssertTrue(text.contains("mise -C \"$repo\" exec -- mbx +1.99.1 xtask desktop build"))
        XCTAssertTrue(text.contains("mise -C \"$repo\" exec -- mbx +1.99.1 xtask desktop verify"))
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
        let automation =
            sourcesRoot
            .appendingPathComponent("JackinDesktop")
            .appendingPathComponent("DesktopAppDelegate+VisualQA.swift")
        let automationText = try String(contentsOf: automation, encoding: .utf8)
        XCTAssertFalse(
            automationText.contains(
                "if visualQALaunchOptions.elevatesFixtureWindow {\n            return .accessory"
            )
        )
        XCTAssertTrue(
            automationText.contains(
                "if visualQALaunchOptions.openUsage || visualQALaunchOptions.openPopover"
            )
        )
        XCTAssertTrue(delegateText.contains("initialActivationPolicy"))
        XCTAssertTrue(automationText.contains("return .accessory"))

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

    func testStatusBarUsesTemplateMarksAndSemanticTextColors() throws {
        let label =
            sourcesRoot
            .appendingPathComponent("JackinDesktop/StatusItemLabel.swift")
        let text = try String(contentsOf: label, encoding: .utf8)
        let marks =
            sourcesRoot
            .appendingPathComponent("JackinDesktop/ProviderMarks.swift")
        let marksText = try String(contentsOf: marks, encoding: .utf8)
        XCTAssertTrue(
            text.contains("ProviderMarks.templateImage(forIconKey: iconKey)"),
            "status icons must use the native template-mark loader"
        )
        XCTAssertTrue(
            text.contains("image.isTemplate = true") && text.contains("return fallbackIcon()"),
            "fallback status icons must remain template images"
        )
        XCTAssertTrue(
            marksText.contains("loadPDF(named: iconKey, template: true)")
                && marksText.contains("copy.isTemplate = template"),
            "provider marks must be rendered as template images"
        )
        XCTAssertTrue(text.contains("NSColor.labelColor"))
        XCTAssertTrue(text.contains("NSColor.secondaryLabelColor"))
        XCTAssertFalse(text.contains("severityTint"))
        XCTAssertFalse(text.contains(".tint("), "status text must not carry severity tint")
        XCTAssertFalse(text.contains(".systemRed"))
        XCTAssertFalse(text.contains(".systemGreen"))
        XCTAssertFalse(text.contains(".systemYellow"))
    }

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

    func testSelectStatusBarGlanceRowsHidesZeroAndCapsThree() {
        func row(id: String, pct: Double?) -> GlanceProviderRow {
            GlanceProviderRow(
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
                dimmed: false,
                glanceMeterPercent: pct,
                glanceMeterSide: pct == nil ? nil : "remaining"
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
}
