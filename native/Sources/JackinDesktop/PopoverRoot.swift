// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit
import JackinUsageBridge
import SwiftUI

struct PopoverRoot: View {
    @ObservedObject var store: PresentationStore
    @Environment(\.openWindow) private var openWindow
    @Environment(\.openSettings) private var openSettings

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
            if store.overviewRows.isEmpty {
                emptyState
            } else {
                overviewStrip
            }
            footerMenu
            footerStatus
        }
        .frame(width: 360)
        .background {
            GlassFallbacks.panelSurfaceBackground()
        }
        .onAppear {
            if !store.isOpen {
                store.openDefault()
            }
        }
    }

    private var header: some View {
        Text("jackin❯ Desktop")
            .font(.headline)
            .accessibilityLabel("jackin Desktop")
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var overviewStrip: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(store.overviewRows) { row in
                Button {
                    store.selectUsageSurface(row.surfaceId)
                    openWindow(id: "usage")
                } label: {
                    HStack(spacing: 8) {
                        Circle()
                            .fill(severityTint(row.severity))
                            .frame(width: 7, height: 7)
                        Text(row.displayLabel)
                            .font(.body.weight(.medium))
                            .lineLimit(1)
                        Spacer(minLength: 8)
                        trailingBody(for: row)
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityElement(children: .combine)
                .accessibilityLabel(rowAccessibility(row))
            }
        }
        .padding(.bottom, 4)
    }

    @ViewBuilder
    private func trailingBody(for row: PresentationStore.OverviewRow) -> some View {
        switch overviewGlanceBody(
            headline: row.headline,
            resetLabel: row.resetLabel,
            statusWord: row.statusWord
        ) {
        case .numeric(let headline, let reset):
            HStack(spacing: 4) {
                Text(headline)
                    .font(.caption)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
                if let reset {
                    Text("·")
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                    Text(reset)
                        .font(.caption)
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
        case .statusWord(let word):
            Text(word)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    private func rowAccessibility(_ row: PresentationStore.OverviewRow) -> String {
        switch overviewGlanceBody(
            headline: row.headline,
            resetLabel: row.resetLabel,
            statusWord: row.statusWord
        ) {
        case .numeric(let headline, let reset):
            if let reset {
                return "\(row.displayLabel), \(headline), \(reset)"
            }
            return "\(row.displayLabel), \(headline)"
        case .statusWord(let word):
            return "\(row.displayLabel), \(word)"
        }
    }

    private var emptyState: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("No usage surfaces enabled.")
                .font(.body.weight(.medium))
            Text(
                "jackin❯ Desktop reads the credentials your agent CLIs already store — sign in with an agent, then enable its surface in Settings."
            )
            .font(.caption)
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
            SettingsLink {
                Text("Open Settings…")
            }
            .controlSize(.small)
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
    }

    private var footerMenu: some View {
        VStack(spacing: 0) {
            Divider()
            menuRow(title: "Open Usage…", systemImage: "rectangle.split.2x1", shortcut: nil) {
                store.selectUsageSurface(nil)
                openWindow(id: "usage")
            }
            menuRow(title: "Refresh", systemImage: "arrow.clockwise", shortcut: "⌘R") {
                store.refreshAll()
            }
            .keyboardShortcut("r", modifiers: [.command])
            SettingsLink {
                menuRowLabel(title: "Settings…", systemImage: "gearshape", shortcut: "⌘,")
            }
            .keyboardShortcut(",", modifiers: [.command])
            .buttonStyle(.plain)
            menuRow(title: "Quit", systemImage: "power", shortcut: "⌘Q") {
                NSApplication.shared.terminate(nil)
            }
            .keyboardShortcut("q", modifiers: [.command])
        }
    }

    private var footerStatus: some View {
        VStack(spacing: 0) {
            Divider()
            HStack {
                Text(footerStatusLine)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                Spacer(minLength: 4)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background {
                GlassFallbacks.footerBarBackground()
            }
        }
    }

    private var footerStatusLine: String {
        let updated = store.surfaces
            .filter(\.enabled)
            .compactMap { row in
                row.updatedLabel.isEmpty ? nil : row.updatedLabel
            }
            .first
        let next = store.nextRefreshLabel
        switch (updated, next.isEmpty) {
        case (let u?, false):
            return "\(u) · \(next)"
        case (let u?, true):
            return u
        case (nil, false):
            return next
        case (nil, true):
            return ""
        }
    }

    private func menuRow(
        title: String,
        systemImage: String,
        shortcut: String?,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            menuRowLabel(title: title, systemImage: systemImage, shortcut: shortcut)
        }
        .buttonStyle(.plain)
    }

    private func menuRowLabel(title: String, systemImage: String, shortcut: String?) -> some View {
        HStack {
            Label(title, systemImage: systemImage)
                .labelStyle(.titleAndIcon)
            Spacer()
            if let shortcut {
                Text(shortcut)
                    .font(.caption)
                    .foregroundStyle(.tertiary)
                    .monospacedDigit()
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 7)
        .contentShape(Rectangle())
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
