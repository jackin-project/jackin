// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import Foundation

/// Thin homogeneous adapter required by SwiftUI's hierarchical `Table` API.
///
/// Every display string is copied verbatim from the atomic Rust projection.
public struct OverviewTreeRow: Identifiable, Equatable, Sendable {
    public let id: String
    public let surfaceId: String
    public let accountKey: String?
    public let providerLabel: String
    public let accountLabel: String
    public let planOrStatusLabel: String
    public let remainingLabel: String
    public let resetLabel: String
    public let accessibilityLabel: String
    public let lastError: String?
    public let children: [OverviewTreeRow]?

    public var isProvider: Bool { accountKey == nil }

    public init(group: PresentationStore.ProviderGroupRow) {
        id = "provider#\(group.surfaceId)"
        surfaceId = group.surfaceId
        accountKey = nil
        providerLabel = group.displayLabel
        accountLabel = group.accountColumnLabel
        planOrStatusLabel = group.planOrStatusLabel
        remainingLabel = group.remainingLabel
        resetLabel = group.resetDisplayLabel
        accessibilityLabel = group.accessibilityLabel
        lastError = group.lastError
        children = group.accounts.isEmpty ? nil : group.accounts.map(Self.init(account:))
    }

    public init(account: PresentationStore.AccountRow) {
        id = "account#\(account.surfaceId)#\(account.accountKey)"
        surfaceId = account.surfaceId
        accountKey = account.accountKey
        providerLabel = account.providerColumnLabel
        accountLabel = account.accountLabel
        planOrStatusLabel = account.planOrStatusLabel
        remainingLabel = account.remainingLabel
        resetLabel = account.resetDisplayLabel
        accessibilityLabel = account.accessibilityLabel
        lastError = account.lastError
        children = nil
    }
}

public enum OverviewInventory: Sendable {
    public static func tree(
        groups: [PresentationStore.ProviderGroupRow]
    ) -> [OverviewTreeRow] {
        groups.map(OverviewTreeRow.init(group:))
    }
}
