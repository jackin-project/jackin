// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import XCTest

@testable import JackinDesktopUI
@testable import JackinUsageBridge

final class PopoverPresentationTests: XCTestCase {
    private var popoverSource: String {
        get throws {
            let url = URL(fileURLWithPath: #filePath)
                .deletingLastPathComponent()
                .deletingLastPathComponent()
                .deletingLastPathComponent()
                .appendingPathComponent("Sources/JackinDesktop/PopoverRoot.swift")
            return try String(contentsOf: url, encoding: .utf8)
        }
    }

    private var providerDetailsSource: String {
        get throws {
            let url = URL(fileURLWithPath: #filePath)
                .deletingLastPathComponent()
                .deletingLastPathComponent()
                .deletingLastPathComponent()
                .appendingPathComponent("Sources/JackinDesktop/PopoverProviderDetails.swift")
            return try String(contentsOf: url, encoding: .utf8)
        }
    }

    private var statusBarControllerSource: String {
        get throws {
            let url = URL(fileURLWithPath: #filePath)
                .deletingLastPathComponent()
                .deletingLastPathComponent()
                .deletingLastPathComponent()
                .appendingPathComponent("Sources/JackinDesktop/StatusBarController.swift")
            return try String(contentsOf: url, encoding: .utf8)
        }
    }

    func testPopoverContentOrderAndFooterPlacement() throws {
        let root = try popoverSource
        let details = try providerDetailsSource
        let identity = try XCTUnwrap(details.range(of: "Section { providerIdentity }"))
        let limits = try XCTUnwrap(details.range(of: "limitsSection(limitRows, surface: surface)"))
        let metadata = try XCTUnwrap(details.range(of: "detailsSection(metadataRows)"))
        let identityEnd = try XCTUnwrap(details.range(of: "private func limitsSection"))
        let controls = try XCTUnwrap(root.range(of: "private var controls"))
        let formSource = details[identity.lowerBound..<identityEnd.lowerBound]
        let controlsSource = root[controls.lowerBound...]

        XCTAssertLessThan(identity.lowerBound, limits.lowerBound)
        XCTAssertLessThan(limits.lowerBound, metadata.lowerBound)
        XCTAssertFalse(formSource.contains("Picker("))
        XCTAssertTrue(controlsSource.contains("HStack(spacing: 4)"))
        XCTAssertTrue(
            controlsSource.contains(
                "Label(\"Refresh\", systemImage: \"arrow.clockwise\")\n"
                    + "                        .labelStyle(.iconOnly)"
            )
        )
        XCTAssertTrue(
            controlsSource.contains(
                "Label(\"Open Usage\", systemImage: \"macwindow\")\n"
                    + "                        .labelStyle(.iconOnly)"
            )
        )
        XCTAssertFalse(controlsSource.contains("\n            .labelStyle(.iconOnly)"))
        XCTAssertTrue(controlsSource.contains("Spacer(minLength: 12)"))
        XCTAssertTrue(controlsSource.contains("Picker("))
        XCTAssertTrue(controlsSource.contains(".labelsHidden()"))
        XCTAssertFalse(details.contains("ScrollViewReader { proxy in"))
        XCTAssertTrue(
            details.contains("@State private var scrollPosition = ScrollPosition(edge: .top)")
        )
        XCTAssertTrue(details.contains(".scrollPosition($scrollPosition)"))
        XCTAssertTrue(details.contains(".defaultScrollAnchor(.top, for: .initialOffset)"))
        XCTAssertTrue(details.contains(".task(id: reset)"))
        XCTAssertTrue(
            details.contains(
                "presentationState.claimScrollReset(accountLabel: reset.accountLabel)"
            )
        )
        XCTAssertFalse(details.contains(".task(id: presentationState.sequence)"))
        XCTAssertFalse(details.contains(".task(id: provider.accountLabel)"))
        XCTAssertTrue(details.contains("scrollPosition.scrollTo(edge: .top)"))
        XCTAssertFalse(details.contains(".scrollPosition(id:"))
        XCTAssertTrue(controlsSource.contains(".help(\"Refresh\")"))
        XCTAssertTrue(controlsSource.contains(".help(\"Open Usage\")"))
        XCTAssertTrue(controlsSource.contains(".help(\"Choose account\")"))
    }

    @MainActor
    func testScrollResetClaimSurvivesViewRemounts() {
        let state = PopoverPresentationState()

        XCTAssertTrue(state.claimScrollReset(accountLabel: "first"))
        XCTAssertFalse(state.claimScrollReset(accountLabel: "first"))
        XCTAssertTrue(state.claimScrollReset(accountLabel: "second"))
        XCTAssertFalse(state.claimScrollReset(accountLabel: "second"))
        state.beginPresentation()
        XCTAssertTrue(state.claimScrollReset(accountLabel: "second"))
    }

    func testPopoverResetsScrollOnlyAfterExplicitNativePresentation() throws {
        let source = try statusBarControllerSource
        let toggle = try XCTUnwrap(source.range(of: "private func togglePopover"))
        let reset = try XCTUnwrap(
            source.range(of: "private func resetPopoverScrollAfterPresentation")
        )
        let toggleBody = source[toggle.lowerBound..<reset.lowerBound]
        let resetBody = source[reset.lowerBound...]

        XCTAssertFalse(source.contains("NSPopoverDelegate"))
        XCTAssertFalse(source.contains("popoverDidShow"))
        XCTAssertEqual(
            toggleBody.components(separatedBy: "resetPopoverScrollAfterPresentation()").count,
            3
        )
        XCTAssertTrue(resetBody.contains("DispatchQueue.main.async"))
        XCTAssertTrue(resetBody.contains("popoverPresentationState.beginPresentation()"))
    }

    func testPopoverActivatesBeforePresentingFromSecondaryDisplayStatusItem() throws {
        let source = try statusBarControllerSource
        let toggle = try XCTUnwrap(source.range(of: "private func togglePopover"))
        let show = try XCTUnwrap(source.range(of: "func showPopover"))
        let body = source[toggle.lowerBound..<show.lowerBound]
        let activation = try XCTUnwrap(body.range(of: "NSApp.activate(ignoringOtherApps: true)"))
        let presentation = try XCTUnwrap(
            body.range(of: "popover.show(relativeTo: sender.bounds, of: sender")
        )
        let keying = try XCTUnwrap(
            body.range(of: "popover.contentViewController?.view.window?.makeKey()")
        )

        XCTAssertLessThan(activation.lowerBound, presentation.lowerBound)
        XCTAssertLessThan(presentation.lowerBound, keying.lowerBound)
    }

    @MainActor
    func testSingleAccountIdentityNeedsNoPicker() {
        let fixture = VisualQAFixtures.fixture(id: .singleNormal)
        XCTAssertEqual(fixture.accounts.count, 1)
        XCTAssertEqual(
            fixture.surfaces.first?.identity?.accountLabel,
            fixture.accounts.first?.accountLabel
        )
    }

    @MainActor
    func testExactAccountSelectionUpdatesIdentityAndUsageHandoff() {
        let fixture = VisualQAFixtures.fixture(id: .multiAccount)
        let store = PresentationStore()
        store.applyQIFixture(fixture.input)

        store.setSelectedAccount(surfaceId: "codex", accountKey: "codex-organization")
        let context = UsageNavigationContext(
            surfaceId: "codex",
            accountKey: "codex-organization"
        )
        store.selectUsageContext(
            surfaceId: context.surfaceId,
            accountKey: context.accountKey
        )

        XCTAssertEqual(
            store.surfaces.first?.identity?.accountLabel,
            "organization-production-sandbox@example.test")
        XCTAssertEqual(store.providerGlanceRows.first?.glanceRemainingPercent, 88)
        XCTAssertEqual(store.usageSelection, "codex")
        XCTAssertEqual(store.usageAccountSelection, "codex-organization")
    }
}
