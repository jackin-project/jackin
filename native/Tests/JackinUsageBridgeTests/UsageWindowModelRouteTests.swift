// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import JackinUsageBindings
import XCTest

@testable import JackinUsageBridge

extension UsageWindowModelTests {
    func testPresentProviderGroupWithInvalidRouteFailsClosed() {
        let model = UsageWindowModel(
            glanceRows: [glance("codex")],
            surfaces: [surface("codex")],
            accounts: [],
            providerGroups: [providerGroup("codex", accounts: [], route: nil)],
            selection: "codex"
        )

        XCTAssertEqual(model.selection, .overview)
        XCTAssertNil(model.content)
        XCTAssertNil(model.routeNotice)
    }

    func testMultiAccountActionAndSelectedStylingUsesExactRustRoute() {
        let accounts = [
            account("codex", key: "a", selected: true),
            account("codex", key: "b", selected: false),
        ]
        let model = UsageWindowModel(
            glanceRows: [glance("codex")],
            surfaces: [surface("codex", detail: .empty)],
            accounts: accounts,
            providerGroups: [
                providerGroup(
                    "codex",
                    accounts: accounts,
                    route: .available(accountKey: "a")
                )
            ],
            selection: "codex"
        )
        XCTAssertEqual(model.content?.accounts.count, 2)
        XCTAssertEqual(model.content?.accounts.first?.selected, true)
        XCTAssertEqual(model.content?.headAccount?.accountKey, "a")

        let selected = UsageWindowModel(
            glanceRows: [glance("codex")],
            surfaces: [surface("codex", detail: .empty)],
            accounts: accounts,
            providerGroups: [
                providerGroup(
                    "codex",
                    accounts: accounts,
                    route: .available(accountKey: "b")
                )
            ],
            selection: "codex"
        )
        XCTAssertEqual(selected.content?.headAccount?.accountKey, "b")
    }

    func testAvailableRouteMissingFromProviderCatalogFailsClosed() {
        let accounts = [
            account("codex", key: "selected-sibling", selected: true),
            account("codex", key: "other-sibling", selected: false),
        ]
        let model = UsageWindowModel(
            glanceRows: [glance("codex")],
            surfaces: [surface("codex", detail: .empty)],
            accounts: accounts,
            providerGroups: [
                providerGroup(
                    "codex",
                    accounts: accounts,
                    route: .available(accountKey: "removed-account")
                )
            ],
            selection: "codex"
        )

        XCTAssertEqual(model.selection, .overview)
        XCTAssertNil(model.content)
        XCTAssertNil(model.routeNotice)
    }

    func testUnselectedAndResolvingRoutesNeverChooseSiblingAccounts() {
        let siblings = [
            account("codex", key: "sibling-a", selected: true),
            account("codex", key: "sibling-b", selected: false),
        ]
        let unselected = UsageWindowModel(
            glanceRows: [glance("codex")],
            surfaces: [surface("codex")],
            accounts: siblings,
            providerGroups: [
                providerGroup("codex", accounts: siblings, route: .unselected)
            ],
            selection: "codex"
        )
        XCTAssertEqual(unselected.content?.selectedAccountKey, nil)
        XCTAssertNil(unselected.content?.headAccount)

        let resolving = UsageWindowModel(
            glanceRows: [glance("codex")],
            surfaces: [surface("codex")],
            accounts: siblings,
            providerGroups: [
                providerGroup(
                    "codex",
                    accounts: siblings,
                    route: .resolving(accountKey: "cold-persisted-key")
                )
            ],
            selection: "codex"
        )
        XCTAssertEqual(resolving.selection, .provider("codex"))
        XCTAssertEqual(
            resolving.content?.selectedAccountRoute, .resolving(accountKey: "cold-persisted-key"))
        XCTAssertNil(resolving.content?.selectedAccountKey)
        XCTAssertNil(resolving.content?.headAccount)
        XCTAssertNil(resolving.routeNotice)
    }

    func testAvailableAndRestoredRouteUsesOnlyExactAccountAndClearsNotice() {
        let accounts = [
            account("codex", key: "sibling", selected: true),
            account("codex", key: "restored", selected: false),
        ]
        let model = UsageWindowModel(
            glanceRows: [glance("codex")],
            surfaces: [surface("codex")],
            accounts: accounts,
            providerGroups: [
                providerGroup(
                    "codex",
                    accounts: accounts,
                    route: .available(accountKey: "restored")
                )
            ],
            selection: "codex"
        )

        XCTAssertEqual(model.content?.selectedAccountKey, "restored")
        XCTAssertEqual(model.content?.headAccount?.accountKey, "restored")
        XCTAssertNil(model.routeNotice)
    }

    func testUnavailableRouteWithSiblingShowsFixedNoticeAndPreservesProviderScope() {
        let codexSibling = account("codex", key: "codex-sibling", selected: true)
        let claudeSelected = account("claude", key: "claude-selected", selected: true)
        let accountRows = [codexSibling, claudeSelected]
        let notice = "Selected account is no longer available."
        let groups = [
            providerGroup(
                "codex",
                accounts: [codexSibling],
                route: .unavailable(accountKey: "removed-codex", notice: notice)
            ),
            providerGroup(
                "claude",
                accounts: [claudeSelected],
                route: .available(accountKey: "claude-selected")
            ),
        ]
        let model = UsageWindowModel(
            glanceRows: [glance("codex"), glance("claude")],
            surfaces: [surface("codex"), surface("claude")],
            accounts: accountRows,
            providerGroups: groups,
            selection: "codex"
        )

        XCTAssertEqual(model.selection, .overview)
        XCTAssertNil(model.content)
        XCTAssertEqual(model.routeNotice?.surfaceId, "codex")
        XCTAssertEqual(model.routeNotice?.message, notice)
        XCTAssertNotEqual(model.routeNotice?.surfaceId, "claude")
    }

    func testProviderLastErrorDoesNotCreateAnAccountRouteNotice() {
        let group = ProviderGroupRow(
            surfaceId: "codex",
            displayLabel: "OpenAI",
            iconKey: "codex",
            fallbackGlyph: "?",
            usageURL: nil,
            accountColumnLabel: "Accounts",
            planOrStatusLabel: "Needs login",
            remainingLabel: "—",
            resetDisplayLabel: "—",
            accounts: [],
            accessibilityLabel: "OpenAI",
            lastError: "Credentials expired; sign in again.",
            selectedAccountRoute: .unselected
        )
        let model = UsageWindowModel(
            glanceRows: [glance("codex")],
            surfaces: [surface("codex")],
            accounts: [],
            providerGroups: [group],
            selection: "codex"
        )

        XCTAssertEqual(model.selection, .provider("codex"))
        XCTAssertNil(model.routeNotice)
        XCTAssertNil(model.content?.selectedAccountKey)
    }

    func testSentinelRowsTransmittedUnchanged() {
        let ampDaily = detailRow(
            "bucket:0",
            kind: .bucket,
            label: "Daily",
            lines: [line(leading: "61% left"), line(trailing: "Resets daily")]
        )
        let grokPlan = detailRow(
            "plan",
            kind: .metadata,
            label: "Plan",
            lines: [line(leading: "SuperGrok")]
        )
        let detail = UsageDetailPresentation(rows: [grokPlan, ampDaily])
        let model = UsageWindowModel(
            glanceRows: [glance("amp")],
            surfaces: [surface("amp", detail: detail)],
            accounts: [],
            selection: "amp"
        )
        guard let content = model.content else { return XCTFail("missing provider content") }
        let rows = content.detail.rows
        XCTAssertEqual(rows[0].displayLabel, "SuperGrok")
        XCTAssertEqual(rows[1].displayLabel, "61% left · Resets daily")
    }
}
