// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import JackinUsageBridge
import SwiftUI

/// Liquid Glass Usage window: Overview sidebar + provider detail (S3/S4).
struct UsageWindowRoot: View {
    @ObservedObject var store: PresentationStore
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openSettings) private var openSettings

    private static let overviewId = "__overview__"

    var body: some View {
        NavigationSplitView {
            List(selection: selectionBinding) {
                Label("Overview", systemImage: "square.grid.2x2")
                    .tag(Self.overviewId)
                ForEach(store.overviewRows) { row in
                    HStack(spacing: 8) {
                        Circle()
                            .fill(severityTint(row.severity))
                            .frame(width: 8, height: 8)
                        Text(row.displayLabel)
                        Spacer(minLength: 4)
                        if !row.headline.isEmpty {
                            Text(row.headline)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .monospacedDigit()
                                .lineLimit(1)
                        }
                    }
                    .tag(row.surfaceId)
                    .accessibilityLabel("\(row.displayLabel) \(row.headline)")
                }
            }
            .listStyle(.sidebar)
            .navigationSplitViewColumnWidth(min: 180, ideal: 220, max: 300)
            .background {
                GlassFallbacks.sidebarBackground()
            }
        } detail: {
            Group {
                if let id = store.usageSelection,
                   let surface = store.surfaces.first(where: { $0.id == id && $0.enabled })
                {
                    ProviderCardView(surface: surface, percentStyle: store.percentStyle)
                } else {
                    OverviewListView(store: store) { surfaceId in
                        store.selectUsageSurface(surfaceId)
                    }
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background {
                GlassFallbacks.windowContentBackground()
            }
        }
        .navigationTitle("jackin❯ Desktop — Usage")
        .toolbar {
            ToolbarItemGroup(placement: .primaryAction) {
                Button("Refresh") {
                    store.refreshAll()
                }
                .keyboardShortcut("r", modifiers: [.command])
                Button("Settings…") {
                    openSettings()
                }
                .keyboardShortcut(",", modifiers: [.command])
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
        .frame(minWidth: 720, minHeight: 480)
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
