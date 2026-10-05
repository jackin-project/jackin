// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import XCTest

@testable import JackinDesktopUI
@testable import JackinUsageBridge

final class PresentationStoreTests: XCTestCase {
    private enum InjectedProjectionError: Error {
        case failed
    }

    private func providerGroups(
        copying groups: [PresentationStore.ProviderGroupRow],
        accounts: [PresentationStore.AccountRow]
    ) -> [PresentationStore.ProviderGroupRow] {
        groups.map { group in
            PresentationStore.ProviderGroupRow(
                surfaceId: group.surfaceId,
                displayLabel: group.displayLabel,
                iconKey: group.iconKey,
                fallbackGlyph: group.fallbackGlyph,
                usageURL: group.usageURL,
                accountColumnLabel: group.accountColumnLabel,
                planOrStatusLabel: group.planOrStatusLabel,
                remainingLabel: group.remainingLabel,
                resetDisplayLabel: group.resetDisplayLabel,
                accounts: accounts.filter { $0.surfaceId == group.surfaceId },
                accessibilityLabel: group.accessibilityLabel,
                lastError: group.lastError
            )
        }
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

    @MainActor
    func testMissingClaudeAccountDoesNotClearValidCodexRoute() {
        let fixture = VisualQAFixtures.fixture(id: .catalogNormal)
        guard
            let codexAccount = fixture.accounts.first(where: {
                $0.surfaceId == "codex" && $0.selected
            }),
            let claudeAccount = fixture.accounts.first(where: { $0.surfaceId == "claude" }),
            let claudeSibling = fixture.accounts.first(where: {
                $0.surfaceId == "claude" && $0.id != claudeAccount.id
            })
        else {
            return XCTFail("catalog fixture is missing expected Codex or Claude accounts")
        }
        // Claude's selected account is absent from this projection while the
        // active Codex account remains present. Other providers must not clear
        // the current provider/account route.
        let projectedAccounts = fixture.accounts.filter { $0.id != claudeAccount.id }
        let store = PresentationStore()

        store.applyQIFixture(
            glanceRows: fixture.glanceRows,
            statusBarGlanceRows: fixture.statusGlanceRows,
            surfaces: fixture.surfaces,
            accounts: projectedAccounts,
            providerGroups: providerGroups(
                copying: fixture.providerGroups,
                accounts: projectedAccounts
            ),
            popoverSelection: fixture.popoverSelection,
            usageSelection: "codex",
            usageAccountSelection: codexAccount.accountKey
        )

        XCTAssertFalse(store.accounts.contains { $0.id == claudeAccount.id })
        XCTAssertTrue(store.accounts.contains { $0.id == claudeSibling.id })
        XCTAssertEqual(
            store.providerGroups.first(where: { $0.surfaceId == "claude" })?.accounts.map(\.id),
            [claudeSibling.id]
        )
        XCTAssertEqual(store.usageSelection, "codex")
        XCTAssertEqual(store.usageAccountSelection, codexAccount.accountKey)
    }

    @MainActor
    func testMissingActiveAccountReturnsToOverviewInsteadOfSelectingSibling() {
        let fixture = VisualQAFixtures.fixture(id: .multiAccount)
        guard
            let sibling = fixture.accounts.first(where: {
                $0.surfaceId == "codex" && $0.selected
            })
        else {
            return XCTFail("multi-account fixture is missing its selected Codex sibling")
        }
        let store = PresentationStore()

        store.applyQIFixture(
            glanceRows: fixture.glanceRows,
            statusBarGlanceRows: fixture.statusGlanceRows,
            surfaces: fixture.surfaces,
            accounts: fixture.accounts,
            providerGroups: fixture.providerGroups,
            popoverSelection: fixture.popoverSelection,
            usageSelection: "codex",
            usageAccountSelection: "removed-codex-account"
        )

        XCTAssertNotEqual(sibling.accountKey, "removed-codex-account")
        XCTAssertTrue(store.accounts.contains { $0.id == sibling.id })
        XCTAssertNil(store.usageSelection)
        XCTAssertNil(store.usageAccountSelection)
    }
}
