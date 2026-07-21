// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit
import JackinUsageBridge
import SwiftUI

struct PopoverRoot: View {
    @ObservedObject var store: PresentationStore

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            if let err = store.lastError {
                Text(err)
                    .font(.caption)
                    .foregroundStyle(.red)
                    .padding(.horizontal, 12)
                    .padding(.bottom, 6)
                    .accessibilityLabel("Error \(err)")
            }
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 10) {
                    ForEach(store.surfaces.filter(\.enabled)) { surface in
                        SurfaceCard(surface: surface)
                    }
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 8)
            }
            .frame(maxHeight: 420)
            footer
        }
        .frame(width: 360)
        .onAppear {
            if !store.isOpen {
                store.openDefault()
            }
        }
    }

    private var header: some View {
        HStack {
            Text("jackin❯ usage")
                .font(.headline)
                .accessibilityLabel("jackin usage")
            Spacer()
            Button("Refresh") {
                store.refreshAll()
            }
            .keyboardShortcut("r", modifiers: [.command])
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
    }

    private var footer: some View {
        VStack(spacing: 0) {
            Divider()
            HStack(spacing: 12) {
                Text(footerUpdatedLabel)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                Spacer(minLength: 4)
                Button("Refresh") {
                    store.refreshAll()
                }
                .keyboardShortcut("r", modifiers: [.command])
                .controlSize(.small)
                SettingsLink {
                    Text("Settings…")
                }
                .keyboardShortcut(",", modifiers: [.command])
                .controlSize(.small)
                Button("Quit") {
                    NSApplication.shared.terminate(nil)
                }
                .keyboardShortcut("q", modifiers: [.command])
                .controlSize(.small)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background {
                GlassFallbacks.footerBarBackground()
            }
        }
    }

    private var footerUpdatedLabel: String {
        store.surfaces
            .filter(\.enabled)
            .compactMap { row in
                row.updatedLabel.isEmpty ? nil : row.updatedLabel
            }
            .first ?? "Rust owns probes · Swift display only"
    }
}
