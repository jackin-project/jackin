// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import XCTest

extension JackinDesktopUITests {
    func testFocusedPopoverUsesRealHost() {
        defer { application.terminate() }
        guard launchPopover(fixture: "F03-multi-account", selection: "codex") else { return }

        let popover = application.popovers.firstMatch
        XCTAssertTrue(popover.exists)
        let brand = application.staticTexts["jackin❯ desktop"]
        XCTAssertTrue(brand.exists)
        XCTAssertEqual(brand.label, "jackin❯ desktop")
        let accountPicker = element("popover.account-picker")
        let providerIdentity = element("popover.provider-identity")
        let refresh = element("popover.refresh")
        let openUsage = element("popover.open-usage")
        XCTAssertTrue(accountPicker.exists)
        XCTAssertTrue(providerIdentity.exists)
        XCTAssertTrue(refresh.exists)
        XCTAssertTrue(openUsage.exists)
        XCTAssertEqual(accountPicker.elementType, .popUpButton)
        XCTAssertEqual(refresh.elementType, .button)
        XCTAssertEqual(openUsage.elementType, .button)
        XCTAssertEqual(refresh.label, "Refresh")
        XCTAssertEqual(openUsage.label, "Open Usage")
        XCTAssertTrue(popover.frame.intersects(providerIdentity.frame))
        XCTAssertGreaterThan(accountPicker.frame.minX, refresh.frame.maxX)
        XCTAssertGreaterThan(accountPicker.frame.minX, openUsage.frame.maxX)
    }

    func testPopoverRoutesProviderContextIntoUsage() {
        defer { application.terminate() }
        guard launchPopover(fixture: "F03-multi-account", selection: "codex") else { return }

        let openUsage = element("popover.open-usage")
        XCTAssertTrue(openUsage.waitForExistence(timeout: 5))
        openUsage.click()

        XCTAssertTrue(application.windows["usage-window"].waitForExistence(timeout: 5))
        XCTAssertTrue(element("usage.provider.codex").waitForExistence(timeout: 3))
        XCTAssertTrue(element("usage.account-picker").exists)
    }

    func testRetainedUsageWindowPreservesContextAcrossCloseAndReopen() {
        defer { application.terminate() }
        guard launchUsage(fixture: "F03-multi-account", selection: "codex", size: "920x620")
        else { return }
        XCTAssertTrue(ensureUsageWindowVisible(contentIdentifier: "usage.provider.codex"))

        let usageWindow = application.windows.firstMatch
        XCTAssertTrue(element("usage.provider.codex").waitForExistence(timeout: 5))
        let accountPicker = element("usage.account-picker")
        XCTAssertTrue(accountPicker.waitForExistence(timeout: 3))
        let expectedAccount = accountPicker.value as? String
        XCTAssertNotNil(expectedAccount)
        let toggle = sidebarToggle(label: "Hide Sidebar", in: usageWindow)
        XCTAssertEqual(toggle.label, "Hide Sidebar")
        toggle.click()
        XCTAssertTrue(ensureUsageWindowVisible(contentIdentifier: "usage.provider.codex"))
        XCTAssertTrue(
            sidebarToggle(label: "Show Sidebar", in: usageWindow).waitForExistence(timeout: 3))
        let expectedFrame = usageWindow.frame

        let close = usageWindow.buttons["_XCUI:CloseWindow"]
        XCTAssertTrue(close.isHittable)
        close.click()
        XCTAssertTrue(usageWindow.waitForNonExistence(timeout: 3))
        DistributedNotificationCenter.default().postNotificationName(
            Notification.Name("com.jackin-project.desktop.visual-qa.show-usage"),
            object: nil,
            userInfo: nil,
            deliverImmediately: true
        )

        XCTAssertTrue(usageWindow.waitForExistence(timeout: 5), application.debugDescription)
        XCTAssertTrue(element("usage.provider.codex").waitForExistence(timeout: 3))
        XCTAssertEqual(element("usage.account-picker").value as? String, expectedAccount)
        XCTAssertTrue(sidebarToggle(label: "Show Sidebar", in: usageWindow).exists)
        XCTAssertEqual(usageWindow.frame.origin.x, expectedFrame.origin.x, accuracy: 1)
        XCTAssertEqual(usageWindow.frame.origin.y, expectedFrame.origin.y, accuracy: 1)
        XCTAssertEqual(usageWindow.frame.size.width, expectedFrame.size.width, accuracy: 1)
        XCTAssertEqual(usageWindow.frame.size.height, expectedFrame.size.height, accuracy: 1)
    }

    func testMaximumContentRemainsScrollableAtMinimumSize() {
        defer { application.terminate() }
        guard launchUsage(fixture: "F12-layout-envelope", selection: "claude", size: "760x500")
        else { return }

        let provider = element("usage.provider.claude")
        let lastLimit = element("usage.limit.bucket:layout-long")
        XCTAssertTrue(lastLimit.waitForExistence(timeout: 3))
        XCTAssertTrue(scroll(lastLimit, intoViewThrough: provider))
    }

    func testMaximumPopoverContentRemainsScrollable() {
        defer { application.terminate() }
        guard launchPopover(fixture: "F12-layout-envelope", selection: "claude") else { return }

        let provider = element("popover.provider.claude")
        let lastLimit = element("popover.limit.bucket:layout-long")
        let refresh = element("popover.refresh")
        let openUsage = element("popover.open-usage")
        XCTAssertTrue(lastLimit.waitForExistence(timeout: 3))
        let controlsReady = ensurePopoverControlsHittable([refresh, openUsage])
        XCTAssertTrue(controlsReady, application.debugDescription)
        guard controlsReady else { return }
        XCTAssertTrue(scroll(lastLimit, intoViewThrough: provider))
        application.activate()
        XCTAssertTrue(lastLimit.waitForHittable(timeout: 3))
        XCTAssertTrue(refresh.waitForHittable(timeout: 3))
        XCTAssertTrue(openUsage.waitForHittable(timeout: 3))

        DistributedNotificationCenter.default().postNotificationName(
            Notification.Name("com.jackin-project.desktop.visual-qa.close-popover"),
            object: nil,
            userInfo: nil,
            deliverImmediately: true
        )
        XCTAssertTrue(application.popovers.firstMatch.waitForNonExistence(timeout: 3))
        DistributedNotificationCenter.default().postNotificationName(
            Notification.Name("com.jackin-project.desktop.visual-qa.show-popover"),
            object: nil,
            userInfo: nil,
            deliverImmediately: true
        )

        let reopenedPopover = application.popovers.firstMatch
        let providerIdentity = element("popover.provider-identity")
        XCTAssertTrue(reopenedPopover.waitForExistence(timeout: 5))
        XCTAssertTrue(providerIdentity.waitForExistence(timeout: 3))
        XCTAssertTrue(reopenedPopover.frame.intersects(providerIdentity.frame))
    }

    func testStandardCommandsAndMenusShareNativeState() {
        defer { application.terminate() }
        guard
            launchUsage(
                fixture: "F02-catalog-normal",
                selection: "overview",
                size: "920x620",
                accessoryFixture: false
            )
        else { return }

        application.menuBars.menuBarItems["jackin❯ desktop"].click()
        application.menuItems["Settings…"].click()
        let settingsWindow = application.windows["settings-window"]
        XCTAssertTrue(
            settingsWindow.waitForExistence(timeout: 3),
            application.debugDescription
        )
        application.menuBars.menuBarItems["File"].click()
        application.menuItems["Close Window"].click()
        XCTAssertTrue(settingsWindow.waitForNonExistence(timeout: 3))

        let usageWindow = application.windows["usage-window"]
        usageWindow.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.03)).click()
        let nativeToggle = sidebarToggle(label: "Hide Sidebar", in: usageWindow)
        XCTAssertEqual(nativeToggle.label, "Hide Sidebar")

        application.menuBars.menuBarItems["View"].click()
        application.menuItems["Hide Sidebar"].click()
        XCTAssertTrue(
            sidebarToggle(label: "Show Sidebar", in: usageWindow).waitForExistence(timeout: 3),
            application.debugDescription)
        application.menuBars.menuBarItems["View"].click()
        application.menuItems["Show Sidebar"].click()
        XCTAssertTrue(
            sidebarToggle(label: "Hide Sidebar", in: usageWindow).waitForExistence(timeout: 3))

        XCTAssertTrue(toggleSidebarWithShortcut(in: usageWindow, expecting: "Show Sidebar"))
        XCTAssertTrue(toggleSidebarWithShortcut(in: usageWindow, expecting: "Hide Sidebar"))

        application.menuBars.menuBarItems["View"].click()
        application.menuItems["Refresh"].click()
        XCTAssertTrue(usageWindow.exists)
        XCTAssertTrue(element("usage.refresh").waitForEnabled(timeout: 8))
    }

    func testFocusedPopoverPassesAccessibilityAudit() throws {
        defer { application.terminate() }
        guard launchPopover(fixture: "F03-multi-account", selection: "codex") else { return }
        application.activate()
        XCTAssertTrue(element("popover.provider.codex").waitForExistence(timeout: 3))

        let limitElements = application.descendants(matching: .any).matching(
            NSPredicate(format: "identifier BEGINSWITH %@", "popover.limit.")
        ).allElementsBoundByIndex
        let limitDescriptions = limitElements.flatMap { limit in
            [limit.label, limit.value as? String].compactMap { $0 }
        }.filter { !$0.isEmpty }
        XCTAssertFalse(limitDescriptions.isEmpty)

        let refresh = element("popover.refresh")
        let openUsage = element("popover.open-usage")
        let accountPicker = element("popover.account-picker")
        let providerIdentity = element("popover.provider-identity")
        let refreshVerified =
            refresh.exists && refresh.isEnabled && refresh.elementType == .button
            && refresh.label == "Refresh"
        let openUsageVerified =
            openUsage.exists && openUsage.isEnabled && openUsage.elementType == .button
            && openUsage.label == "Open Usage"
        let pickerVerified =
            accountPicker.exists && accountPicker.isEnabled
            && accountPicker.elementType == .popUpButton
            && !accountPicker.label.isEmpty
        let snapshot = PopoverAuditSnapshot(
            limitDescriptions: limitDescriptions,
            systemHostVerified: application.popovers.firstMatch.exists,
            controlsVerified:
                refreshVerified && openUsageVerified && pickerVerified
                && providerIdentity.exists && !providerIdentity.label.isEmpty
        )
        XCTAssertTrue(snapshot.systemHostVerified)
        XCTAssertTrue(snapshot.controlsVerified)

        try application.performAccessibilityAudit { issue in
            self.handlesSystemAccessibilityAuditFalsePositive(
                issue,
                popoverSnapshot: snapshot
            )
        }
    }
}
