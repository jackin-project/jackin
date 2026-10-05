// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit
import JackinUsageBindings
import SwiftUI
import XCTest

@testable import JackinDesktopUI
@testable import JackinUsageBridge

@MainActor
final class PopoverRetainedFailureTests: XCTestCase {
    private let notice = "Selected account is no longer available."
    private let failure = "Usage could not be updated. Try again."

    private func fixtureStore(compactRows: Bool = true) -> PresentationStore {
        let fixture = VisualQAFixtures.fixture(id: .multiAccount)
        let store = PresentationStore()
        store.applyQIFixture(
            glanceRows: compactRows ? fixture.glanceRows : [],
            statusBarGlanceRows: compactRows ? fixture.statusGlanceRows : [],
            surfaces: fixture.surfaces,
            accounts: fixture.accounts,
            providerGroups: fixture.providerGroups,
            refreshingProjection: fixture.refreshingProjection,
            accountProjections: fixture.accountProjections,
            popoverSelection: fixture.popoverSelection,
            usageSelection: fixture.usageSelection
        )
        return store
    }

    func testRetainedFailureKeepsFullInventoryAndDistinctError() throws {
        let store = fixtureStore(compactRows: false)
        let surfaces = store.surfaces
        let accounts = store.accounts
        let groups = store.providerGroups
        store.applyProjectionFailureForTesting(NSError(domain: "fixture", code: 1))
        let content = try XCTUnwrap(StatusPopoverFocus.content(
            providerGroups: store.providerGroups, surfaces: store.surfaces,
            accounts: store.accounts, glanceRows: store.providerGlanceRows,
            selection: store.popoverSelection, refreshError: store.lastError))
        XCTAssertEqual(store.surfaces, surfaces)
        XCTAssertEqual(store.accounts, accounts)
        XCTAssertEqual(store.providerGroups, groups)
        XCTAssertEqual(content.surface, surfaces.first)
        XCTAssertEqual(content.accounts, accounts)
        XCTAssertNil(content.glance)
        XCTAssertEqual(content.refreshError, failure)
        XCTAssertFalse(content.surface.detailPresentation.rows.isEmpty)
    }

    func testHostedRetainedFailureRendersValuesAndInvokesGlobalRetry() async throws {
        try await assertHostedRetainedFailure(compactRows: false)
    }

    func testHostedOriginalCompactInventoryRetainsErrorAndValues() async throws {
        try await assertHostedRetainedFailure(compactRows: true)
    }

    func testHostedBusyRetryDisablesActionWithoutHidingRetainedValues() async throws {
        try await assertHostedRetainedFailure(compactRows: true, isBusy: true)
    }

    private func assertHostedRetainedFailure(compactRows: Bool, isBusy: Bool = false) async throws {
        let restore = enableAccessibility()
        defer { restore() }
        let store = PresentationStore()
        var dto = codex(["a", "b"], "a")
        if !compactRows { dto.glanceRows = [] }
        dto.refreshInProgress = isBusy
        store.applyProjection(dto, request: 1)
        store.popoverSelection = "codex"
        let rows = try XCTUnwrap(store.surfaces.first).detailPresentation.rows
        store.applyProjectionFailureForTesting(NSError(domain: "fixture", code: 1))
        var retries = 0
        let view = PopoverRoot(
            store: store, presentationState: PopoverPresentationState(),
            onRetryLastOperation: { retries += 1 }
        ).environment(\.popoverQIFullPlate, true)
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 380, height: 1100),
            styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        let host = NSHostingView(rootView: view)
        window.contentView = host
        window.orderFront(nil)
        defer { window.close() }
        for _ in 0..<20 {
            host.layoutSubtreeIfNeeded()
            await Task.yield()
        }
        let nodes = accessibilityNodes(host)
        let error = try XCTUnwrap(nodes.first {
            getter($0, "accessibilityIdentifier") as? String == "popover.refresh-error"
        })
        XCTAssertEqual(
            getter(error, "accessibilityLabel") as? String
                ?? getter(error, "accessibilityValue") as? String,
            failure)
        XCTAssertFalse(nodes.contains {
            getter($0, "accessibilityIdentifier") as? String == "popover.global-error"
        })
        for row in rows where row.kind == .bucket {
            let node = try XCTUnwrap(nodes.first {
                getter($0, "accessibilityIdentifier") as? String == "popover.limit.\(row.rowId)"
            })
            XCTAssertEqual(getter(node, "accessibilityLabel") as? String,
                           "\(row.label), \(row.displayLabel)")
        }
        let retry = try XCTUnwrap(nodes.first {
            getter($0, "accessibilityIdentifier") as? String == "popover.refresh-retry"
        })
        XCTAssertEqual(getter(retry, "accessibilityRole") as? String,
                       NSAccessibility.Role.button.rawValue)
        let press = NSSelectorFromString("accessibilityPerformPress")
        XCTAssertTrue(retry.responds(to: press))
        XCTAssertEqual(booleanAction(retry, selector: NSSelectorFromString("accessibilityEnabled")), !isBusy)
        _ = booleanAction(retry, selector: press)
        await Task.yield()
        XCTAssertEqual(retries, isBusy ? 0 : 1)
        XCTAssertEqual(store.refreshInProgress, isBusy,
                       "Global Retry must not route to provider refresh")
    }

    func testHostedProviderRetryStartsProviderRefreshAlongsideRetainedFailure() async throws {
        let restore = enableAccessibility()
        defer { restore() }
        let fixture = VisualQAFixtures.fixture(id: .multiAccount)
        var surfaces = fixture.surfaces
        surfaces[0].lastError = failure
        let store = PresentationStore()
        store.applyQIFixture(
            glanceRows: fixture.glanceRows, surfaces: surfaces, accounts: fixture.accounts,
            providerGroups: fixture.providerGroups,
            refreshingProjection: fixture.refreshingProjection,
            popoverSelection: fixture.popoverSelection, usageSelection: fixture.usageSelection,
            lastError: failure)
        var globalRetries = 0
        let host = NSHostingView(rootView: PopoverRoot(
            store: store, presentationState: PopoverPresentationState(),
            onRetryLastOperation: { globalRetries += 1 }
        ).environment(\.popoverQIFullPlate, true))
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 380, height: 1100),
            styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = host
        window.orderFront(nil)
        defer { window.close() }
        for _ in 0..<20 {
            host.layoutSubtreeIfNeeded()
            await Task.yield()
        }
        let nodes = accessibilityNodes(host)
        XCTAssertEqual(nodes.filter {
            getter($0, "accessibilityIdentifier") as? String == "popover.provider-error"
        }.count, 1)
        XCTAssertFalse(nodes.contains {
            getter($0, "accessibilityIdentifier") as? String == "popover.refresh-error"
        }, "Equal messages render once; each retry authority remains reachable")
        XCTAssertTrue(nodes.contains {
            getter($0, "accessibilityIdentifier") as? String == "popover.refresh-retry"
        })
        let retry = try XCTUnwrap(nodes.first {
            getter($0, "accessibilityIdentifier") as? String == "popover.provider-retry"
        })
        let press = NSSelectorFromString("accessibilityPerformPress")
        XCTAssertTrue(retry.responds(to: press))
        _ = booleanAction(retry, selector: press)
        await Task.yield()
        XCTAssertTrue(store.refreshInProgress)
        XCTAssertEqual(globalRetries, 0)
        XCTAssertFalse(store.surfaces.first?.detailPresentation.rows.isEmpty ?? true)
    }

    func testPublishedDTOFailurePreservesIdentityDetailsAndAccountTypes() throws {
        let store = PresentationStore()
        store.applyProjection(codex(["a", "b"], "a"), request: 1)
        let surfaces = store.surfaces
        let accounts = store.accounts
        let glances = store.providerGlanceRows
        store.applyProjectionFailureForTesting(NSError(domain: "fixture", code: 1))
        XCTAssertEqual(store.surfaces, surfaces)
        XCTAssertEqual(store.accounts, accounts)
        XCTAssertEqual(store.providerGlanceRows, glances)
        let content = try XCTUnwrap(StatusPopoverFocus.content(
            providerGroups: store.providerGroups, surfaces: store.surfaces,
            accounts: store.accounts, glanceRows: store.providerGlanceRows,
            selection: "codex", refreshError: store.lastError))
        XCTAssertEqual(content.accountLabel, "a")
        XCTAssertEqual(content.activityLabel, "Updated now")
        XCTAssertEqual(content.surface.detailPresentation.rows.first?.displayLabel,
                       "9007199254740993 requests left")
        XCTAssertNil(content.accounts.first?.countQuota)
        XCTAssertNil(content.accounts.first?.resetsAt)
        XCTAssertEqual(content.refreshError, failure)
    }

    func testEqualFailureMessagesPreserveBothRetryAuthorities() throws {
        let store = fixtureStore()
        var surfaces = store.surfaces
        surfaces[0].lastError = failure
        let content = try XCTUnwrap(StatusPopoverFocus.content(
            providerGroups: store.providerGroups, surfaces: surfaces,
            accounts: store.accounts, glanceRows: store.providerGlanceRows,
            selection: store.popoverSelection, refreshError: failure))
        XCTAssertEqual(content.lastError, failure)
        XCTAssertEqual(content.refreshError, failure)
        let different = try XCTUnwrap(StatusPopoverFocus.content(
            providerGroups: store.providerGroups, surfaces: surfaces,
            accounts: store.accounts, glanceRows: store.providerGlanceRows,
            selection: store.popoverSelection, refreshError: "A distinct bridge failure"))
        XCTAssertEqual(different.refreshError, "A distinct bridge failure")
    }

    private func accessibilityNodes(_ root: Any, depth: Int = 0) -> [NSObject] {
        guard depth < 64, let node = root as? NSObject else { return [] }
        let children = getter(node, "accessibilityChildren") as? [Any] ?? []
        return [node] + children.flatMap { accessibilityNodes($0, depth: depth + 1) }
    }

    // BOOL getters/actions require their actual Objective-C ABI; NSObject.perform
    // treats the result as an object and cannot safely read these selectors.
    private func booleanAction(_ node: NSObject, selector: Selector) -> Bool? {
        guard node.responds(to: selector) else { return nil }
        typealias BooleanMethod = @convention(c) (AnyObject, Selector) -> Bool
        let method = unsafeBitCast(node.method(for: selector), to: BooleanMethod.self)
        return method(node, selector)
    }

    private func getter(_ node: NSObject, _ name: String) -> Any? {
        let selector = NSSelectorFromString(name)
        guard node.responds(to: selector) else { return nil }
        return node.perform(selector)?.takeUnretainedValue()
    }

    private func enableAccessibility() -> () -> Void {
        let app = NSApplication.shared
        let attribute = "AXEnhancedUserInterface"
        let get = NSSelectorFromString("accessibilityAttributeValue:")
        let set = NSSelectorFromString("accessibilitySetValue:forAttribute:")
        let previous = app.perform(get, with: attribute)?.takeUnretainedValue()
        _ = app.perform(set, with: NSNumber(value: true), with: attribute)
        return { _ = app.perform(set, with: previous ?? NSNumber(value: false), with: attribute) }
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
            detailPresentation: UsageDetailPresentationDto(rows: [
                UsageDetailRowDto(
                    rowId: "requests", kind: "bucket", label: "Requests",
                    layoutLines: [UsagePresentationLineDto(
                        leading: "9007199254740993 requests left", trailing: nil)],
                    displayLabel: "9007199254740993 requests left",
                    meterPercent: nil, severity: "normal")
            ]))
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
