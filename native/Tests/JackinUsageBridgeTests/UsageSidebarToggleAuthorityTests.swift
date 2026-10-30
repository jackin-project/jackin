// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit
import XCTest

@testable import JackinDesktopUI

@MainActor
final class UsageSidebarToggleAuthorityTests: XCTestCase {
    func testViewMenuDispatchesOnlyThroughNativeSplitViewResponder() {
        let item = AppMainMenu.sidebarMenuItem()

        XCTAssertEqual(item.action, #selector(NSSplitViewController.toggleSidebar(_:)))
        XCTAssertNil(item.target)
        XCTAssertEqual(item.title, "Hide Sidebar")
        XCTAssertEqual(item.keyEquivalent, AppMainMenu.sidebarKeyEquivalent)
        XCTAssertEqual(item.keyEquivalentModifierMask, AppMainMenu.sidebarKeyModifiers)
    }

    func testWindowToolbarUsesOnlyStandardSplitViewItems() async {
        let sidebarItem = NSSplitViewItem(
            sidebarWithViewController: NSViewController()
        )
        let owner = UsageWindowToolbar(sidebarItem: sidebarItem, onSidebarStateChange: {})
        let toolbar = owner.makeToolbar()
        let window = NSWindow()
        window.toolbar = toolbar
        owner.installStandardItems(in: toolbar)

        let expected: [NSToolbarItem.Identifier] = [
            .toggleSidebar, .sidebarTrackingSeparator,
        ]
        XCTAssertEqual(toolbar.itemIdentifiers, expected)
        XCTAssertEqual(toolbar.items.map(\.itemIdentifier), toolbar.itemIdentifiers)
        XCTAssertEqual(toolbar.items[0].label, "Hide Sidebar")
        XCTAssertEqual(toolbar.items[0].toolTip, "Hide Sidebar")
        XCTAssertEqual(toolbar.items[0].view?.accessibilityLabel(), "Hide Sidebar")
        XCTAssertEqual(
            owner.toolbarDefaultItemIdentifiers(toolbar),
            expected
        )
        XCTAssertEqual(
            owner.toolbarAllowedItemIdentifiers(toolbar),
            expected
        )

        sidebarItem.isCollapsed = true
        await Task.yield()
        XCTAssertEqual(toolbar.items[0].label, "Show Sidebar")
        XCTAssertEqual(toolbar.items[0].toolTip, "Show Sidebar")
        XCTAssertEqual(toolbar.items[0].view?.accessibilityLabel(), "Show Sidebar")
    }

    func testStandardMenuKeyEquivalents() {
        XCTAssertEqual(AppMainMenu.settingsKeyEquivalent, ",")
        XCTAssertEqual(AppMainMenu.settingsKeyModifiers, .command)
        XCTAssertEqual(AppMainMenu.closeKeyEquivalent, "w")
        XCTAssertEqual(AppMainMenu.closeKeyModifiers, .command)
        XCTAssertEqual(AppMainMenu.sidebarKeyEquivalent, "s")
        XCTAssertEqual(AppMainMenu.sidebarKeyModifiers, [.command, .control])
        XCTAssertEqual(AppMainMenu.refreshKeyEquivalent, "r")
        XCTAssertEqual(AppMainMenu.refreshKeyModifiers, .command)
    }
}
