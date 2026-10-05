// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit
import JackinUsageBindings
import SwiftUI
import XCTest

@testable import JackinDesktopUI
@testable import JackinUsageBridge

@MainActor
final class PresentationSelectionTests: XCTestCase {
    private let notice = "Selected account is no longer available."

    func testRemovedAccountKeepsOverviewNoticeAndRequiresExplicitSiblingSelection() {
        let store = PresentationStore()
        store.applyProjection(
            codex(["a", "b"], "a", 1),
            request: 1)
        store.selectUsageContext(
            surfaceId: "codex",
            accountKey: "a")
        let removed = codex(["b"], "a", 2)
        store.applyProjection(
            removed,
            request: 2)
        XCTAssertNil(store.usageSelection)
        XCTAssertNil(store.usageAccountSelection)
        XCTAssertEqual(store.usageNotice, notice)
        XCTAssertFalse(store.accounts[0].selected)

        store.applyProjection(
            removed,
            request: 3)
        XCTAssertEqual(store.usageNotice, notice)
        store.selectUsageSurface("codex")
        XCTAssertNil(store.usageSelection)
        XCTAssertEqual(store.usageNotice, notice)

        store.selectUsageContext(
            surfaceId: "codex",
            accountKey: "b")
        store.applyProjection(
            removed,
            request: 4)
        XCTAssertNil(store.usageSelection)
        XCTAssertEqual(store.usageNotice, notice)
        store.applyProjection(
            codex(["b"], "b", 3),
            request: 5)
        store.selectUsageContext(
            surfaceId: "codex",
            accountKey: "b")
        XCTAssertEqual(store.usageAccountSelection, "b")
        XCTAssertEqual(model(store).content?.headAccount?.accountKey, "b")
        XCTAssertNil(store.usageNotice)
    }

    func testUnrelatedUnavailableSelectionPreservesActiveExplicitAccount() {
        let store = PresentationStore()
        store.applyProjection(
            projection([
                provider(
                    "claude",
                    keys: ["old", "other"],
                    selected: "old"),
                provider(
                    "codex",
                    keys: ["work", "personal"],
                    selected: "work"),
            ]),
            request: 1)
        store.selectUsageContext(
            surfaceId: "codex",
            accountKey: "work")
        store.applyProjection(
            projection(
                [
                    provider(
                        "claude",
                        keys: ["other"],
                        selected: "old"),
                    provider(
                        "codex",
                        keys: ["personal", "work"],
                        selected: "work"),
                ],
                generation: 2),
            request: 2)
        XCTAssertEqual(store.usageSelection, "codex")
        XCTAssertEqual(store.usageAccountSelection, "work")
        XCTAssertNil(store.usageNotice)
        XCTAssertEqual(model(store).content?.headAccount?.accountKey, "work")
    }

    func testLastRemovedAccountLeavesActionableProviderRowAndNotice() {
        let store = PresentationStore()
        store.applyProjection(
            codex(["a"], "a", 1),
            request: 1)
        store.selectUsageSurface("codex")
        store.applyProjection(
            codex([], "a", 2),
            request: 2)
        XCTAssertNil(store.usageSelection)
        XCTAssertEqual(store.usageNotice, notice)
        XCTAssertTrue(store.accounts.isEmpty)
        XCTAssertEqual(store.providerGroups.first?.lastError, notice)
        XCTAssertEqual(store.providerGroups.first?.surfaceId, "codex")
        XCTAssertEqual(model(store).selection, .overview)

        store.selectUsageSurface("codex")
        XCTAssertNil(store.usageSelection)
        XCTAssertEqual(store.usageNotice, notice)
        store.selectUsageSurface(nil)
        XCTAssertNil(store.usageNotice)
    }

    func testProviderOnlyDestinationUsesExactRustSelectionAfterReordering() {
        let store = PresentationStore()
        store.applyProjection(
            codex(["a", "b"], "b", 1),
            request: 1)
        store.selectUsageSurface("codex")
        XCTAssertEqual(store.usageAccountSelection, "b")
        store.applyProjection(
            codex(["b", "a"], "b", 2),
            request: 2)
        XCTAssertEqual(store.usageAccountSelection, "b")
        XCTAssertEqual(model(store).content?.headAccount?.accountKey, "b")
    }

    func testReappearingPersistedAccountDoesNotNavigateWithoutUserAction() {
        let store = PresentationStore()
        store.applyProjection(
            codex(["a"], "a", 1),
            request: 1)
        store.selectUsageSurface("codex")
        store.applyProjection(
            codex([], "a", 2),
            request: 2)
        store.applyProjection(
            codex(["a"], "a", 3),
            request: 3)
        XCTAssertNil(store.usageSelection)
        XCTAssertEqual(store.usageNotice, notice)
        store.selectUsageSurface("codex")
        XCTAssertEqual(store.usageAccountSelection, "a")
        XCTAssertNil(store.usageNotice)
    }

    func testLatestAccountIntentWinsWhenEarlierProjectionCompletesFirst() async {
        let client = ControlledSelectionClient()
        let store = PresentationStore(
            scheduler: RefreshScheduler(),
            selectionClient: client)
        store.applyProjection(
            codex(["a", "b", "c"], "a", 1),
            request: 0)
        store.selectUsageSurface("codex")
        guard
            let earlier = store.setAccountSelection(
                surfaceId: "codex",
                accountKey: "b",
                navigateToUsage: true)
        else {
            return XCTFail("missing production selection task")
        }
        await client.waitForRead(1)
        guard
            let latest = store.setAccountSelection(
                surfaceId: "codex",
                accountKey: "c",
                navigateToUsage: true)
        else {
            return XCTFail("missing production selection task")
        }
        await client.waitForRead(2)
        await client.completeRead(
            1,
            projection: codex(["a", "b", "c"], "b", 2))
        await earlier.value
        XCTAssertEqual(store.usageSelection, "codex")
        XCTAssertEqual(store.usageAccountSelection, "a")
        await client.completeRead(
            2,
            projection: codex(["a", "b", "c"], "c", 3))
        await latest.value
        XCTAssertEqual(store.usageAccountSelection, "c")
        XCTAssertEqual(model(store).content?.headAccount?.accountKey, "c")
        XCTAssertEqual(store.surfaces.first?.identity?.accountLabel, "c")
    }

    func testUnrelatedPopoverAccountChangeDoesNotCancelPendingUsageAccountIntent() async {
        let client = ControlledSelectionClient()
        let store = PresentationStore(scheduler: RefreshScheduler(), selectionClient: client)
        store.applyProjection(twoProviders("a", "x"), request: 0)
        store.selectUsageSurface("codex")
        guard
            let usage = store.setAccountSelection(
                surfaceId: "codex", accountKey: "b", navigateToUsage: true)
        else {
            return XCTFail("missing Usage selection task")
        }
        await client.waitForRead(1)
        guard
            let popover = store.setAccountSelection(
                surfaceId: "claude", accountKey: "y", navigateToUsage: false)
        else {
            return XCTFail("missing popover selection task")
        }
        await client.waitForRead(2)
        await client.completeRead(1, projection: twoProviders("b", "x", 2))
        await usage.value
        XCTAssertEqual(store.usageSelection, "codex")
        XCTAssertEqual(store.usageAccountSelection, "b")
        await client.completeRead(2, projection: twoProviders("b", "y", 3))
        await popover.value
        XCTAssertEqual(store.usageAccountSelection, "b")
        XCTAssertEqual(model(store).content?.headAccount?.accountKey, "b")
        XCTAssertNil(store.usageNotice)
    }

    func testUnrelatedPopoverAccountChangeKeepsRemovedDestinationNotice() async {
        let client = ControlledSelectionClient()
        let store = PresentationStore(scheduler: RefreshScheduler(), selectionClient: client)
        let initial = projection([
            provider("codex", keys: ["b"], selected: "a"),
            provider("claude", keys: ["x", "y"], selected: "x"),
        ])
        store.applyProjection(initial, request: 0)
        store.selectUsageSurface("codex")
        XCTAssertEqual(store.usageNotice, notice)
        guard
            let popover = store.setAccountSelection(
                surfaceId: "claude", accountKey: "y", navigateToUsage: false)
        else {
            return XCTFail("missing popover selection task")
        }
        await client.waitForRead(1)
        let changed = projection(
            [
                provider("codex", keys: ["b"], selected: "a"),
                provider("claude", keys: ["x", "y"], selected: "y"),
            ], generation: 2)
        await client.completeRead(1, projection: changed)
        await popover.value
        XCTAssertNil(store.usageSelection)
        XCTAssertEqual(store.usageNotice, notice)
    }

    func testMatchingPopoverAccountChoiceClearsRemovedDestinationNoticeWithoutNavigation() async {
        let client = ControlledSelectionClient()
        let store = PresentationStore(scheduler: RefreshScheduler(), selectionClient: client)
        store.applyProjection(codex(["b"], "a"), request: 0)
        store.selectUsageSurface("codex")
        XCTAssertEqual(store.usageNotice, notice)
        guard
            let popover = store.setAccountSelection(
                surfaceId: "codex", accountKey: "b", navigateToUsage: false)
        else {
            return XCTFail("missing popover selection task")
        }
        await client.waitForRead(1)
        XCTAssertEqual(store.usageNotice, notice)
        await client.completeRead(1, projection: codex(["b"], "b", 2))
        await popover.value
        XCTAssertNil(store.usageSelection)
        XCTAssertNil(store.usageNotice)
        XCTAssertEqual(store.accounts.first(where: \.selected)?.accountKey, "b")
    }

    func testPopoverRecoveryDoesNotClearNoticeWhenRequestedAccountDisappears() async {
        let client = ControlledSelectionClient()
        let store = PresentationStore(scheduler: RefreshScheduler(), selectionClient: client)
        store.applyProjection(codex(["b"], "a"), request: 0)
        store.selectUsageSurface("codex")
        guard
            let popover = store.setAccountSelection(
                surfaceId: "codex", accountKey: "b", navigateToUsage: false)
        else {
            return XCTFail("missing popover selection task")
        }
        await client.waitForRead(1)
        await client.completeRead(1, projection: codex([], "b", 2))
        await popover.value
        XCTAssertNil(store.usageSelection)
        XCTAssertEqual(store.usageNotice, notice)
        XCTAssertEqual(store.providerGroups.first?.lastError, notice)
        XCTAssertTrue(store.accounts.isEmpty)
    }

    func testExplicitOverviewAccountChoiceExplainsReplacementDisappearingWithSibling() async {
        let client = ControlledSelectionClient()
        let store = PresentationStore(scheduler: RefreshScheduler(), selectionClient: client)
        store.applyProjection(codex(["a", "b"], "a"), request: 0)
        XCTAssertNil(store.usageNotice)
        guard
            let selection = store.setAccountSelection(
                surfaceId: "codex", accountKey: "b", navigateToUsage: true)
        else {
            return XCTFail("missing Overview selection task")
        }
        await client.waitForRead(1)
        await client.completeRead(1, projection: codex(["a"], "b", 2))
        await selection.value
        XCTAssertNil(store.usageSelection)
        XCTAssertNil(model(store).content)
        XCTAssertEqual(store.usageNotice, notice)
        XCTAssertEqual(store.accounts.map(\.accountKey), ["a"])
        XCTAssertFalse(store.accounts.contains(where: \.selected))
    }

    func testNavigationFencesPendingSetterAndRejectsMismatchedPublishedIdentity() async {
        let client = ControlledSelectionClient()
        let store = PresentationStore(
            scheduler: RefreshScheduler(),
            selectionClient: client)
        store.applyProjection(
            codex(["a", "b"], "a", 1),
            request: 0)
        guard
            let pending = store.setAccountSelection(
                surfaceId: "codex",
                accountKey: "b",
                navigateToUsage: true)
        else {
            return XCTFail("missing production selection task")
        }
        await client.waitForRead(1)
        store.selectUsageSurface(nil)
        store.selectUsageContext(
            surfaceId: "codex",
            accountKey: "a")
        await client.completeRead(
            1,
            projection: codex(["a", "b"], "b", 2))
        await pending.value
        XCTAssertNil(store.usageSelection)
        XCTAssertNil(model(store).content)
        store.selectUsageSurface("codex")
        XCTAssertEqual(model(store).content?.headAccount?.accountKey, "b")
        XCTAssertEqual(store.surfaces.first?.identity?.accountLabel, "b")
    }

    func testFullInventoryNavigationUsesAllProvidersWithoutExpandingCompactSummary() {
        let ids = [
            "codex", "claude", "amp", "grok", "zai", "kimi", "minimax", "opencode", "google",
            "cursor", "meta", "openrouter",
        ]
        let store = PresentationStore()
        var publication = projection(
            ids.map { provider($0, keys: ["account"], selected: "account") })
        publication.glanceRows = Array(publication.glanceRows.prefix(7))
        store.applyProjection(publication, request: 1)
        XCTAssertEqual(model(store).sidebar.map(\.surfaceId), ids)
        XCTAssertEqual(store.providerGlanceRows.map(\.surfaceId), Array(ids.prefix(7)))
        for id in ids {
            store.selectUsageSurface(id)
            XCTAssertEqual(store.usageSelection, id)
            XCTAssertEqual(model(store).content?.surfaceId, id)
        }
    }

    func testNoncompactDisabledProviderKeepsDestinationAndHonestInventory() {
        let store = PresentationStore()
        var publication = projection([
            provider("openrouter", keys: ["account"], selected: "account")
        ])
        publication.glanceRows = []
        publication.surfaces[0].enabled = false
        store.applyProjection(publication, request: 1)
        store.selectUsageSurface("openrouter")
        XCTAssertFalse(model(store).isEmpty)
        XCTAssertEqual(store.usageSelection, "openrouter")
        XCTAssertEqual(model(store).content?.headAccount?.accountKey, "account")
        publication.generation = 2
        publication.surfaces[0].enabled = true
        store.applyProjection(publication, request: 2)
        XCTAssertEqual(store.usageSelection, "openrouter")
        store.applyProjectionFailureForTesting(NSError(domain: "synthetic", code: 1))
        XCTAssertEqual(model(store).sidebar.map(\.surfaceId), ["openrouter"])
        XCTAssertEqual(store.usageAccountSelection, "account")
    }

    func testUnresolvedNoncompactProviderIsSelectableAndTableSelectionReconcilesRemoval() {
        let store = PresentationStore()
        var provider = provider("google", keys: [], selected: "unused")
        provider.selectedAccountKey = nil
        provider.selectedUsage.lastError = "Credential required"
        provider.selectedUsage.status = "unavailable"
        var publication = projection([provider])
        publication.glanceRows = []
        publication.surfaces[0].enabled = false
        store.applyProjection(publication, request: 1)
        store.selectUsageSurface("google")
        XCTAssertEqual(store.usageSelection, "google")
        XCTAssertTrue(model(store).content?.accounts.isEmpty == true)
        XCTAssertEqual(store.surfaces.first?.lastError, "Credential required")
        store.overviewSelectionID = "provider#google"
        store.applyProjection(publication, request: 2)
        XCTAssertEqual(store.overviewSelectionID, "provider#google")
        store.applyProjection(projection([], generation: 2), request: 3)
        XCTAssertNil(store.overviewSelectionID)
        XCTAssertNil(store.usageSelection)
        XCTAssertTrue(model(store).isEmpty)
    }

    func testOverviewAccountHighlightRetainsValidKeyAndClearsRemovedKey() {
        let store = PresentationStore()
        store.applyProjection(codex(["a", "b"], "a"), request: 1)
        store.overviewSelectionID = "account#codex#b"
        store.applyProjection(codex(["b", "a"], "a", 2), request: 2)
        XCTAssertEqual(store.overviewSelectionID, "account#codex#b")
        store.applyProjection(codex(["a"], "a", 3), request: 3)
        XCTAssertNil(store.overviewSelectionID)
        XCTAssertNil(store.usageNotice)
    }

    func testLastAccountRemovalRendersNoticeAndRetryInNativeWindow() async {
        let restoreAccessibility = enableProcessAccessibility()
        defer { restoreAccessibility() }
        let store = PresentationStore()
        store.applyQIFixture(
            glanceRows: [],
            surfaces: [],
            accounts: [],
            providerGroups: [],
            popoverSelection: nil,
            usageSelection: nil)
        store.applyProjection(
            codex(["a"], "a", 1),
            request: 1)
        store.selectUsageSurface("codex")
        store.applyProjection(
            codex([], "a", 2),
            request: 2)
        let window = NSWindow(
            contentRect: NSRect(
                x: 0,
                y: 0,
                width: 920,
                height: 620),
            styleMask: [.titled, .resizable],
            backing: .buffered,
            defer: false)
        window.isReleasedWhenClosed = false
        let host = NSHostingView(rootView: UsageWindowDetail(store: store))
        window.contentView = host
        window.orderFront(nil)
        defer { window.close() }
        for _ in 0..<20 {
            host.layoutSubtreeIfNeeded()
            await Task.yield()
            let nodes = accessibilityNodes(host)
            if nodes.contains(where: {
                $0.identifier == "usage.overview.retry.codex"
            }) {
                break
            }
        }
        assertRemovalAccessibility(host)
    }

    func testMissingPopoverSelectionRendersUnavailableInsteadOfFirstSibling() async {
        let restoreAccessibility = enableProcessAccessibility()
        defer { restoreAccessibility() }
        let store = PresentationStore()
        store.applyQIFixture(
            glanceRows: [],
            surfaces: [],
            accounts: [],
            providerGroups: [],
            popoverSelection: nil,
            usageSelection: nil)
        store.applyProjection(codex(["b", "c"], "a"), request: 1)
        store.popoverSelection = "codex"
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 380, height: 520),
            styleMask: [.titled],
            backing: .buffered,
            defer: false)
        window.isReleasedWhenClosed = false
        let host = NSHostingView(rootView: PopoverRoot(store: store))
        window.contentView = host
        window.orderFront(nil)
        defer { window.close() }
        for _ in 0..<20 {
            host.layoutSubtreeIfNeeded()
            await Task.yield()
        }
        let nodes = accessibilityNodes(host)
        let picker = nodes.first { $0.identifier == "popover.account-picker" }
        XCTAssertNotNil(picker)
        let displayed = picker?.value ?? picker?.title
        XCTAssertEqual(displayed, notice)
        XCTAssertFalse(store.accounts.contains(where: \.selected))
    }

    func testCountOverviewRendersLiteralUnsignedRequestsAndUnknownReset() async {
        await assertQuotaOverview(
            countProvider(),
            expected: "OpenRouter, count-account, Ready, 18446744073709551615 requests left, —")
    }

    func testMoneyOverviewRendersExactLargeAmountAndNegativeRemaining() async {
        await assertQuotaOverview(
            moneyProvider(),
            expected:
                "OpenRouter, money-account, Ready, $90071992547409.93 / $5.00 spent · $-2.50 remaining, —"
        )
    }

    private func assertQuotaOverview(
        _ provider: DesktopProviderProjectionDto, expected: String
    ) async {
        let restoreAccessibility = enableProcessAccessibility()
        defer { restoreAccessibility() }
        let store = PresentationStore()
        store.applyQIFixture(
            glanceRows: [],
            surfaces: [],
            accounts: [],
            providerGroups: [],
            popoverSelection: nil,
            usageSelection: nil)
        store.applyProjection(projection([provider]), request: 1)
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 920, height: 620),
            styleMask: [.titled],
            backing: .buffered,
            defer: false)
        window.isReleasedWhenClosed = false
        let host = NSHostingView(rootView: UsageWindowDetail(store: store))
        window.contentView = host
        window.orderFront(nil)
        defer { window.close() }
        for _ in 0..<20 {
            host.layoutSubtreeIfNeeded()
            await Task.yield()
        }
        XCTAssertTrue(accessibilityNodes(host).contains { ($0.label ?? $0.value) == expected })
        XCTAssertEqual(
            store.accounts.first?.remainingLabel, provider.group.accounts.first?.remainingLabel)
        XCTAssertEqual(
            store.accounts.first?.countQuota,
            provider.group.accounts.first?.countQuota.map(PresentationStore.CountQuotaRow.init))
        XCTAssertEqual(store.accounts.first?.usedMoney, provider.group.accounts.first?.usedMoney)
        XCTAssertEqual(store.accounts.first?.limitMoney, provider.group.accounts.first?.limitMoney)
        XCTAssertEqual(
            store.accounts.first?.remainingMoney, provider.group.accounts.first?.remainingMoney)
        XCTAssertNil(store.accounts.first?.resetsAt)
    }

    private func countProvider() -> DesktopProviderProjectionDto {
        var result = provider("openrouter", keys: ["count-account"], selected: "count-account")
        var row = result.group.accounts[0]
        row.countQuota = CountQuotaDto(
            used: nil,
            limit: nil,
            remaining: UInt64.max,
            unit: "requests",
            period: "unknown",
            provenance: "provider_reported")
        row.remainingPercent = nil
        row.remainingLabel = "18446744073709551615 requests left"
        row.headline = row.remainingLabel
        row.resetsAt = nil
        row.resetDisplayLabel = "—"
        row.accessibilityLabel =
            "OpenRouter, count-account, Ready, 18446744073709551615 requests left, —"
        result.group.accounts = [row]
        return result
    }

    private func moneyProvider() -> DesktopProviderProjectionDto {
        var result = provider("openrouter", keys: ["money-account"], selected: "money-account")
        var row = result.group.accounts[0]
        row.usedMoney = MoneyDto(amountMinor: 9_007_199_254_740_993, currency: "USD", exponent: 2)
        row.limitMoney = MoneyDto(amountMinor: 500, currency: "USD", exponent: 2)
        row.remainingMoney = MoneyDto(amountMinor: -250, currency: "USD", exponent: 2)
        row.remainingPercent = nil
        row.remainingLabel = "$90071992547409.93 / $5.00 spent · $-2.50 remaining"
        row.headline = row.remainingLabel
        row.accessibilityLabel =
            "OpenRouter, money-account, Ready, $90071992547409.93 / $5.00 spent · $-2.50 remaining, —"
        result.group.accounts = [row]
        return result
    }

    func testOnlyNoncompactDisabledProviderRendersFullSidebarAndDetail() async {
        await assertNoncompactInventoryVisible(isLoading: false, error: nil)
    }

    func testRetainedNoncompactInventoryRemainsVisibleWhileOpening() async {
        await assertNoncompactInventoryVisible(isLoading: true, error: nil)
    }

    func testRetainedNoncompactInventoryRemainsVisibleAfterProjectionError() async {
        await assertNoncompactInventoryVisible(
            isLoading: false, error: "Synthetic projection failure")
    }

    private func assertNoncompactInventoryVisible(isLoading: Bool, error: String?) async {
        let restoreAccessibility = enableProcessAccessibility()
        defer { restoreAccessibility() }
        let store = PresentationStore()
        store.applyQIFixture(
            glanceRows: [],
            surfaces: [],
            accounts: [],
            providerGroups: [],
            popoverSelection: nil,
            usageSelection: nil,
            isLoading: isLoading)
        var publication = projection([
            provider("openrouter", keys: ["account"], selected: "account")
        ])
        publication.glanceRows = []
        publication.errorMessage = error
        publication.surfaces[0].enabled = false
        store.applyProjection(publication, request: 1)
        store.selectUsageSurface("openrouter")
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 920, height: 620),
            styleMask: [.titled],
            backing: .buffered,
            defer: false)
        window.isReleasedWhenClosed = false
        let host = NSHostingView(
            rootView: HStack {
                UsageWindowSidebar(store: store)
                UsageWindowDetail(store: store)
            })
        window.contentView = host
        window.orderFront(nil)
        defer { window.close() }
        for _ in 0..<20 {
            host.layoutSubtreeIfNeeded()
            await Task.yield()
        }
        assertNoncompactInventoryAccessibility(host, error: error)
    }

    private func assertNoncompactInventoryAccessibility(_ host: NSView, error: String?) {
        let nodes = accessibilityNodes(host)
        let identifiers = nodes.compactMap(\.identifier)
        XCTAssertTrue(identifiers.contains("usage.sidebar.provider.openrouter"))
        XCTAssertTrue(identifiers.contains("usage.provider.openrouter"))
        XCTAssertFalse(identifiers.contains("usage.global-error"))
        XCTAssertFalse(identifiers.contains("usage.loading"))
        if let error {
            let notice = nodes.first { $0.identifier == "usage.refresh-error" }
            XCTAssertEqual(notice?.label ?? notice?.value, error)
            XCTAssertTrue(
                nodes.contains { $0.identifier == "usage.refresh-retry" && $0.role == .button })
        }
    }

    private func assertRemovalAccessibility(_ host: NSView) {
        let nodes = accessibilityNodes(host)
        let renderedNotice = nodes.first {
            $0.identifier == "usage.overview.notice"
        }
        XCTAssertNotNil(renderedNotice)
        XCTAssertEqual(renderedNotice?.label ?? renderedNotice?.value, notice)
        XCTAssertTrue(
            nodes.contains {
                $0.identifier == "usage.overview.error.codex"
            })
        XCTAssertTrue(
            nodes.contains {
                $0.identifier == "usage.overview.retry.codex"
                    && $0.role == .button
            })
        XCTAssertFalse(
            nodes.contains {
                $0.identifier == "usage.provider.codex"
            })
    }

    private struct AccessibilitySnapshot {
        let identifier: String?
        let label: String?
        let role: NSAccessibility.Role?
        let value: String?
        let title: String?
    }

    // SwiftUI AccessibilityNode implements Objective-C getters without declaring
    // NSAccessibilityProtocol conformance. Snapshot the actual rendered nodes.
    private func accessibilityNodes(_ root: Any, depth: Int = 0) -> [AccessibilitySnapshot] {
        guard depth < 64, let node = root as? NSObject else { return [] }
        let role = accessibilityGetter(node, "accessibilityRole") as? String
        let snapshot = AccessibilitySnapshot(
            identifier: accessibilityGetter(node, "accessibilityIdentifier") as? String,
            label: accessibilityGetter(node, "accessibilityLabel") as? String,
            role: role.map { NSAccessibility.Role(rawValue: $0) },
            value: accessibilityGetter(node, "accessibilityValue") as? String,
            title: accessibilityGetter(node, "accessibilityTitle") as? String)
        let children = accessibilityGetter(node, "accessibilityChildren") as? [Any] ?? []
        return [snapshot] + children.flatMap { accessibilityNodes($0, depth: depth + 1) }
    }

    private func accessibilityGetter(_ node: NSObject, _ name: String) -> Any? {
        let selector = NSSelectorFromString(name)
        if node.responds(to: selector),
            let value = node.perform(selector)?.takeUnretainedValue()
        {
            return value
        }
        // Native Table row/column proxies expose the legacy AX attribute API.
        let attributes = [
            "accessibilityChildren": "AXChildren",
            "accessibilityIdentifier": "AXIdentifier",
            "accessibilityLabel": "AXDescription",
            "accessibilityRole": "AXRole",
            "accessibilityValue": "AXValue",
            "accessibilityTitle": "AXTitle",
        ]
        let attributeGetter = NSSelectorFromString("accessibilityAttributeValue:")
        guard let attribute = attributes[name], node.responds(to: attributeGetter) else {
            return nil
        }
        return node.perform(attributeGetter, with: attribute)?.takeUnretainedValue()
    }

    // Enable this test process's lazy SwiftUI AX tree; restore prior state.
    private func enableProcessAccessibility() -> () -> Void {
        let application = NSApplication.shared
        let attribute = "AXEnhancedUserInterface"
        let getter = NSSelectorFromString("accessibilityAttributeValue:")
        let setter = NSSelectorFromString("accessibilitySetValue:forAttribute:")
        let previous = application.perform(getter, with: attribute)?.takeUnretainedValue()
        _ = application.perform(setter, with: NSNumber(value: true), with: attribute)
        return {
            _ = application.perform(
                setter, with: previous ?? NSNumber(value: false), with: attribute)
        }
    }

    private func model(_ store: PresentationStore) -> UsageWindowModel {
        UsageWindowModel(
            providerGroups: store.providerGroups,
            surfaces: store.surfaces,
            accounts: store.accounts,
            selection: store.usageSelection,
            accountSelection: store.usageAccountSelection)
    }

    private func twoProviders(
        _ codexKey: String, _ claudeKey: String, _ generation: UInt64 = 1
    ) -> DesktopProjectionDto {
        projection(
            [
                provider("codex", keys: ["a", "b"], selected: codexKey),
                provider("claude", keys: ["x", "y"], selected: claudeKey),
            ], generation: generation)
    }

    private func codex(
        _ keys: [String], _ selected: String, _ generation: UInt64 = 1
    ) -> DesktopProjectionDto {
        projection([provider("codex", keys: keys, selected: selected)], generation: generation)
    }

    private func projection(
        _ providers: [DesktopProviderProjectionDto],
        generation: UInt64 = 1
    ) -> DesktopProjectionDto {
        DesktopProjectionDto(
            generation: generation,
            refreshInProgress: false,
            errorMessage: nil,
            nextRefreshLabel: "Next update due",
            surfaces: providers.map {
                SurfaceDescriptorDto(
                    id: $0.group.surfaceId,
                    label: $0.group.displayLabel,
                    agent: $0.group.surfaceId,
                    provider: nil,
                    enabled: true)
            },
            providers: providers,
            glanceRows: providers.map { glance($0.group.surfaceId) },
            statusBarGlanceRows: [],
            diagnostics: [])
    }

    private func provider(
        _ id: String,
        keys: [String],
        selected: String
    ) -> DesktopProviderProjectionDto {
        let unavailable = !keys.contains(selected)
        let error = unavailable ? notice : nil
        return DesktopProviderProjectionDto(
            group: DesktopProviderGroupDto(
                surfaceId: id,
                displayLabel: id,
                iconKey: id,
                fallbackGlyph: "?",
                usageUrl: nil,
                accountColumnLabel: "—",
                planOrStatusLabel: "—",
                remainingLabel: "—",
                resetDisplayLabel: "—",
                accessibilityLabel: id,
                accounts: keys.map {
                    account(
                        id,
                        key: $0,
                        selected: $0 == selected)
                },
                emptyState: keys.isEmpty
                    ? DesktopProviderStateDto(
                        statusWord: "unavailable",
                        statusLabel: "Unavailable",
                        updatedLabel: "Unavailable",
                        lastError: error,
                        isRefreshing: false)
                    : nil),
            selectedAccountKey: selected,
            selectedUsage: selectedUsage(
                id, selected: selected, unavailable: unavailable, error: error))
    }

    private func selectedUsage(
        _ id: String, selected: String, unavailable: Bool, error: String?
    ) -> UsageViewDto {
        UsageViewDto(
            identity: UsageIdentityPresentationDto(
                providerTitle: id,
                accountLabel: unavailable ? "" : selected,
                activityLabel: error ?? "Updated now",
                activityKind: unavailable ? "exceptional" : "idle",
                accessibilityLabel: id),
            focusedAgent: id,
            focusedProvider: nil,
            providerLabel: id,
            accountLabel: unavailable ? "" : selected,
            username: nil,
            planLabel: nil,
            credentialOrigin: nil,
            buckets: [],
            status: unavailable ? "unavailable" : "fresh",
            source: "api",
            confidence: "authoritative",
            fetchedAtEpoch: 1,
            updatedLabel: "Updated now",
            statusBarLabel: "",
            lastError: error,
            estimateCaption: nil,
            detailPresentation: UsageDetailPresentationDto(rows: []))
    }

    private func account(
        _ id: String,
        key: String,
        selected: Bool
    ) -> AccountDescriptorDto {
        AccountDescriptorDto(
            surfaceId: id,
            providerColumnLabel: "",
            accountKey: key,
            accountLabel: key,
            planLabel: nil,
            selected: selected,
            lifecycle: "current",
            lifecycleLabel: "Current",
            provenance: [],
            provenanceLabel: "Fixture",
            planOrStatusLabel: "Ready",
            remainingPercent: 50,
            remainingLabel: "50%",
            headline: "50% left",
            resetLabel: nil,
            resetDisplayLabel: "—",
            exactReset: nil,
            statusWord: "fresh",
            statusLabel: "Ready",
            severity: "normal",
            updatedLabel: "Updated now",
            lastError: nil,
            dimmed: false,
            accessibilityLabel: key,
            countQuota: nil,
            resetsAt: nil,
            usedMoney: nil,
            limitMoney: nil,
            remainingMoney: nil)
    }

    private func glance(_ id: String) -> ProviderGlanceRowDto {
        ProviderGlanceRowDto(
            surfaceId: id,
            iconKey: id,
            fallbackGlyph: "?",
            usageUrl: nil,
            displayLabel: id,
            accountLabel: "",
            planLabel: nil,
            glanceRemainingPercent: nil,
            barLabel: "",
            headline: "—",
            resetLabel: nil,
            compactResetLabel: nil,
            exactReset: nil,
            statusWord: "fresh",
            isRefreshing: false,
            statusLabel: "Ready",
            severity: "normal",
            updatedLabel: "Updated now",
            activityLabel: "Updated now",
            activityKind: "idle",
            accessibilityLabel: id,
            lastError: nil,
            dimmed: false)
    }
}

private actor ControlledSelectionClient: UsageSelectionClient {
    private var readCount = 0
    private var reads: [Int: CheckedContinuation<DesktopProjectionDto, Never>] = [:]
    private var waiters: [Int: CheckedContinuation<Void, Never>] = [:]

    func setSelectedAccount(
        surfaceId: String,
        accountKey: String
    ) async throws {}

    func desktopProjection(statusBarMax: UInt32) async throws -> DesktopProjectionDto {
        readCount += 1
        let read = readCount
        return await withCheckedContinuation { continuation in
            reads[read] = continuation
            waiters.removeValue(forKey: read)?.resume()
        }
    }

    func waitForRead(_ read: Int) async {
        guard reads[read] == nil else { return }
        await withCheckedContinuation { waiters[read] = $0 }
    }

    func completeRead(
        _ read: Int,
        projection: DesktopProjectionDto
    ) {
        reads.removeValue(forKey: read)?.resume(returning: projection)
    }
}
