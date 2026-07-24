// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import Foundation

/// A status-item right-click menu action (pure; no AppKit).
public enum StatusItemMenuAction: Equatable, Sendable {
    case openUsageWindow
    case refresh
    case quit
}

/// One static menu row: title, action, and key equivalent (spec-fixed copy).
public struct StatusItemMenuRow: Equatable, Sendable {
    public let title: String
    public let action: StatusItemMenuAction
    public let keyEquivalent: String

    public init(title: String, action: StatusItemMenuAction, keyEquivalent: String) {
        self.title = title
        self.action = action
        self.keyEquivalent = keyEquivalent
    }
}

/// The fixed status-item context menu: Open Usage Window, Refresh, Quit.
public enum StatusItemMenuModel {
    public static let rows: [StatusItemMenuRow] = [
        StatusItemMenuRow(title: "Open Usage Window", action: .openUsageWindow, keyEquivalent: ""),
        StatusItemMenuRow(title: "Refresh", action: .refresh, keyEquivalent: "r"),
        StatusItemMenuRow(title: "Quit jackin❯ Desktop", action: .quit, keyEquivalent: "q"),
    ]
}

/// Pure action router: maps a menu action (or a provider-header click) to the
/// injected host handlers. Testable without AppKit — tests inject counters.
public struct StatusItemMenuRouter {
    private let openUsageWindow: (String?) -> Void
    private let refresh: () -> Void
    private let quit: () -> Void

    /// - Parameters:
    ///   - openUsageWindow: opens the Usage window, optionally focused on a
    ///     provider surface id (`nil` = Overview).
    public init(
        openUsageWindow: @escaping (String?) -> Void,
        refresh: @escaping () -> Void,
        quit: @escaping () -> Void
    ) {
        self.openUsageWindow = openUsageWindow
        self.refresh = refresh
        self.quit = quit
    }

    public func dispatch(_ action: StatusItemMenuAction) {
        switch action {
        case .openUsageWindow:
            openUsageWindow(nil)
        case .refresh:
            refresh()
        case .quit:
            quit()
        }
    }

    /// A provider-header click opens the Usage window focused on that provider.
    public func openUsage(focusOn surfaceId: String?) {
        openUsageWindow(surfaceId)
    }
}
