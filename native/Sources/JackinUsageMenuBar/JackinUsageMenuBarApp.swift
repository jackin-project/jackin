// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//
// jackin❯ usage menu bar — display-only shell over Rust UniFFI.
// Clean-room CodexBar-shaped UX (tiles, bars, countdowns); no Swift probes.

import SwiftUI
import JackinUsageBridge
import AppKit

@main
struct JackinUsageMenuBarApp: App {
    @StateObject private var store = PresentationStore()

    var body: some Scene {
        MenuBarExtra {
            PopoverRoot(store: store)
        } label: {
            Text(compactBarText(store.mergedBarLabel))
                .font(.system(size: 12, weight: .medium, design: .monospaced))
        }
        .menuBarExtraStyle(.window)

        Settings {
            SettingsView(store: store)
        }
    }
}

private func compactBarText(_ label: String) -> String {
    if label.count <= 48 {
        return label
    }
    return String(label.prefix(45)) + "…"
}

struct PopoverRoot: View {
    @ObservedObject var store: PresentationStore

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
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
            if let err = store.lastError {
                Text(err)
                    .font(.caption)
                    .foregroundStyle(.red)
                    .accessibilityLabel("Error \(err)")
            }
            Divider()
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 10) {
                    ForEach(store.surfaces.filter(\.enabled)) { surface in
                        SurfaceTile(surface: surface)
                    }
                }
            }
            .frame(maxHeight: 420)
            Divider()
            Text("Rust owns probes · Swift display only")
                .font(.caption2)
                .foregroundStyle(.secondary)
        }
        .padding(12)
        .frame(width: 360)
        .onAppear {
            if !store.isOpen {
                store.openDefault()
            }
        }
    }
}

struct SurfaceTile: View {
    let surface: PresentationStore.SurfaceRow

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(surface.label)
                    .font(.subheadline.weight(.semibold))
                Spacer()
                Text(surface.status)
                    .font(.caption2)
                    .padding(.horizontal, 6)
                    .padding(.vertical, 2)
                    .background(statusColor(surface.status).opacity(0.15))
                    .clipShape(Capsule())
            }
            if !surface.accountLabel.isEmpty {
                Text(surface.accountLabel)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            if let plan = surface.planLabel {
                Text(plan)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
            Text(surface.statusBarLabel)
                .font(.system(.caption, design: .monospaced))
                .accessibilityLabel("\(surface.label) \(surface.statusBarLabel)")
            ForEach(surface.buckets) { bucket in
                BucketBar(bucket: bucket)
            }
            if let reset = surface.buckets.compactMap(\.resetLabel).first {
                Text(reset)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
            if let err = surface.lastError {
                Text(err)
                    .font(.caption2)
                    .foregroundStyle(.orange)
            }
            Text(surface.updatedLabel)
                .font(.caption2)
                .foregroundStyle(.tertiary)
        }
        .padding(8)
        .background(Color(nsColor: .controlBackgroundColor))
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .accessibilityElement(children: .combine)
    }

    private func statusColor(_ status: String) -> Color {
        switch status {
        case "fresh": return .green
        case "stale", "refreshing": return .yellow
        case "error", "unavailable", "needs_login", "needs_secret": return .red
        default: return .secondary
        }
    }
}

struct BucketBar: View {
    let bucket: PresentationStore.BucketRow

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack {
                Text(bucket.label)
                    .font(.caption2)
                Spacer()
                if let used = bucket.usedLabel {
                    Text(used)
                        .font(.caption2.monospacedDigit())
                } else if bucket.status == "unavailable" || bucket.status == "refreshing" {
                    Text(bucket.status)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }
            // Never invent % — only draw when Rust provided remaining_percent.
            if let remaining = bucket.remainingPercent {
                ProgressView(value: Double(100 - Int(remaining)), total: 100)
                    .tint(severityColor(bucket.severity))
                    .accessibilityLabel("\(bucket.label) \(100 - Int(remaining)) percent used")
            }
        }
    }

    private func severityColor(_ severity: String) -> Color {
        switch severity {
        case "danger": return .red
        case "warn": return .orange
        default: return .accentColor
        }
    }
}

struct SettingsView: View {
    @ObservedObject var store: PresentationStore

    var body: some View {
        Form {
            Section("Surfaces") {
                ForEach(store.surfaces) { surface in
                    Toggle(surface.label, isOn: Binding(
                        get: { surface.enabled },
                        set: { store.setEnabled(surfaceId: surface.id, enabled: $0) }
                    ))
                    .accessibilityLabel("\(surface.label) enabled")
                }
            }
            Section("About") {
                Text("Account quotas from host credentials via jackin-usage (Rust).")
                    .font(.caption)
                Text("No passwords stored. No Capsule required.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
        .frame(width: 420, height: 360)
        .onAppear {
            if !store.isOpen {
                store.openDefault()
            }
        }
    }
}
