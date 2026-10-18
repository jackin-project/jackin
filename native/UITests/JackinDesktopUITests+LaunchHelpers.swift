// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import XCTest

extension JackinDesktopUITests {
    func launchPopover(fixture: String, selection: String) -> Bool {
        application.launchArguments = [
            "--fixture", fixture,
            "--ui-test",
            "--open-popover",
            "--selection", selection,
        ]
        application.launch()
        var opened = element("popover.provider.\(selection)").waitForExistence(timeout: 4)
        if !opened {
            DistributedNotificationCenter.default().postNotificationName(
                Notification.Name("com.jackin-project.desktop.visual-qa.show-popover"),
                object: nil,
                userInfo: nil,
                deliverImmediately: true
            )
            opened = element("popover.provider.\(selection)").waitForExistence(timeout: 8)
        }
        XCTAssertTrue(opened, application.debugDescription)
        guard opened else { return false }
        application.activate()
        return true
    }

    func element(_ identifier: String) -> XCUIElement {
        application.descendants(matching: .any)[identifier]
    }

    func scroll(
        _ target: XCUIElement,
        intoViewThrough container: XCUIElement
    ) -> Bool {
        for _ in 0..<8 {
            if target.isHittable { return true }
            guard container.waitForHittable(timeout: 3) else {
                application.activate()
                continue
            }
            container.scroll(byDeltaX: 0, deltaY: -1_200)
        }
        return target.waitForHittable(timeout: 3)
    }

    func ensurePopoverControlsHittable(_ controls: [XCUIElement]) -> Bool {
        for _ in 0..<3 {
            application.activate()
            if controls.allSatisfy({ $0.waitForHittable(timeout: 2) }) {
                return true
            }
            DistributedNotificationCenter.default().postNotificationName(
                Notification.Name("com.jackin-project.desktop.visual-qa.show-popover"),
                object: nil,
                userInfo: nil,
                deliverImmediately: true
            )
            _ = application.popovers.firstMatch.waitForExistence(timeout: 2)
        }
        return controls.allSatisfy(\.isHittable)
    }

    func sidebarToggle(label: String, in window: XCUIElement) -> XCUIElement {
        window.buttons.matching(NSPredicate(format: "label == %@", label)).firstMatch
    }

    func sidebarToggleCount(label: String, in window: XCUIElement) -> Int {
        window.buttons.matching(NSPredicate(format: "label == %@", label)).count
    }

    func toggleSidebarWithShortcut(
        in window: XCUIElement,
        expecting label: String
    ) -> Bool {
        for _ in 0..<3 {
            application.activate()
            application.typeKey("s", modifierFlags: [.control, .command])
            if sidebarToggle(label: label, in: window).waitForExistence(timeout: 1) {
                return true
            }
        }
        return false
    }

    func ensureUsageWindowVisible(contentIdentifier: String? = nil) -> Bool {
        let usageWindow = application.windows["usage-window"]
        if usageWindow.exists {
            application.activate()
            guard let contentIdentifier else { return true }
            if element(contentIdentifier).waitForExistence(timeout: 1) {
                return true
            }
        }
        DistributedNotificationCenter.default().postNotificationName(
            Notification.Name("com.jackin-project.desktop.visual-qa.show-usage"),
            object: nil,
            userInfo: nil,
            deliverImmediately: true
        )
        guard usageWindow.waitForExistence(timeout: 3) else { return false }
        application.activate()
        if !usageWindow.waitForExistence(timeout: 1) {
            DistributedNotificationCenter.default().postNotificationName(
                Notification.Name("com.jackin-project.desktop.visual-qa.show-usage"),
                object: nil,
                userInfo: nil,
                deliverImmediately: true
            )
            guard usageWindow.waitForExistence(timeout: 3) else { return false }
        }
        guard let contentIdentifier else { return true }
        return element(contentIdentifier).waitForExistence(timeout: 5)
    }

    func launchUsage(
        fixture: String,
        selection: String,
        size: String,
        accessoryFixture: Bool = true
    ) -> Bool {
        var arguments = [
            "--fixture", fixture,
            "--open-usage",
            "--selection", selection,
            "--window-size", size,
        ]
        if accessoryFixture {
            arguments.append("--ui-test")
        }
        application.launchArguments = arguments
        application.launch()
        application.activate()
        var opened = application.windows["usage-window"].waitForExistence(timeout: 8)
        if !opened {
            DistributedNotificationCenter.default().postNotificationName(
                Notification.Name("com.jackin-project.desktop.visual-qa.show-usage"),
                object: nil,
                userInfo: nil,
                deliverImmediately: true
            )
            opened = application.windows["usage-window"].waitForExistence(timeout: 8)
        }
        XCTAssertTrue(opened, application.debugDescription)
        guard opened else { return false }
        application.activate()
        return true
    }
}
