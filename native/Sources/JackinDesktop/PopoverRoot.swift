// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit
import JackinUsageBridge
import SwiftUI

/// Glance popover — OpenUsage-inspired floating panel (clean-room layout).
///
/// Provider sections with inset metric cards; full detail still opens the Usage
/// window. All numbers/strings are Rust-owned.
struct PopoverRoot: View {
    @ObservedObject var store: PresentationStore
    @Environment(\.openWindow) private var openWindow
    @Environment(\.openSettings) private var openSettings

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if let err = store.lastError {
                Text(err)
                    .font(.caption)
                    .foregroundStyle(.red)
                    .padding(.horizontal, 14)
                    .padding(.top, 12)
                    .accessibilityLabel("Error \(err)")
            }

            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    if enabledSurfaces.isEmpty {
                        emptyState
                    } else {
                        ForEach(enabledSurfaces) { surface in
                            providerSection(surface)
                        }
                    }
                }
                .padding(.horizontal, 12)
                .padding(.top, 12)
                .padding(.bottom, 8)
            }
            .frame(maxHeight: 520)

            footerBar
        }
        .frame(width: 340)
        .background {
            GlassFallbacks.panelSurfaceBackground()
        }
        .onAppear {
            if !store.isOpen {
                store.openDefault()
            }
        }
    }

    private var enabledSurfaces: [PresentationStore.SurfaceRow] {
        store.surfaces.filter(\.enabled)
    }

    // MARK: - Provider section (OpenUsage card stack)

    private func providerSection(_ surface: PresentationStore.SurfaceRow) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Button {
                store.selectUsageSurface(surface.id)
                openWindow(id: "usage")
            } label: {
                HStack(spacing: 8) {
                    Text(statusItemGlyph(compactLabel: surface.label, surfaceId: surface.id))
                        .font(.system(size: 10, weight: .bold, design: .rounded))
                        .frame(width: 22, height: 22)
                        .background {
                            Circle().fill(severityTint(worstSeverity(surface)).opacity(0.2))
                        }
                    Text(surface.label)
                        .font(.body.weight(.semibold))
                        .lineLimit(1)
                    if let plan = surface.planLabel, !plan.isEmpty {
                        Text(plan)
                            .font(.caption.weight(.medium))
                            .foregroundStyle(.secondary)
                    }
                    Spacer(minLength: 4)
                    if let symbol = statusBadgeSymbol(surface.status) {
                        Image(systemName: symbol)
                            .font(.caption)
                            .foregroundStyle(badgeTint(surface.status))
                    }
                    Image(systemName: "chevron.right")
                        .font(.caption2.weight(.semibold))
                        .foregroundStyle(.tertiary)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("\(surface.label), open usage")

            VStack(alignment: .leading, spacing: 12) {
                if surface.buckets.isEmpty {
                    emptyMetricRow()
                } else {
                    ForEach(surface.buckets) { bucket in
                        metricRow(bucket)
                    }
                }
            }
            .padding(12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background {
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .fill(Color(nsColor: .controlBackgroundColor).opacity(0.55))
            }
        }
    }

    private func metricRow(_ bucket: PresentationStore.BucketRow) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            Text(bucket.label)
                .font(.subheadline.weight(.semibold))

            switch bucketRowShape(
                remainingPercent: bucket.remainingPercent,
                usedLabel: bucket.usedLabel
            ) {
            case .gauge:
                if let remaining = bucket.remainingPercent {
                    fullWidthBar(remaining: remaining, severity: bucket.severity)
                }
                HStack(alignment: .firstTextBaseline) {
                    Text(bucket.usedLabel ?? "—")
                        .font(.caption)
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                    Spacer(minLength: 8)
                    if let reset = bucket.resetLabel, !reset.isEmpty {
                        Text(reset)
                            .font(.caption)
                            .monospacedDigit()
                            .foregroundStyle(.secondary)
                            .multilineTextAlignment(.trailing)
                    }
                }
            case .valueOnly:
                HStack {
                    Text(bucket.usedLabel ?? "—")
                        .font(.caption)
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                    Spacer()
                    if let reset = bucket.resetLabel {
                        Text(reset)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
            case .empty:
                emptyMetricRow(title: nil)
            }

            if let pace = bucket.paceLabel, !pace.isEmpty {
                Text(pace)
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
        }
        .accessibilityElement(children: .combine)
    }

    private func emptyMetricRow(title: String? = "—") -> some View {
        VStack(alignment: .leading, spacing: 5) {
            if let title {
                Text(title)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(.secondary)
            }
            Capsule()
                .fill(Color.primary.opacity(0.08))
                .frame(height: 4)
            HStack {
                Text("—")
                    .font(.caption)
                    .foregroundStyle(.tertiary)
                Spacer()
                Text("No data")
                    .font(.caption)
                    .foregroundStyle(.tertiary)
            }
        }
    }

    private func fullWidthBar(remaining: UInt8, severity: String) -> some View {
        let used = statusItemUsedFraction(remainingPercent: remaining)
        return GeometryReader { geo in
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(Color.primary.opacity(0.10))
                Capsule()
                    .fill(severityTint(severity))
                    .frame(width: max(3, geo.size.width * (1.0 - used)))
            }
        }
        .frame(height: 5)
        .accessibilityHidden(true)
    }

    private func worstSeverity(_ surface: PresentationStore.SurfaceRow) -> String {
        let ranks = ["danger": 0, "warn": 1, "ok": 2, "info": 3]
        return surface.buckets
            .map(\.severity)
            .min(by: { (ranks[$0] ?? 9) < (ranks[$1] ?? 9) })
            ?? "ok"
    }

    private func badgeTint(_ status: String) -> Color {
        switch status {
        case "error", "needs_login", "needs_secret", "unavailable":
            return .orange
        case "stale":
            return .secondary
        default:
            return .secondary
        }
    }

    // MARK: - Empty / footer

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
        .padding(4)
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
    }

    private var footerBar: some View {
        VStack(spacing: 0) {
            Divider().opacity(0.35)
            HStack(alignment: .center, spacing: 10) {
                VStack(alignment: .leading, spacing: 2) {
                    Text("jackin❯ Desktop")
                        .font(.caption2.weight(.medium))
                        .foregroundStyle(.secondary)
                        .accessibilityLabel("jackin Desktop")
                    if !store.nextRefreshLabel.isEmpty {
                        Text(store.nextRefreshLabel)
                            .font(.caption2)
                            .foregroundStyle(.tertiary)
                            .lineLimit(1)
                    }
                }
                Spacer(minLength: 8)
                Menu {
                    Button("Open Usage…") {
                        store.selectUsageSurface(nil)
                        openWindow(id: "usage")
                    }
                    Button("Refresh") {
                        store.refreshAll()
                    }
                    .keyboardShortcut("r", modifiers: [.command])
                    Divider()
                    SettingsLink {
                        Text("Settings…")
                    }
                    .keyboardShortcut(",", modifiers: [.command])
                    Divider()
                    Button("Quit") {
                        NSApplication.shared.terminate(nil)
                    }
                    .keyboardShortcut("q", modifiers: [.command])
                } label: {
                    Text("Options")
                        .font(.caption.weight(.medium))
                        .padding(.horizontal, 12)
                        .padding(.vertical, 5)
                        .background {
                            Capsule().fill(Color.primary.opacity(0.08))
                        }
                }
                .menuStyle(.borderlessButton)
                .fixedSize()
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
            .background {
                GlassFallbacks.footerBarBackground()
            }
        }
    }
}
