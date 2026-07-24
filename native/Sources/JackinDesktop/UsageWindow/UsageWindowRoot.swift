// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import JackinUsageBridge
import SwiftUI

/// Liquid Glass Usage window: glass **sidebar + toolbar** chrome above
/// standard-material **content** (HIG: no Liquid Glass in the content layer).
///
/// Every provider, order, label, number, and segment comes from Rust via
/// ``UsageWindowModel``; this view only routes selection actions to the store's
/// one mutation each and draws the Rust-finished rows (plan 008).
struct UsageWindowRoot: View {
    @ObservedObject var store: PresentationStore
    @Environment(\.dismiss) private var dismiss

    private static let overviewId = "__overview__"

    private var model: UsageWindowModel {
        UsageWindowModel(
            glanceRows: store.providerGlanceRows,
            surfaces: store.surfaces,
            accounts: store.accounts,
            selection: store.usageSelection
        )
    }

    var body: some View {
        let model = self.model
        NavigationSplitView {
            List(selection: selectionBinding) {
                Section {
                    Label("Overview", systemImage: "square.grid.2x2")
                        .tag(Self.overviewId)
                    // Sidebar rows in the Rust-owned canonical (Capsule tab) order.
                    ForEach(model.sidebar) { row in
                        HStack(spacing: 8) {
                            Circle()
                                .fill(severityTint(row.severity))
                                .frame(width: 8, height: 8)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(row.displayLabel)
                                    .font(.body.weight(.medium))
                                if !row.headline.isEmpty {
                                    Text(row.headline)
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                        .monospacedDigit()
                                        .lineLimit(1)
                                        .minimumScaleFactor(0.75)
                                }
                            }
                            Spacer(minLength: 4)
                        }
                        .tag(row.surfaceId)
                        .accessibilityLabel("\(row.displayLabel) \(row.headline)")
                    }
                }
            }
            .listStyle(.sidebar)
            .navigationSplitViewColumnWidth(min: 200, ideal: 240, max: 320)
            .background {
                // Navigation layer — Liquid Glass.
                GlassFallbacks.sidebarBackground()
            }
            .safeAreaInset(edge: .bottom, spacing: 0) {
                HStack {
                    Text(store.nextRefreshLabel)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                    Spacer(minLength: 4)
                }
                .padding(.horizontal, 14)
                .padding(.vertical, 10)
                .background {
                    GlassFallbacks.footerBarBackground()
                }
            }
        } detail: {
            // Content layer — standard window background only (no glass).
            Group {
                if let content = model.content {
                    ProviderCardView(
                        content: content,
                        onSelectAccount: { key in
                            store.setSelectedAccount(
                                surfaceId: content.surfaceId,
                                accountKey: key
                            )
                        }
                    )
                } else {
                    OverviewListView(model: model) { surfaceId in
                        store.selectUsageSurface(surfaceId)
                    }
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background {
                GlassFallbacks.windowContentBackground()
            }
        }
        .navigationSplitViewStyle(.balanced)
        .navigationTitle("Usage")
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button {
                    store.refreshAll()
                } label: {
                    Label("Refresh", systemImage: "arrow.clockwise")
                }
                .keyboardShortcut("r", modifiers: [.command])
                .help("Refresh all enabled providers")
            }
        }
        .onExitCommand {
            dismiss()
        }
        .onAppear {
            if !store.isOpen {
                store.openDefault()
            }
        }
        .frame(minWidth: 760, minHeight: 500)
    }

    private var selectionBinding: Binding<String?> {
        Binding(
            get: {
                store.usageSelection ?? Self.overviewId
            },
            set: { newValue in
                if newValue == Self.overviewId || newValue == nil {
                    store.selectUsageSurface(nil)
                } else {
                    store.selectUsageSurface(newValue)
                }
            }
        )
    }
}
