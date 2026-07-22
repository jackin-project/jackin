// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit
import JackinUsageBridge
import SwiftUI

/// Glance popover — CodexBar-style agent list + detail (clean-room).
///
/// **Top:** Overview + every host surface tile (Claude, Codex, Amp, …) whether
/// enabled or not. **Below:** detailization for the selected agent (identity,
/// quota bars, money, pace). Menu footer with key equivalents.
/// All numbers/strings are Rust-owned.
struct PopoverRoot: View {
    @ObservedObject var store: PresentationStore
    @Environment(\.openWindow) private var openWindow

    /// `nil` = Overview; otherwise surface id from `listSurfaces`.
    @State private var selectedSurfaceId: String?

    private let tileColumns = [
        GridItem(.flexible(), spacing: 6),
        GridItem(.flexible(), spacing: 6),
        GridItem(.flexible(), spacing: 6),
        GridItem(.flexible(), spacing: 6),
    ]

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if let err = store.lastError {
                Text(err)
                    .font(.caption)
                    .foregroundStyle(.red)
                    .padding(.horizontal, 14)
                    .padding(.top, 10)
            }

            // Fixed agent strip (CodexBar): always visible above detail.
            VStack(alignment: .leading, spacing: 10) {
                Text("Agents")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 14)
                    .padding(.top, 12)
                providerTileGrid
                    .padding(.horizontal, 10)
            }

            Divider().opacity(0.35)
                .padding(.top, 10)

            ScrollView {
                Group {
                    if store.surfaces.isEmpty {
                        emptyState
                    } else {
                        detailPane
                    }
                }
                .padding(.horizontal, 14)
                .padding(.top, 12)
                .padding(.bottom, 8)
            }
            .frame(maxHeight: 480)

            menuFooter
        }
        .frame(width: 320)
        .background {
            RoundedRectangle(cornerRadius: 18, style: .continuous)
                .fill(Color(nsColor: .windowBackgroundColor))
                .shadow(color: .black.opacity(0.18), radius: 28, y: 10)
        }
        .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
        .onAppear {
            if !store.isOpen {
                store.openDefault()
            }
            ensureSelection()
        }
        .onChange(of: store.surfaces) { _, _ in
            ensureSelection()
        }
    }

    /// Every surface from Rust `listSurfaces` (full agent catalog).
    private var allAgents: [PresentationStore.SurfaceRow] {
        store.surfaces
    }

    private var selectedSurface: PresentationStore.SurfaceRow? {
        guard let id = selectedSurfaceId else { return nil }
        return allAgents.first(where: { $0.id == id })
    }

    private func ensureSelection() {
        if let id = selectedSurfaceId, allAgents.contains(where: { $0.id == id }) {
            return
        }
        // Prefer first agent that has live buckets; else first agent; else Overview.
        if let withData = allAgents.first(where: { $0.enabled && !$0.buckets.isEmpty }) {
            selectedSurfaceId = withData.id
        } else if let first = allAgents.first {
            selectedSurfaceId = first.id
        } else {
            selectedSurfaceId = nil
        }
    }

    // MARK: - Agent tile grid

    private var providerTileGrid: some View {
        LazyVGrid(columns: tileColumns, spacing: 8) {
            tileButton(
                id: nil,
                title: "Overview",
                glyph: "",
                severity: "ok",
                enabled: true,
                systemImage: "square.grid.2x2"
            )
            ForEach(allAgents) { surface in
                tileButton(
                    id: surface.id,
                    title: shortTitle(label: surface.label, id: surface.id),
                    glyph: statusItemGlyph(compactLabel: surface.label, surfaceId: surface.id),
                    severity: worstSeverity(surface),
                    enabled: surface.enabled,
                    systemImage: tileSystemImage(surfaceId: surface.id)
                )
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Agents")
    }

    private func tileButton(
        id: String?,
        title: String,
        glyph: String,
        severity: String,
        enabled: Bool,
        systemImage: String?
    ) -> some View {
        let selected = selectedSurfaceId == id
        return Button {
            selectedSurfaceId = id
        } label: {
            VStack(spacing: 4) {
                ZStack {
                    RoundedRectangle(cornerRadius: 10, style: .continuous)
                        .fill(selected ? Color.accentColor : Color.primary.opacity(enabled ? 0.06 : 0.03))
                        .frame(height: 36)
                    if let systemImage {
                        Image(systemName: systemImage)
                            .font(.system(size: 14, weight: .semibold))
                            .foregroundStyle(
                                selected
                                    ? Color.white
                                    : Color.primary.opacity(enabled ? 0.8 : 0.35)
                            )
                    } else {
                        Text(glyph)
                            .font(.system(size: 12, weight: .bold, design: .rounded))
                            .foregroundStyle(
                                selected
                                    ? Color.white
                                    : Color.primary.opacity(enabled ? 0.85 : 0.35)
                            )
                    }
                }
                Text(title)
                    .font(.system(size: 10, weight: selected ? .semibold : .regular))
                    .foregroundStyle(
                        selected
                            ? Color.accentColor
                            : Color.secondary.opacity(enabled ? 1 : 0.5)
                    )
                    .lineLimit(1)
                    .minimumScaleFactor(0.75)
                // Severity underline (Amp red / Grok green) when not selected.
                Capsule()
                    .fill(selected || !enabled ? Color.clear : underlineTint(severity))
                    .frame(width: 22, height: 2)
            }
            .frame(maxWidth: .infinity)
            .contentShape(Rectangle())
            .opacity(enabled || selected ? 1 : 0.85)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(enabled ? title : "\(title), disabled")
        .accessibilityAddTraits(selected ? .isSelected : [])
    }

    private func shortTitle(label: String, id: String) -> String {
        switch id {
        case "grok": return "Grok"
        case "zai": return "z.ai"
        case "minimax": return "MiniMax"
        case "opencode": return "OpenCode"
        default:
            return label.count <= 8 ? label : String(label.prefix(7))
        }
    }

    /// SF Symbol for known surface ids (layout only — not a provider zoo).
    private func tileSystemImage(surfaceId: String) -> String? {
        switch surfaceId {
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

    // MARK: - Detail (selected agent)

    @ViewBuilder
    private var detailPane: some View {
        if let surface = selectedSurface {
            if surface.enabled {
                providerDetail(surface)
            } else {
                disabledDetail(surface)
            }
        } else {
            overviewDetail
        }
    }

    private var overviewDetail: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Overview")
                .font(.title3.weight(.semibold))
            Text("All enabled agents")
                .font(.caption)
                .foregroundStyle(.secondary)

            ForEach(store.overviewRows) { row in
                Button {
                    selectedSurfaceId = row.surfaceId
                } label: {
                    HStack(spacing: 8) {
                        Circle()
                            .fill(severityTint(row.severity))
                            .frame(width: 7, height: 7)
                        Text(row.displayLabel)
                            .font(.body.weight(.medium))
                            .lineLimit(1)
                        Spacer(minLength: 6)
                        trailingOverview(row)
                    }
                    .padding(.vertical, 5)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }

            if store.overviewRows.isEmpty {
                Text("No fresh usage rows yet — select an agent or Refresh.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
    }

    @ViewBuilder
    private func trailingOverview(_ row: PresentationStore.OverviewRow) -> some View {
        switch overviewGlanceBody(
            headline: row.headline,
            resetLabel: row.resetLabel,
            statusWord: row.statusWord
        ) {
        case .numeric(let headline, let reset):
            VStack(alignment: .trailing, spacing: 1) {
                Text(headline)
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
                if let reset {
                    Text(reset)
                        .font(.caption2.monospacedDigit())
                        .foregroundStyle(.tertiary)
                }
            }
        case .statusWord(let word):
            Text(word)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    private func disabledDetail(_ surface: PresentationStore.SurfaceRow) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            identityHeader(surface)
            Text("This agent is disabled in Settings. Enable it to refresh quotas and show bars.")
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            Toggle(
                "Enable \(surface.label)",
                isOn: Binding(
                    get: { surface.enabled },
                    set: { store.setEnabled(surfaceId: surface.id, enabled: $0) }
                )
            )
            .toggleStyle(.switch)
            .controlSize(.small)
        }
    }

    private func providerDetail(_ surface: PresentationStore.SurfaceRow) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            identityHeader(surface)
                .padding(.bottom, 10)

            Divider().opacity(0.3)

            VStack(alignment: .leading, spacing: 14) {
                if surface.buckets.isEmpty {
                    emptyMetric()
                    Text("No quota data yet. Try Refresh after signing in with this agent.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                } else {
                    ForEach(surface.buckets) { bucket in
                        metricBlock(bucket)
                        Divider().opacity(0.2)
                    }
                }

                moneyGrid(surface)

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
            }
            .padding(.top, 12)

            Button {
                store.selectUsageSurface(surface.id)
                openWindow(id: "usage")
            } label: {
                HStack {
                    Label("Open full usage", systemImage: "arrow.up.right.square")
                    Spacer()
                }
                .font(.body)
                .padding(.top, 8)
            }
            .buttonStyle(.plain)
        }
    }

    private func identityHeader(_ surface: PresentationStore.SurfaceRow) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(alignment: .firstTextBaseline) {
                Text(surface.label)
                    .font(.title3.weight(.semibold))
                Spacer(minLength: 8)
                if let account = accountDisplay(surface) {
                    Text(account)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
            }
            HStack {
                Text(surface.updatedLabel.isEmpty ? "—" : surface.updatedLabel)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Spacer()
                if let plan = surface.planLabel, !plan.isEmpty {
                    Text(plan)
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.secondary)
                }
            }
        }
    }

    private func accountDisplay(_ surface: PresentationStore.SurfaceRow) -> String? {
        if let user = surface.username, !user.isEmpty { return user }
        if !surface.accountLabel.isEmpty { return surface.accountLabel }
        return nil
    }

    private func metricBlock(_ bucket: PresentationStore.BucketRow) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(bucket.label)
                .font(.body.weight(.semibold))

            switch bucketRowShape(
                remainingPercent: bucket.remainingPercent,
                usedLabel: bucket.usedLabel
            ) {
            case .gauge:
                if let remaining = bucket.remainingPercent {
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
                    }
                }
                if let pace = bucket.paceLabel, !pace.isEmpty {
                    paceRow(pace)
                }
            case .valueOnly:
                if let desc = bucket.limitLabel ?? bucket.statusSlot, !desc.isEmpty {
                    Text(desc)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                if let used = bucket.usedLabel, !used.isEmpty {
                    Text(used)
                        .font(.caption.monospacedDigit())
                        .foregroundStyle(.secondary)
                }
            case .empty:
                emptyMetric()
            }
        }
    }

    private func moneyGrid(_ surface: PresentationStore.SurfaceRow) -> some View {
        let pairs: [(String, String)] = surface.buckets.compactMap { bucket in
            guard let money = bucket.usedMoney else { return nil }
            let title = bucket.label.isEmpty ? "Spend" : bucket.label
            return (title, formatMoneyDto(money))
        }
        return Group {
            if !pairs.isEmpty {
                LazyVGrid(
                    columns: [GridItem(.flexible()), GridItem(.flexible())],
                    alignment: .leading,
                    spacing: 12
                ) {
                    ForEach(Array(pairs.enumerated()), id: \.offset) { _, pair in
                        VStack(alignment: .leading, spacing: 2) {
                            Text(pair.0)
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                            Text(pair.1)
                                .font(.title3.weight(.semibold).monospacedDigit())
                        }
                    }
                }
                .padding(.top, 2)
            }
        }
    }

    private func emptyMetric() -> some View {
        VStack(alignment: .leading, spacing: 5) {
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

    @ViewBuilder
    private func paceRow(_ pace: String) -> some View {
        let parts = splitPace(pace)
        if parts.count >= 2 {
            HStack(alignment: .firstTextBaseline) {
                Text(parts[0])
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
                    .monospacedDigit()
                Spacer(minLength: 8)
                Text(parts[1])
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
                    .multilineTextAlignment(.trailing)
            }
        } else {
            Text(pace)
                .font(.caption2)
                .foregroundStyle(.tertiary)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    private func splitPace(_ pace: String) -> [String] {
        for sep in [" · ", " • ", " | ", " — "] {
            let bits = pace.components(separatedBy: sep)
            if bits.count >= 2 {
                return [
                    bits[0].trimmingCharacters(in: .whitespaces),
                    bits.dropFirst().joined(separator: sep).trimmingCharacters(in: .whitespaces),
                ]
            }
        }
        return [pace]
    }

    private func remainingBar(remaining: UInt8, severity: String) -> some View {
        let frac = Double(remaining) / 100.0
        return GeometryReader { geo in
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(Color.primary.opacity(0.10))
                Capsule()
                    .fill(barFill(severity))
                    .frame(width: max(3, geo.size.width * frac))
                HStack(spacing: 0) {
                    ForEach(0..<4, id: \.self) { i in
                        if i > 0 {
                            Rectangle()
                                .fill(Color(nsColor: .windowBackgroundColor).opacity(0.85))
                                .frame(width: 2, height: 7)
                        }
                        Spacer(minLength: 0)
                    }
                }
            }
        }
        .frame(height: 6)
        .accessibilityHidden(true)
    }

    private func barFill(_ severity: String) -> Color {
        switch severity {
        case "danger": return .red
        case "warn": return .orange
        default: return Color(red: 0.35, green: 0.72, blue: 0.55)
        }
    }

    private func underlineTint(_ severity: String) -> Color {
        switch severity {
        case "danger": return .red
        case "warn": return .orange
        case "ok": return Color(red: 0.35, green: 0.72, blue: 0.55).opacity(0.9)
        default: return .clear
        }
    }

    private func worstSeverity(_ surface: PresentationStore.SurfaceRow) -> String {
        let ranks = ["danger": 0, "warn": 1, "ok": 2, "info": 3]
        return surface.buckets
            .map(\.severity)
            .min(by: { (ranks[$0] ?? 9) < (ranks[$1] ?? 9) })
            ?? (surface.enabled ? "ok" : "info")
    }

    // MARK: - Menu footer

    private var menuFooter: some View {
        VStack(spacing: 0) {
            Divider().opacity(0.35)
            menuRow(title: "Open Usage…", systemImage: "rectangle.split.2x1", shortcut: nil) {
                store.selectUsageSurface(selectedSurfaceId)
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
            if !store.nextRefreshLabel.isEmpty {
                HStack {
                    Text(store.nextRefreshLabel)
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                    Spacer()
                }
                .padding(.horizontal, 14)
                .padding(.vertical, 4)
            }
            menuRow(title: "Quit", systemImage: "xmark.square", shortcut: "⌘Q") {
                NSApplication.shared.terminate(nil)
            }
            .keyboardShortcut("q", modifiers: [.command])
        }
    }

    private var emptyState: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("No agents available.")
                .font(.body.weight(.medium))
            Text(
                "jackin❯ Desktop lists every supported agent surface. Sign in with an agent CLI, then Refresh."
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
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .contentShape(Rectangle())
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
