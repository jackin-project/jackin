// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import JackinUsageBindings
import XCTest

@testable import JackinDesktopUI
@testable import JackinUsageBridge

final class PresentationStoreTests: XCTestCase {
    private enum InjectedProjectionError: Error {
        case failed
    }

    private func providerGroups(
        copying groups: [PresentationStore.ProviderGroupRow],
        accounts: [PresentationStore.AccountRow],
        codexRoute: PresentationStore.SelectedAccountRoute? = nil
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
                lastError: group.lastError,
                selectedAccountRoute: group.surfaceId == "codex"
                    ? codexRoute ?? group.selectedAccountRoute
                    : group.selectedAccountRoute
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

    func testSelectedAccountRouteValidatesStatusKeyNoticeCombinations() {
        let notice = "Selected account is no longer available."
        let valid: [(SelectedAccountRouteDto, PresentationStore.SelectedAccountRoute)] = [
            (
                SelectedAccountRouteDto(status: "unselected", accountKey: nil, notice: nil),
                .unselected
            ),
            (
                SelectedAccountRouteDto(status: "resolving", accountKey: "persisted-key", notice: nil),
                .resolving(accountKey: "persisted-key")
            ),
            (
                SelectedAccountRouteDto(status: "available", accountKey: "persisted-key", notice: nil),
                .available(accountKey: "persisted-key")
            ),
            (
                SelectedAccountRouteDto(
                    status: "unavailable",
                    accountKey: "persisted-key",
                    notice: notice
                ),
                .unavailable(accountKey: "persisted-key", notice: notice)
            ),
        ]
        for (dto, expected) in valid {
            XCTAssertEqual(PresentationStore.SelectedAccountRoute(dto: dto), expected)
        }

        let invalid = [
            SelectedAccountRouteDto(status: "unselected", accountKey: "persisted-key", notice: nil),
            SelectedAccountRouteDto(status: "resolving", accountKey: nil, notice: nil),
            SelectedAccountRouteDto(status: "resolving", accountKey: "persisted-key", notice: notice),
            SelectedAccountRouteDto(status: "available", accountKey: "  ", notice: nil),
            SelectedAccountRouteDto(status: "unavailable", accountKey: "persisted-key", notice: nil),
            SelectedAccountRouteDto(status: "unavailable", accountKey: "persisted-key", notice: "  "),
            SelectedAccountRouteDto(status: "unknown", accountKey: "persisted-key", notice: nil),
        ]
        for dto in invalid {
            XCTAssertNil(PresentationStore.SelectedAccountRoute(dto: dto))
        }
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
    func testAccountSelectionForOtherProviderDoesNotChangeUsageContext() {
        let fixture = VisualQAFixtures.fixture(id: .catalogNormal)
        guard
            let codexAccount = fixture.accounts.first(where: {
                $0.surfaceId == "codex" && $0.accountKey == "codex-plus"
            }),
            let claudeAccount = fixture.accounts.first(where: {
                $0.surfaceId == "claude" && $0.selected
            })
        else {
            return XCTFail("catalog fixture is missing expected Codex or Claude accounts")
        }
        let store = PresentationStore()
        store.applyQIFixture(
            glanceRows: fixture.glanceRows,
            statusBarGlanceRows: fixture.statusGlanceRows,
            surfaces: fixture.surfaces,
            accounts: fixture.accounts,
            providerGroups: fixture.providerGroups,
            accountProjections: fixture.accountProjections,
            popoverSelection: fixture.popoverSelection,
            usageSelection: "claude",
            usageAccountSelection: claudeAccount.accountKey
        )

        // A picker change for Codex must not install its key into the active
        // Claude navigation context.
        store.setSelectedAccount(surfaceId: "codex", accountKey: codexAccount.accountKey)

        XCTAssertEqual(store.usageSelection, "claude")
        XCTAssertEqual(store.usageAccountSelection, claudeAccount.accountKey)
        XCTAssertTrue(
            store.accounts.first {
                $0.surfaceId == codexAccount.surfaceId
                    && $0.accountKey == codexAccount.accountKey
            }?.selected == true
        )
    }

    @MainActor
    func testAvailableRustRouteSupersedesPriorExplicitAccountContext() {
        let fixture = VisualQAFixtures.fixture(id: .multiAccount)
        guard
            let priorAccount = fixture.accounts.first(where: {
                $0.surfaceId == "codex" && $0.selected
            }),
            let rustSelectedAccount = fixture.accounts.first(where: {
                $0.surfaceId == "codex" && $0.accountKey != priorAccount.accountKey
            })
        else {
            return XCTFail("multi-account fixture is missing distinct Codex accounts")
        }
        let groups = providerGroups(
            copying: fixture.providerGroups,
            accounts: fixture.accounts,
            codexRoute: .available(accountKey: rustSelectedAccount.accountKey)
        )
        let store = PresentationStore()
        store.applyQIFixture(
            glanceRows: fixture.glanceRows,
            statusBarGlanceRows: fixture.statusGlanceRows,
            surfaces: fixture.surfaces,
            accounts: fixture.accounts,
            providerGroups: groups,
            popoverSelection: fixture.popoverSelection,
            usageSelection: "codex",
            usageAccountSelection: priorAccount.accountKey
        )

        let model = UsageWindowModel(
            glanceRows: store.providerGlanceRows,
            surfaces: store.surfaces,
            accounts: store.accounts,
            providerGroups: store.providerGroups,
            selection: store.usageSelection
        )

        XCTAssertEqual(store.usageAccountSelection, priorAccount.accountKey)
        XCTAssertEqual(model.content?.selectedAccountKey, rustSelectedAccount.accountKey)
        XCTAssertEqual(model.content?.headAccount?.accountKey, rustSelectedAccount.accountKey)
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

    @MainActor
    func testUnavailablePersistedRouteKeepsProviderScopeForOverviewNotice() {
        let fixture = VisualQAFixtures.fixture(id: .multiAccount)
        let notice = "Selected account is no longer available."
        let groups = providerGroups(
            copying: fixture.providerGroups,
            accounts: fixture.accounts,
            codexRoute: .unavailable(accountKey: "removed-codex-account", notice: notice)
        )
        let store = PresentationStore()
        store.applyQIFixture(
            glanceRows: fixture.glanceRows,
            statusBarGlanceRows: fixture.statusGlanceRows,
            surfaces: fixture.surfaces,
            accounts: fixture.accounts,
            providerGroups: groups,
            popoverSelection: fixture.popoverSelection,
            usageSelection: "codex",
            usageAccountSelection: "removed-codex-account"
        )

        XCTAssertEqual(store.usageSelection, "codex")
        XCTAssertNil(store.usageAccountSelection)
        let model = UsageWindowModel(
            glanceRows: store.providerGlanceRows,
            surfaces: store.surfaces,
            accounts: store.accounts,
            providerGroups: store.providerGroups,
            selection: store.usageSelection
        )
        XCTAssertEqual(model.selection, .overview)
        XCTAssertEqual(model.routeNotice?.surfaceId, "codex")
        XCTAssertEqual(model.routeNotice?.message, notice)
    }
}
