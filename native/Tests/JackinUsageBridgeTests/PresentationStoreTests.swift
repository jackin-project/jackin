// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import XCTest

@testable import JackinDesktopUI
@testable import JackinUsageBridge

final class PresentationStoreTests: XCTestCase {
    private enum InjectedProjectionError: Error {
        case failed
    }

    func testProductionLaunchDoesNotRequireSwiftOwnedHostPaths() {
        let launch = PresentationStore.LaunchConfiguration.resolve(
            environment: [:],
            homeDirectory: "/operator"
        )
        XCTAssertEqual(launch, .production)
    }

    func testDiscoveryDiagnosticKeepsRustOwnedSanitizedCopy() {
        let diagnostic = PresentationStore.DiscoveryDiagnostic(
            surfaceId: "claude",
            scopeLabel: "workspace sample",
            issue: "credential_denied",
            message: "Credential access was denied",
            displayLabel: "workspace sample: Credential access was denied"
        )

        XCTAssertEqual(diagnostic.id, "claude#workspace sample#credential_denied")
        XCTAssertEqual(
            diagnostic.displayLabel,
            "workspace sample: Credential access was denied"
        )
        XCTAssertFalse(diagnostic.displayLabel.contains("/Users/"))
        XCTAssertFalse(diagnostic.displayLabel.contains("op://"))
    }

    @MainActor
    func testProjectionFailureRetainsExactLastGoodStateAndSelection() {
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
        store.selectUsageContext(surfaceId: "codex", accountKey: "codex-plus")

        let glances = store.providerGlanceRows
        let surfaces = store.surfaces
        let accounts = store.accounts
        let groups = store.providerGroups
        let usageSelection = store.usageSelection
        let accountSelection = store.usageAccountSelection
        let popoverSelection = store.popoverSelection

        store.applyProjectionFailureForTesting(InjectedProjectionError.failed)

        XCTAssertEqual(store.providerGlanceRows, glances)
        XCTAssertEqual(store.surfaces, surfaces)
        XCTAssertEqual(store.accounts, accounts)
        XCTAssertEqual(store.providerGroups, groups)
        XCTAssertEqual(store.usageSelection, usageSelection)
        XCTAssertEqual(store.usageAccountSelection, accountSelection)
        XCTAssertEqual(store.popoverSelection, popoverSelection)
        XCTAssertEqual(store.lastError, "Usage could not be updated. Try again.")
    }
}
