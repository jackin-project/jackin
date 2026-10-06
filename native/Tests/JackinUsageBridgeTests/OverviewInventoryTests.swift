// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import XCTest

@testable import JackinUsageBridge

final class OverviewInventoryTests: XCTestCase {
    private func account(
        surface: String,
        key: String,
        provider: String,
        account: String
    ) -> PresentationStore.AccountRow {
        PresentationStore.AccountRow(
            surfaceId: surface,
            providerColumnLabel: provider,
            accountKey: key,
            accountLabel: account,
            planLabel: "Fixture plan",
            selected: key == "a1",
            lifecycle: "authenticated",
            lifecycleLabel: "Authenticated",
            provenanceLabel: "Fixture source",
            planOrStatusLabel: "Fixture plan",
            remainingPercent: 57,
            remainingLabel: "57%",
            headline: "57% left",
            resetDisplayLabel: "Resets in 3d (17 Aug 2026, 17:02)",
            statusWord: "fresh",
            statusLabel: "Ready",
            severity: "normal",
            updatedLabel: "Updated now",
            lastError: nil,
            dimmed: false,
            accessibilityLabel: "\(provider), \(account), Fixture plan, 57%"
        )
    }

    private func group(
        surface: String,
        provider: String,
        accounts: [PresentationStore.AccountRow]
    ) -> PresentationStore.ProviderGroupRow {
        PresentationStore.ProviderGroupRow(
            surfaceId: surface,
            displayLabel: provider,
            iconKey: surface,
            fallbackGlyph: "?",
            usageURL: "https://example.test/usage",
            accountColumnLabel: "\(accounts.count) accounts",
            planOrStatusLabel: "Multiple plans",
            remainingLabel: "12–57%",
            resetDisplayLabel: "Varies",
            accounts: accounts,
            accessibilityLabel: "\(provider), \(accounts.count) accounts",
            lastError: nil
        )
    }

    func testTreePreservesRustGroupAndAccountOrder() {
        let claude = group(
            surface: "claude",
            provider: "Anthropic",
            accounts: [account(surface: "claude", key: "p1", provider: "", account: "Personal")]
        )
        let codex = group(
            surface: "codex",
            provider: "OpenAI",
            accounts: [
                account(surface: "codex", key: "a1", provider: "", account: "alexey@example.test"),
                account(surface: "codex", key: "a2", provider: "", account: "work@example.test"),
            ]
        )

        let rows = OverviewInventory.tree(groups: [claude, codex])

        XCTAssertEqual(rows.map(\.id), ["provider#claude", "provider#codex"])
        XCTAssertEqual(rows[1].children?.map(\.id), ["account#codex#a1", "account#codex#a2"])
    }

    func testTreeCopiesFinishedDisplayStringsVerbatim() {
        let child = account(
            surface: "codex",
            key: "a1",
            provider: "",
            account: "alexey@example.test"
        )
        let projected = group(surface: "codex", provider: "OpenAI", accounts: [child])

        guard let provider = OverviewInventory.tree(groups: [projected]).first,
            let account = provider.children?.first
        else {
            return XCTFail("missing hierarchy")
        }

        XCTAssertEqual(provider.providerLabel, projected.displayLabel)
        XCTAssertEqual(provider.accountLabel, projected.accountColumnLabel)
        XCTAssertEqual(provider.planOrStatusLabel, projected.planOrStatusLabel)
        XCTAssertEqual(provider.remainingLabel, projected.remainingLabel)
        XCTAssertEqual(provider.resetLabel, projected.resetDisplayLabel)
        XCTAssertEqual(account.accountLabel, child.accountLabel)
        XCTAssertEqual(account.resetLabel, child.resetDisplayLabel)
        XCTAssertEqual(account.accessibilityLabel, child.accessibilityLabel)
    }

    func testProviderWithoutAccountsIsLeaf() {
        let projected = group(surface: "kimi", provider: "Kimi", accounts: [])
        let row = OverviewInventory.tree(groups: [projected])[0]

        XCTAssertTrue(row.isProvider)
        XCTAssertNil(row.children)
    }
}
