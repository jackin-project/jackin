// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import JackinUsageBindings
import XCTest

@testable import JackinDesktopUI
@testable import JackinUsageBridge

extension PresentationStoreTests {
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
        let input = fixture.input.replacing(
            transitions: QIFixtureTransitions(),
            selection: QIFixtureSelection(
                popover: fixture.popoverSelection,
                usage: "codex",
                account: priorAccount.accountKey
            )
        )
        let projection = QIFixtureProjection(
            glanceRows: input.projection.glanceRows,
            statusBarGlanceRows: input.projection.statusBarGlanceRows,
            surfaces: input.projection.surfaces,
            accounts: input.projection.accounts,
            providerGroups: groups
        )
        store.applyQIFixture(input.replacing(projection: projection))

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
            fixture.input.replacing(
                transitions: QIFixtureTransitions(),
                selection: QIFixtureSelection(
                    popover: fixture.popoverSelection,
                    usage: "codex",
                    account: "removed-codex-account"
                )
            )
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
        let input = fixture.input.replacing(
            transitions: QIFixtureTransitions(),
            selection: QIFixtureSelection(
                popover: fixture.popoverSelection,
                usage: "codex",
                account: "removed-codex-account"
            )
        )
        let projection = QIFixtureProjection(
            glanceRows: input.projection.glanceRows,
            statusBarGlanceRows: input.projection.statusBarGlanceRows,
            surfaces: input.projection.surfaces,
            accounts: input.projection.accounts,
            providerGroups: groups
        )
        store.applyQIFixture(input.replacing(projection: projection))

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
