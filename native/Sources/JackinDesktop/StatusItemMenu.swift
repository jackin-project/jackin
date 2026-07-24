// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit
import JackinUsageBridge

/// Builds the status-item right-click `NSMenu` from the pure
/// `StatusItemMenuModel` and dispatches selections through the injected router.
@MainActor
final class StatusItemMenu: NSObject {
    private let router: StatusItemMenuRouter

    init(router: StatusItemMenuRouter) {
        self.router = router
        super.init()
    }

    func build() -> NSMenu {
        let menu = NSMenu()
        for (index, row) in StatusItemMenuModel.rows.enumerated() {
            let item = NSMenuItem(
                title: row.title,
                action: #selector(handle(_:)),
                keyEquivalent: row.keyEquivalent
            )
            item.target = self
            item.tag = index
            menu.addItem(item)
        }
        return menu
    }

    @objc private func handle(_ sender: NSMenuItem) {
        let rows = StatusItemMenuModel.rows
        guard rows.indices.contains(sender.tag) else { return }
        router.dispatch(rows[sender.tag].action)
    }
}
