// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import JackinUsageBridge
import SwiftUI

/// Glance popover composition root: a tab grid over the Rust-owned glance rows,
/// then the selected Overview or provider tab, then a Refresh-only footer.
///
/// Display-only — every provider, order, label, number, and segment comes from
/// the Rust `providerGlanceRows` / bucket presentation. `onOpenUsage` is an
/// optional seam plan 007 binds to the Usage window; it is `nil` here so the
/// header click is inert.
struct PopoverRoot: View {
    @ObservedObject var store: PresentationStore
    var onOpenUsage: ((String?) -> Void)?

    init(store: PresentationStore, onOpenUsage: ((String?) -> Void)? = nil) {
        self.store = store
        self.onOpenUsage = onOpenUsage
    }

    var body: some View {
        VStack(spacing: 0) {
            PopoverTabGrid(
                providers: store.providerGlanceRows,
                selection: $store.popoverSelection
            )
            Divider()
            ScrollView {
                content
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            Divider()
            PopoverFooter(refreshInProgress: store.refreshInProgress) {
                store.refreshAll()
            }
        }
        .frame(width: 320)
        .frame(minHeight: 200, maxHeight: 480)
    }

    @ViewBuilder
    private var content: some View {
        if let selection = store.popoverSelection,
           let provider = store.providerGlanceRows.first(where: { $0.surfaceId == selection })
        {
            PopoverProviderTab(
                provider: provider,
                surface: store.surfaces.first(where: { $0.id == selection }),
                accounts: store.accountsForSurface(selection),
                refreshInProgress: store.refreshInProgress,
                onSelectAccount: { surfaceId, accountKey in
                    store.setSelectedAccount(surfaceId: surfaceId, accountKey: accountKey)
                },
                onOpenUsageWindow: { id in onOpenUsage?(id) }
            )
        } else if store.providerGlanceRows.isEmpty {
            emptyState
        } else {
            PopoverOverviewTab(
                providers: store.providerGlanceRows,
                selection: $store.popoverSelection
            )
        }
    }

    private var emptyState: some View {
        VStack(spacing: 8) {
            Image(systemName: "chevron.right")
                .font(.title)
                .foregroundStyle(.secondary)
            Text("No agent usage detected")
                .font(.callout)
                .foregroundStyle(.secondary)
            Text("Sign in to a supported agent to see usage.")
                .font(.caption2)
                .foregroundStyle(.tertiary)
        }
        .padding()
        .frame(maxWidth: .infinity)
    }
}
