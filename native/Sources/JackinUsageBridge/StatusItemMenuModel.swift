// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import Foundation


public enum StatusItemMenuAction: Equatable, Sendable {
    case openUsageWindow
    case refresh
    case quit
}


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


public enum StatusItemMenuModel {
    public static let rows: [StatusItemMenuRow] = [
        StatusItemMenuRow(title: "Open Usage Window", action: .openUsageWindow, keyEquivalent: ""),
        StatusItemMenuRow(title: "Refresh", action: .refresh, keyEquivalent: "r"),
        StatusItemMenuRow(title: "Quit jackin❯ desktop", action: .quit, keyEquivalent: "q"),
    ]
}




public struct StatusItemMenuRouter {
    private let openUsageWindow: (String?) -> Void
    private let refresh: () -> Void
    private let quit: () -> Void






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


    public func openUsage(focusOn surfaceId: String?) {
        openUsageWindow(surfaceId)
    }
}
