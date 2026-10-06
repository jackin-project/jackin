// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import JackinUsageBridge
import SwiftUI

/// Native provider/account hierarchy for the Overview destination.
public struct OverviewListView: View {
    public let groups: [PresentationStore.ProviderGroupRow]
    @Binding public var selectedRowID: String?
    @Binding public var expandedProviderIDs: Set<String>
    public var onSelect: (String, String?) -> Void
    public var onRetry: (String) -> Void

    public init(
        groups: [PresentationStore.ProviderGroupRow],
        selectedRowID: Binding<String?>,
        expandedProviderIDs: Binding<Set<String>>,
        onSelect: @escaping (String, String?) -> Void,
        onRetry: @escaping (String) -> Void
    ) {
        self.groups = groups
        _selectedRowID = selectedRowID
        _expandedProviderIDs = expandedProviderIDs
        self.onSelect = onSelect
        self.onRetry = onRetry
    }

    private var inventory: [OverviewTreeRow] {
        OverviewInventory.tree(groups: groups)
    }

    public var body: some View {
        if inventory.isEmpty {
            ContentUnavailableView(
                "No providers detected",
                systemImage: "chevron.right",
                description: Text(UsageWindowModel.emptyHint)
            )
            .accessibilityIdentifier("usage.overview.empty")
        } else {
            Table(of: OverviewTreeRow.self, selection: $selectedRowID) {
                TableColumn("Provider") { row in
                    Text(row.providerLabel)
                        .lineLimit(2)
                        .accessibilityLabel(row.accessibilityLabel)
                        .accessibilityIdentifier(rowAccessibilityIdentifier(row))
                }
                TableColumn("Account") { row in
                    Text(row.accountLabel)
                        .lineLimit(2)
                        .accessibilityHidden(true)
                }
                TableColumn("Plan or status") { row in
                    VStack(alignment: .leading, spacing: 3) {
                        Text(row.planOrStatusLabel)
                            .foregroundStyle(.primary)
                        if let error = row.lastError {
                            Text(error)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .lineLimit(2)
                                .accessibilityIdentifier("usage.overview.error.\(row.surfaceId)")
                            Button("Retry") { onRetry(row.surfaceId) }
                                .controlSize(.small)
                                .buttonStyle(.bordered)
                                .accessibilityIdentifier("usage.overview.retry.\(row.surfaceId)")
                        }
                    }
                    .accessibilityHidden(row.lastError == nil)
                }
                .width(min: 120, ideal: 210)
                TableColumn("Remaining") { row in
                    Text(row.remainingLabel)
                        .monospacedDigit()
                        .accessibilityHidden(true)
                }
                .width(min: 90, ideal: 110)
                TableColumn("Reset") { row in
                    Text(row.resetLabel)
                        .foregroundStyle(.primary)
                        .lineLimit(2)
                        .accessibilityHidden(true)
                }
                .width(min: 140, ideal: 210)
            } rows: {
                ForEach(inventory) { provider in
                    if let children = provider.children {
                        DisclosureTableRow(
                            provider,
                            isExpanded: expansionBinding(provider.surfaceId)
                        ) {
                            ForEach(children) { account in
                                TableRow(account)
                            }
                        }
                    } else {
                        TableRow(provider)
                    }
                }
            }
            .accessibilityLabel("Usage overview")
            .accessibilityIdentifier("usage.overview.table")
            .onChange(of: selectedRowID) { _, selectedID in
                guard let selectedID, let row = findRow(id: selectedID) else { return }
                onSelect(row.surfaceId, row.accountKey)
            }
        }
    }

    private func expansionBinding(_ surfaceId: String) -> Binding<Bool> {
        Binding(
            get: { expandedProviderIDs.contains(surfaceId) },
            set: { expanded in
                if expanded {
                    expandedProviderIDs.insert(surfaceId)
                } else {
                    expandedProviderIDs.remove(surfaceId)
                }
            }
        )
    }

    private func findRow(id: String) -> OverviewTreeRow? {
        for provider in inventory {
            if provider.id == id { return provider }
            if let account = provider.children?.first(where: { $0.id == id }) {
                return account
            }
        }
        return nil
    }

    private func rowAccessibilityIdentifier(_ row: OverviewTreeRow) -> String {
        if let accountKey = row.accountKey {
            return "usage.overview.account.\(row.surfaceId).\(accountKey)"
        }
        return "usage.overview.provider.\(row.surfaceId)"
    }
}
