// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit
import XCTest

@testable import JackinDesktopUI
@testable import JackinUsageBridge

final class VisualQAFixturesTests: XCTestCase {
    func testCatalogContainsEveryStableFixtureExactlyOnce() {
        XCTAssertEqual(VisualQAFixtureID.allCases.count, 15)
        XCTAssertEqual(Set(VisualQAFixtureID.allCases.map(\.rawValue)).count, 15)
        for id in VisualQAFixtureID.allCases {
            XCTAssertEqual(VisualQAFixtures.fixture(id: id).id, id)
        }
    }

    func testCatalogProviderOrderAndLayoutEnvelope() {
        let catalog = VisualQAFixtures.fixture(id: .catalogNormal)
        XCTAssertEqual(
            catalog.glanceRows.map(\.surfaceId),
            ["codex", "claude", "amp", "grok", "zai", "kimi", "minimax"]
        )
        XCTAssertEqual(
            catalog.statusGlanceRows.map(\.surfaceId),
            ["claude", "codex", "minimax"]
        )
        let overviewPlanOrStatus = catalog.providerGroups.flatMap { group in
            [group.planOrStatusLabel] + group.accounts.map(\.planOrStatusLabel)
        }
        XCTAssertFalse(overviewPlanOrStatus.contains("fresh"))
        XCTAssertEqual(VisualQAFixtures.fixture(id: .layoutEnvelope).accounts.count, 10)
    }

    func testFixtureIdentitiesAreSynthetic() {
        for id in VisualQAFixtureID.allCases {
            let fixture = VisualQAFixtures.fixture(id: id)
            for account in fixture.accounts {
                XCTAssertFalse(account.accountLabel.contains("chainargos"))
                XCTAssertFalse(account.accountLabel.contains("zhokhov"))
                if account.accountLabel.contains("@") {
                    XCTAssertTrue(account.accountLabel.hasSuffix(".test"))
                }
            }
        }
    }

    func testFixtureModeRequiresExplicitSelector() {
        let production = VisualQALaunchOptions.resolve(
            arguments: ["JackinDesktop"], environment: [:])
        XCTAssertFalse(production.usesFixture)

        let fixture = VisualQALaunchOptions.resolve(
            arguments: ["JackinDesktop", "--fixture", "F03-multi-account"],
            environment: [:]
        )
        XCTAssertEqual(fixture.fixtureID, .multiAccount)
        XCTAssertNil(fixture.invalidFixtureID)

        let environmentOnly = VisualQALaunchOptions.resolve(
            arguments: ["JackinDesktop"],
            environment: ["JACKIN_DESKTOP_FIXTURE": "F03-multi-account"]
        )
        XCTAssertFalse(environmentOnly.usesFixture)

        let invalid = VisualQALaunchOptions.resolve(
            arguments: ["JackinDesktop", "--fixture", "F99-unknown"],
            environment: [:]
        )
        XCTAssertNil(invalid.fixtureID)
        XCTAssertEqual(invalid.invalidFixtureID, "F99-unknown")
    }

    @MainActor
    func testFixtureAccountSelectionNeverCallsBridge() {
        let fixture = VisualQAFixtures.fixture(id: .multiAccount)
        let store = PresentationStore()
        store.applyQIFixture(
            glanceRows: fixture.glanceRows,
            statusBarGlanceRows: fixture.statusGlanceRows,
            surfaces: fixture.surfaces,
            accounts: fixture.accounts,
            providerGroups: fixture.providerGroups,
            refreshingProjection: fixture.refreshingProjection,
            accountProjections: fixture.accountProjections,
            popoverSelection: fixture.popoverSelection,
            usageSelection: fixture.usageSelection
        )

        store.setSelectedAccount(surfaceId: "codex", accountKey: "codex-organization")

        XCTAssertTrue(
            store.accounts.first { $0.accountKey == "codex-organization" }?.selected == true
        )
        XCTAssertEqual(store.accounts.filter(\.selected).count, 1)
        XCTAssertEqual(
            store.providerGlanceRows.first?.accountLabel,
            "organization-production-sandbox@example.test"
        )
        XCTAssertEqual(store.providerGlanceRows.first?.glanceRemainingPercent, 88)
        XCTAssertEqual(
            store.surfaces.first?.identity?.accountLabel,
            "organization-production-sandbox@example.test")
        XCTAssertEqual(
            store.surfaces.first?.detailPresentation.rows.last?.displayLabel,
            "88% left"
        )
    }

    @MainActor
    func testFixturePreferenceChangesNeverCallBridge() async {
        let fixture = VisualQAFixtures.fixture(id: .multiAccount)
        let scheduler = RefreshScheduler()
        scheduler.invalidateAndShutdown()
        let store = PresentationStore(scheduler: scheduler)
        store.applyQIFixture(
            glanceRows: fixture.glanceRows,
            statusBarGlanceRows: fixture.statusGlanceRows,
            surfaces: fixture.surfaces,
            accounts: fixture.accounts,
            providerGroups: fixture.providerGroups,
            refreshingProjection: fixture.refreshingProjection,
            accountProjections: fixture.accountProjections,
            popoverSelection: fixture.popoverSelection,
            usageSelection: fixture.usageSelection
        )

        store.displayMode = .focusPercent
        store.percentStyle = "used"
        store.resetStyle = "exact_clock"
        store.hideWhileScreenSharing.toggle()
        for _ in 0..<4 { await Task.yield() }

        XCTAssertTrue(store.usesFixture)
        XCTAssertNil(store.lastError)
        XCTAssertEqual(
            store.providerGlanceRows.map(\.surfaceId), fixture.glanceRows.map(\.surfaceId))
    }

    @MainActor
    func testFixtureRefreshTransitionsToUpdatingAndBack() async throws {
        let fixture = VisualQAFixtures.fixture(id: .catalogNormal)
        let store = PresentationStore()
        store.applyQIFixture(
            glanceRows: fixture.glanceRows,
            statusBarGlanceRows: fixture.statusGlanceRows,
            surfaces: fixture.surfaces,
            accounts: fixture.accounts,
            providerGroups: fixture.providerGroups,
            refreshingProjection: fixture.refreshingProjection,
            accountProjections: fixture.accountProjections,
            popoverSelection: fixture.popoverSelection,
            usageSelection: fixture.usageSelection
        )

        store.refreshAll()
        XCTAssertTrue(store.refreshInProgress)
        XCTAssertTrue(store.providerGlanceRows.allSatisfy { $0.activityLabel == "Updating…" })

        try await Task.sleep(for: .milliseconds(2_200))
        XCTAssertFalse(store.refreshInProgress)
        XCTAssertTrue(store.providerGlanceRows.allSatisfy { $0.activityLabel == "Updated now" })
    }

    @MainActor
    func testFixtureSnapshotNormalizesRemovedProviderAtStateOwner() {
        let fixture = VisualQAFixtures.fixture(id: .catalogNormal)
        let store = PresentationStore()
        let remainingGlance = fixture.glanceRows.filter { $0.surfaceId != "codex" }
        let remainingSurfaces = fixture.surfaces.filter { $0.id != "codex" }
        let remainingAccounts = fixture.accounts.filter { $0.surfaceId != "codex" }
        let remainingGroups = fixture.providerGroups.filter { $0.surfaceId != "codex" }

        store.applyQIFixture(
            glanceRows: remainingGlance,
            surfaces: remainingSurfaces,
            accounts: remainingAccounts,
            providerGroups: remainingGroups,
            popoverSelection: "codex",
            usageSelection: "codex"
        )

        XCTAssertNil(store.usageSelection)
        XCTAssertEqual(store.popoverSelection, remainingGlance.first?.surfaceId)
    }

    @MainActor
    func testRetainedUsageWindowPreservesValidDestinationUntilExplicitlyChanged() {
        _ = NSApplication.shared
        let fixture = VisualQAFixtures.fixture(id: .multiAccount)
        let store = PresentationStore()
        store.applyQIFixture(
            glanceRows: fixture.glanceRows,
            surfaces: fixture.surfaces,
            accounts: fixture.accounts,
            providerGroups: fixture.providerGroups,
            popoverSelection: fixture.popoverSelection,
            usageSelection: fixture.usageSelection
        )
        let controller = UsageWindowController(store: store)
        defer { controller.invalidate() }

        controller.show(focusOn: "codex")
        let retainedWindow = controller.qiWindow
        retainedWindow?.close()
        controller.show()

        XCTAssertTrue(controller.qiWindow === retainedWindow)
        XCTAssertEqual(store.usageSelection, "codex")

        controller.show(focusOn: nil)
        XCTAssertNil(store.usageSelection)
    }

    func testProductionSourcesExposeNoDestructiveAction() throws {
        let sources = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("Sources/JackinDesktop")
        for relativePath in [
            "PopoverRoot.swift",
            "UsageWindow/UsageWindowRoot.swift",
            "UsageWindow/ProviderDetailView.swift",
        ] {
            let text = try String(
                contentsOf: sources.appendingPathComponent(relativePath),
                encoding: .utf8
            ).lowercased()
            for token in ["delete", "revoke", "consume reset", "sign out"] {
                XCTAssertFalse(text.contains(token), "\(relativePath) exposes \(token)")
            }
        }
    }

    func testProductionSourcesExposeNoInvisibleShortcutControl() throws {
        let source = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("Sources/JackinDesktop/PopoverRoot.swift")
        let text = try String(contentsOf: source, encoding: .utf8)

        XCTAssertTrue(text.contains("accessibilityIdentifier(\"popover.refresh\")"))
        XCTAssertTrue(text.contains("keyboardShortcut(\"r\""))
        XCTAssertFalse(text.contains(".hidden()"))
    }

    func testRefreshingStatusUsesAccessibleNonfocusedProgress() throws {
        let source = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("Sources/JackinDesktop/PopoverRoot.swift")
        let text = try String(contentsOf: source, encoding: .utf8)

        XCTAssertTrue(text.contains("if provider.isRefreshing"))
        XCTAssertTrue(text.contains(".accessibilityLabel(provider.activityLabel)"))
        XCTAssertFalse(text.contains("accessibilityFocused"))
    }
}
