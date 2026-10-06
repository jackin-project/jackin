// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit
import JackinUsageBridge

/// Builds and owns the status-item right-click `NSMenu` from the pure
/// `StatusItemMenuModel` and dispatches selections through the injected router.
///
/// **Retention:** every `NSMenuItem.target` is `self`. The host
/// (`StatusBarController`) must keep a strong reference to this object for as
/// long as the menu is used. Holding only the `NSMenu` lets this controller
/// deallocate; AppKit then disables every row (Open Usage / Refresh / Quit).
@MainActor
public final class StatusItemMenu: NSObject {
    private let router: StatusItemMenuRouter
    /// Owned menu; items target `self`.
    private let menu: NSMenu

    public init(router: StatusItemMenuRouter) {
        self.router = router
        self.menu = NSMenu()
        super.init()
        for (index, row) in StatusItemMenuModel.rows.enumerated() {
            if row.action == .quit, index > 0 {
                menu.addItem(.separator())
            }
            let item = NSMenuItem(
                title: row.title,
                action: #selector(handle(_:)),
                keyEquivalent: row.keyEquivalent
            )
            item.target = self
            item.tag = index
            item.isEnabled = true
            menu.addItem(item)
        }
    }

    public func popUp(positioning item: NSMenuItem?, at location: NSPoint, in view: NSView?) {
        menu.popUp(positioning: item, at: location, in: view)
    }

    public func cancelTracking() {
        menu.cancelTracking()
    }

    @objc private func handle(_ sender: NSMenuItem) {
        let rows = StatusItemMenuModel.rows
        guard rows.indices.contains(sender.tag) else { return }
        router.dispatch(rows[sender.tag].action)
    }
}
