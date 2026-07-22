// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit
import JackinUsageBridge
import SwiftUI

/// Glance popover — OpenUsage-like floating dashboard (clean-room layout).
///
/// Solid elevated card, provider headers outside inset metric groups, full-width
/// capacity bars with value/reset captions, money rows when Rust supplies them,
/// Options footer. Cost donut / sparklines stay deferred (need more Rust models).
struct PopoverRoot: View {
    @ObservedObject var store: PresentationStore
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if let err = store.lastError {
                Text(err)
                    .font(.caption)
                    .foregroundStyle(.red)
                    .padding(.horizontal, 16)
                    .padding(.top, 14)
                    .accessibilityLabel("Error \(err)")
            }

            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    if enabledSurfaces.isEmpty {
                        emptyState
                    } else {
                        // Optional spend summary when any bucket carries money.
                        if !spendRows.isEmpty {
                            spendSummaryCard
                        }
                        ForEach(enabledSurfaces) { surface in
                            providerBlock(surface)
                        }
                    }
                }
                .padding(.horizontal, 14)
                .padding(.top, 14)
                .padding(.bottom, 10)
            }
            .frame(maxHeight: 560)

            footerBar
        }
        .frame(width: 320)
        .background {
            // OpenUsage uses a solid elevated card, not heavy glass chrome.
            RoundedRectangle(cornerRadius: 20, style: .continuous)
                .fill(Color(nsColor: .windowBackgroundColor))
                .shadow(color: .black.opacity(0.18), radius: 28, y: 10)
        }
        .clipShape(RoundedRectangle(cornerRadius: 20, style: .continuous))
        .onAppear {
            if !store.isOpen {
                store.openDefault()
            }
        }
    }

    private var enabledSurfaces: [PresentationStore.SurfaceRow] {
        store.surfaces.filter(\.enabled)
    }

    /// Surface spend lines from Rust money fields (no invented totals).
    private var spendRows: [(id: String, label: String, amount: String)] {
        enabledSurfaces.compactMap { surface in
            let moneys = surface.buckets.compactMap(\.usedMoney)
            guard let first = moneys.first else { return nil }
            // Prefer a single bucket money line per surface (display only).
            return (surface.id, surface.label, formatMoneyDto(first))
        }
    }

    // MARK: - Spend summary (list only — donut deferred)

    private var spendSummaryCard: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Spend")
                .font(.subheadline.weight(.semibold))
            ForEach(spendRows, id: \.id) { row in
                HStack {
                    Circle()
                        .fill(severityTint("ok"))
                        .frame(width: 8, height: 8)
                    Text(row.label)
                        .font(.subheadline)
                    Spacer()
                    Text(row.amount)
                        .font(.subheadline.monospacedDigit())
                        .foregroundStyle(.secondary)
                }
            }
            Button {
                store.selectUsageSurface(nil)
                openWindow(id: "usage")
            } label: {
                Text("Open full usage…")
                    .font(.caption.weight(.medium))
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background {
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .fill(Color(nsColor: .controlBackgroundColor).opacity(0.65))
        }
        .accessibilityElement(children: .combine)
    }

    // MARK: - Provider block

    private func providerBlock(_ surface: PresentationStore.SurfaceRow) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            // Header outside the inset card (OpenUsage: Claude  Max 20x).
            Button {
                store.selectUsageSurface(surface.id)
                openWindow(id: "usage")
            } label: {
                HStack(spacing: 8) {
                    Text(statusItemGlyph(compactLabel: surface.label, surfaceId: surface.id))
                        .font(.system(size: 11, weight: .bold, design: .rounded))
                        .frame(width: 22, height: 22)
                        .background {
                            Circle().fill(severityTint(worstSeverity(surface)).opacity(0.22))
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
                            .foregroundStyle(.orange)
                    }
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("\(surface.label), open usage")

            VStack(alignment: .leading, spacing: 14) {
                if surface.buckets.isEmpty {
                    emptyMetric(title: nil)
                } else {
                    ForEach(surface.buckets) { bucket in
                        metricRow(bucket)
                    }
                }
                if let caption = surface.estimateCaption, !caption.isEmpty {
                    Text(caption)
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }
            }
            .padding(14)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background {
                RoundedRectangle(cornerRadius: 14, style: .continuous)
                    .fill(Color(nsColor: .controlBackgroundColor).opacity(0.55))
            }
        }
    }

    private func metricRow(_ bucket: PresentationStore.BucketRow) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(bucket.label)
                .font(.subheadline.weight(.semibold))

            switch bucketRowShape(
                remainingPercent: bucket.remainingPercent,
                usedLabel: bucket.usedLabel
            ) {
            case .gauge:
                if let remaining = bucket.remainingPercent {
                    // OpenUsage: remaining fill grows left-to-right (full = healthy).
                    remainingBar(remaining: remaining, severity: bucket.severity)
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
                    Text(bucket.usedLabel ?? moneyLine(bucket) ?? "—")
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
                emptyMetric(title: nil)
            }

            if let pace = bucket.paceLabel, !pace.isEmpty {
                Text(pace)
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
            if let money = moneyLine(bucket), bucket.usedLabel == nil {
                Text(money)
                    .font(.caption)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
            }
        }
        .accessibilityElement(children: .combine)
    }

    private func moneyLine(_ bucket: PresentationStore.BucketRow) -> String? {
        if let used = bucket.usedMoney {
            if let limit = bucket.limitMoney {
                return "\(formatMoneyDto(used)) / \(formatMoneyDto(limit))"
            }
            return formatMoneyDto(used)
        }
        return nil
    }

    private func emptyMetric(title: String?) -> some View {
        VStack(alignment: .leading, spacing: 6) {
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

    /// Full-width remaining bar (OpenUsage blue track density).
    private func remainingBar(remaining: UInt8, severity: String) -> some View {
        let remainingFrac = Double(remaining) / 100.0
        return GeometryReader { geo in
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(Color.primary.opacity(0.10))
                Capsule()
                    .fill(severityTint(severity))
                    .frame(width: max(3, geo.size.width * remainingFrac))
            }
        }
        .frame(height: 4)
        .accessibilityHidden(true)
    }

    private func worstSeverity(_ surface: PresentationStore.SurfaceRow) -> String {
        let ranks = ["danger": 0, "warn": 1, "ok": 2, "info": 3]
        return surface.buckets
            .map(\.severity)
            .min(by: { (ranks[$0] ?? 9) < (ranks[$1] ?? 9) })
            ?? "ok"
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
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
    }

    private var footerBar: some View {
        VStack(spacing: 0) {
            Divider().opacity(0.25)
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
                    HStack(spacing: 4) {
                        Text("Options")
                            .font(.caption.weight(.semibold))
                        Image(systemName: "chevron.down")
                            .font(.system(size: 8, weight: .bold))
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 6)
                    .background {
                        Capsule()
                            .fill(Color.primary.opacity(0.07))
                    }
                }
                .menuStyle(.borderlessButton)
                .fixedSize()
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 12)
        }
    }
}
