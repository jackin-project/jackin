// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit
import JackinUsageBridge
import SwiftUI

/// Glance popover — **OpenUsage** reference layout (clean-room).
///
/// Reference: OpenUsage 0.7.x dashboard panel (Cost summary + stacked provider
/// cards + Options footer). Lists every available agent with full metric
/// detailization under its header. All numbers/strings are Rust-owned.
///
/// Deferred vs reference (need more Rust models): donut chart, Today/Yesterday/
/// 30d segmented control, sparklines, external Status/Dashboard URLs.
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
            }

            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    if enabledAgents.isEmpty {
                        emptyState
                    } else {
                        // Spend legend (OpenUsage Cost card without donut).
                        if !spendLegend.isEmpty {
                            spendCard
                        }

                        // Every available agent + full metric detailization.
                        ForEach(enabledAgents) { surface in
                            agentSection(surface)
                        }
                    }
                }
                .padding(.horizontal, 14)
                .padding(.top, 14)
                .padding(.bottom, 10)
            }
            .frame(maxHeight: 560)

            optionsFooter
        }
        .frame(width: 320)
        .background {
            // OpenUsage: solid elevated white card.
            RoundedRectangle(cornerRadius: 20, style: .continuous)
                .fill(Color(nsColor: .windowBackgroundColor))
                .shadow(color: .black.opacity(0.16), radius: 28, y: 10)
        }
        .clipShape(RoundedRectangle(cornerRadius: 20, style: .continuous))
        .onAppear {
            if !store.isOpen {
                store.openDefault()
            }
        }
    }

    /// Enabled agents (available for usage display).
    private var enabledAgents: [PresentationStore.SurfaceRow] {
        store.surfaces.filter(\.enabled)
    }

    /// Per-agent spend lines from Rust money (no invented grand total).
    private var spendLegend: [(id: String, label: String, amount: String, color: Color)] {
        let palette: [Color] = [
            Color(red: 0.30, green: 0.70, blue: 0.55),
            Color(red: 0.90, green: 0.45, blue: 0.35),
            Color(red: 0.45, green: 0.50, blue: 0.55),
            Color.accentColor,
        ]
        var i = 0
        return enabledAgents.compactMap { surface in
            guard let money = surface.buckets.compactMap(\.usedMoney).first else {
                return nil
            }
            let color = palette[i % palette.count]
            i += 1
            return (surface.id, surface.label, formatMoneyDto(money), color)
        }
    }

    // MARK: - Spend card (OpenUsage Cost header, list form)

    private var spendCard: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text("Spend")
                    .font(.subheadline.weight(.semibold))
                Spacer()
                Button {
                    store.selectUsageSurface(nil)
                    openWindow(id: "usage")
                } label: {
                    Image(systemName: "arrow.up.right.square")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Open full usage")
            }

            ForEach(spendLegend, id: \.id) { row in
                HStack(spacing: 8) {
                    Circle()
                        .fill(row.color)
                        .frame(width: 8, height: 8)
                    Text(row.label)
                        .font(.subheadline)
                    Spacer()
                    Text(row.amount)
                        .font(.subheadline.monospacedDigit())
                        .foregroundStyle(.secondary)
                }
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background {
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .fill(Color(nsColor: .controlBackgroundColor).opacity(0.65))
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel("Spend by agent")
    }

    // MARK: - Agent section (OpenUsage provider card)

    private func agentSection(_ surface: PresentationStore.SurfaceRow) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            // Header outside inset card: glyph + name + plan.
            Button {
                store.selectUsageSurface(surface.id)
                openWindow(id: "usage")
            } label: {
                HStack(spacing: 8) {
                    agentGlyph(surface)
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

            // Inset metric group.
            VStack(alignment: .leading, spacing: 14) {
                if surface.buckets.isEmpty {
                    emptyMetricRow()
                } else {
                    ForEach(surface.buckets) { bucket in
                        metricRow(bucket)
                    }
                }

                if let caption = surface.estimateCaption, !caption.isEmpty {
                    Text(caption)
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                        .fixedSize(horizontal: false, vertical: true)
                }

                if let err = surface.lastError, !err.isEmpty {
                    Text(err)
                        .font(.caption)
                        .foregroundStyle(.orange)
                }

                // Account / updated meta footer inside card.
                HStack {
                    Text(surface.updatedLabel.isEmpty ? "—" : surface.updatedLabel)
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                    Spacer()
                    if let account = accountDisplay(surface) {
                        Text(account)
                            .font(.caption2)
                            .foregroundStyle(.tertiary)
                            .lineLimit(1)
                            .truncationMode(.middle)
                    }
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

    @ViewBuilder
    private func agentGlyph(_ surface: PresentationStore.SurfaceRow) -> some View {
        let severity = worstSeverity(surface)
        ZStack {
            Circle()
                .fill(severityTint(severity).opacity(0.18))
                .frame(width: 22, height: 22)
            if let symbol = agentSystemImage(surface.id) {
                Image(systemName: symbol)
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(severityTint(severity))
            } else {
                Text(statusItemGlyph(compactLabel: surface.label, surfaceId: surface.id))
                    .font(.system(size: 10, weight: .bold, design: .rounded))
                    .foregroundStyle(severityTint(severity))
            }
        }
    }

    private func agentSystemImage(_ id: String) -> String? {
        switch id {
        case "claude": return "sparkles"
        case "codex": return "circle.hexagongrid.fill"
        case "amp": return "waveform"
        case "grok": return "circle.dashed"
        case "zai": return "z.square.fill"
        case "kimi": return "k.circle"
        case "minimax": return "waveform.path"
        case "opencode": return "chevron.left.forwardslash.chevron.right"
        default: return nil
        }
    }

    private func accountDisplay(_ surface: PresentationStore.SurfaceRow) -> String? {
        if let user = surface.username, !user.isEmpty { return user }
        if !surface.accountLabel.isEmpty { return surface.accountLabel }
        return nil
    }

    // MARK: - Metric row (OpenUsage Session / Weekly anatomy)

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
                    // OpenUsage: remaining fill grows L→R (full = healthy).
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
                if let pace = bucket.paceLabel, !pace.isEmpty {
                    Text(pace)
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }
            case .valueOnly:
                if let desc = bucket.limitLabel ?? bucket.statusSlot, !desc.isEmpty {
                    Text(desc)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
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
                emptyMetricRow()
            }
        }
        .accessibilityElement(children: .combine)
    }

    private func moneyLine(_ bucket: PresentationStore.BucketRow) -> String? {
        bucket.usedMoney.map(formatMoneyDto)
    }

    private func emptyMetricRow() -> some View {
        VStack(alignment: .leading, spacing: 6) {
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

    private func remainingBar(remaining: UInt8, severity: String) -> some View {
        let frac = Double(remaining) / 100.0
        return GeometryReader { geo in
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(Color.primary.opacity(0.10))
                Capsule()
                    .fill(openUsageBarFill(severity))
                    .frame(width: max(3, geo.size.width * frac))
            }
        }
        .frame(height: 4)
        .accessibilityHidden(true)
    }

    /// OpenUsage uses a single blue accent for healthy bars.
    private func openUsageBarFill(_ severity: String) -> Color {
        switch severity {
        case "danger": return .red
        case "warn": return .orange
        default: return Color.accentColor
        }
    }

    private func worstSeverity(_ surface: PresentationStore.SurfaceRow) -> String {
        let ranks = ["danger": 0, "warn": 1, "ok": 2, "info": 3]
        return surface.buckets
            .map(\.severity)
            .min(by: { (ranks[$0] ?? 9) < (ranks[$1] ?? 9) })
            ?? "ok"
    }

    // MARK: - Footer (OpenUsage Options)

    private var optionsFooter: some View {
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
                        Capsule().fill(Color.primary.opacity(0.07))
                    }
                }
                .menuStyle(.borderlessButton)
                .fixedSize()
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 12)
        }
    }

    private var emptyState: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("No agents enabled.")
                .font(.body.weight(.medium))
            Text(
                "Enable agents in Settings. jackin❯ Desktop reads credentials your agent CLIs already store."
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
    }
}
