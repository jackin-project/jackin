// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import JackinUsageBridge
import SwiftUI

/// Liquid Glass Usage window: glass **sidebar + toolbar** chrome above
/// standard-material **content** (HIG: no Liquid Glass in the content layer).
struct UsageWindowRoot: View {
    @ObservedObject var store: PresentationStore
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openSettings) private var openSettings

    private static let overviewId = "__overview__"

    var body: some View {
        NavigationSplitView {
            List(selection: selectionBinding) {
                Section {
                    Label("Overview", systemImage: "square.grid.2x2")
                        .tag(Self.overviewId)
                    ForEach(store.overviewRows) { row in
                        let subtitle = sidebarSubtitle(for: row)
                        HStack(spacing: 8) {
                            Circle()
                                .fill(severityTint(row.severity))
                                .frame(width: 8, height: 8)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(row.displayLabel)
                                    .font(.body.weight(.medium))
                                // OpenUsage dual remaining when surface has two windows.
                                if let subtitle, !subtitle.isEmpty {
                                    Text(subtitle)
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
                        .accessibilityLabel(
                            "\(row.displayLabel) \(subtitle ?? row.headline)"
                        )
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
                if let id = store.usageSelection,
                   let surface = store.surfaces.first(where: { $0.id == id && $0.enabled })
                {
                    providerDetail(surface)
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
        .navigationSplitViewStyle(.balanced)
        .navigationTitle("Usage")
        .toolbar {
            ToolbarItemGroup(placement: .primaryAction) {
                Button {
                    store.refreshAll()
                } label: {
                    Label("Refresh", systemImage: "arrow.clockwise")
                }
                .keyboardShortcut("r", modifiers: [.command])
                .help("Refresh all enabled providers")

                Button {
                    openSettings()
                } label: {
                    Label("Settings", systemImage: "gearshape")
                }
                .keyboardShortcut(",", modifiers: [.command])
                .help("Open Settings")
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

    @ViewBuilder
    private func providerDetail(_ surface: PresentationStore.SurfaceRow) -> some View {
        let accountRows = store.accountsForSurface(surface.id)
        ProviderCardView(
            surface: surface,
            percentStyle: store.percentStyle,
            accounts: accountRows,
            onSelectAccount: { key in
                store.setSelectedAccount(surfaceId: surface.id, accountKey: key)
            }
        )
    }

    /// Dual remaining subtitle for sidebar rows (OpenUsage multi-window density).
    private func sidebarSubtitle(for row: PresentationStore.OverviewRow) -> String? {
        let surface = store.surfaces.first(where: { $0.id == row.surfaceId })
        let remainings = surface?.buckets.compactMap(\.remainingPercent) ?? []
        if let dual = surfaceRemainingSubtitle(
            remainings: remainings,
            compactLabel: surface?.statusBarLabel ?? "",
            percentStyle: store.percentStyle,
            maxLines: 2
        ) {
            return dual
        }
        if !row.headline.isEmpty {
            return row.headline
        }
        if !row.statusWord.isEmpty {
            return row.statusWord
        }
        return nil
    }
}
