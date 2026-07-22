// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit
import JackinUsageBridge
import SwiftUI

/// Glance popover — CodexBar **Overview** reference (clean-room).
///
/// Reference anatomy (CodexBar Overview):
/// 1. Fixed **agent tile grid** at top (Overview + every surface).
/// 2. When **Overview** is selected: scroll **every agent’s full detail** stacked
///    (Codex block, Claude block, …).
/// 3. When a **single agent** is selected: only that agent’s detailization.
///
/// All numbers/strings are Rust-owned. Multi-account pills ship when the host
/// store has >1 account. Non-goals vs reference: spend charts, token prices,
/// external dashboard URLs (AGENTS limits-only).
struct PopoverRoot: View {
    @ObservedObject var store: PresentationStore
    @Environment(\.openWindow) private var openWindow

    /// `nil` = Overview (stack all agents); otherwise one surface id.
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
                    .padding(.top, 12)
            }

            // 1) Agent catalog — floating chrome island (Liquid Glass navigation layer).
            agentTileGrid
                .padding(10)
                .background {
                    GlassFallbacks.floatingChromeIsland()
                }
                .padding(.horizontal, 12)
                .padding(.top, 12)
                .padding(.bottom, 10)

            // 2) Detailization — content layer (scrolls under glass chrome).
            ScrollView {
                Group {
                    if allAgents.isEmpty {
                        emptyCatalog
                    } else if let id = selectedSurfaceId,
                              let surface = allAgents.first(where: { $0.id == id })
                    {
                        agentDetailBlock(surface, showOpenChevron: true)
                    } else {
                        // Overview: full detail for every agent, stacked.
                        overviewStack
                    }
                }
                .padding(.horizontal, 14)
                .padding(.top, 4)
                .padding(.bottom, 12)
            }
            .frame(maxHeight: 520)

            // 3) Footer actions — glass control strip.
            menuFooter
        }
        .frame(width: 340)
        .background {
            // Detached Tahoe panel — Liquid Glass chrome (not content fill).
            GlassFallbacks.panelSurfaceBackground()
        }
        .clipShape(
            RoundedRectangle(
                cornerRadius: GlassFallbacks.panelCornerRadius,
                style: .continuous
            )
        )
        .onAppear {
            if !store.isOpen {
                store.openDefault()
            }
            // Default to Overview so the agent list + stacked details show first.
            if selectedSurfaceId == nil { /* Overview */ }
        }
    }

    private var allAgents: [PresentationStore.SurfaceRow] {
        store.surfaces
    }

    private var enabledAgents: [PresentationStore.SurfaceRow] {
        allAgents.filter(\.enabled)
    }

    // MARK: - Agent tile grid

    private var agentTileGrid: some View {
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
                    systemImage: agentSystemImage(surface.id),
                    remainingBadgeLines: tileRemainingBadgeLines(for: surface)
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
        systemImage: String?,
        remainingBadgeLines: [String] = []
    ) -> some View {
        let selected = selectedSurfaceId == id
        return Button {
            selectedSurfaceId = id
        } label: {
            VStack(spacing: 4) {
                ZStack {
                    Group {
                        if selected {
                            GlassFallbacks.selectedControlFill()
                        } else {
                            GlassFallbacks.idleControlFill(enabled: enabled)
                        }
                    }
                    .frame(height: 36)
                    if let systemImage {
                        Image(systemName: systemImage)
                            .font(.system(size: 14, weight: .semibold))
                            .foregroundStyle(
                                selected ? Color.white : Color.primary.opacity(enabled ? 0.8 : 0.35)
                            )
                    } else {
                        Text(glyph)
                            .font(.system(size: 12, weight: .bold, design: .rounded))
                            .foregroundStyle(
                                selected ? Color.white : Color.primary.opacity(enabled ? 0.85 : 0.35)
                            )
                    }
                }
                Text(title)
                    .font(.system(size: 10, weight: selected ? .semibold : .regular))
                    .foregroundStyle(
                        selected ? Color.accentColor : Color.secondary.opacity(enabled ? 1 : 0.5)
                    )
                    .lineLimit(1)
                    .minimumScaleFactor(0.75)
                // OpenUsage dual stack under tile (session/weekly remaining or reset).
                if !remainingBadgeLines.isEmpty, enabled {
                    VStack(spacing: 0) {
                        ForEach(Array(remainingBadgeLines.enumerated()), id: \.offset) { _, line in
                            Text(line)
                                .font(
                                    .system(
                                        size: remainingBadgeLines.count > 1 ? 8 : 9,
                                        weight: .semibold,
                                        design: .rounded
                                    )
                                )
                                .monospacedDigit()
                                .foregroundStyle(
                                    selected
                                        ? Color.accentColor.opacity(0.9) : underlineTint(severity)
                                )
                                .lineLimit(1)
                                .minimumScaleFactor(0.65)
                        }
                    }
                } else {
                    Capsule()
                        .fill(selected || !enabled ? Color.clear : underlineTint(severity))
                        .frame(width: 22, height: 2)
                }
            }
            .frame(maxWidth: .infinity)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(
            enabled
                ? (remainingBadgeLines.isEmpty
                    ? title
                    : "\(title) \(remainingBadgeLines.joined(separator: " and "))")
                : "\(title), disabled"
        )
        .accessibilityAddTraits(selected ? .isSelected : [])
    }

    /// Dual remaining stack for agent tiles (OpenUsage menubar density).
    private func tileRemainingBadgeLines(for surface: PresentationStore.SurfaceRow) -> [String] {
        guard surface.enabled else { return [] }
        let remainings = surface.buckets.compactMap(\.remainingPercent)
        guard !remainings.isEmpty else { return [] }
        // When a bucket is depleted, feed its Rust reset label so lines show
        // countdown instead of bare `0%` (same rule as status chips).
        let compact = tileCompactForCountdown(surface)
        return JackinUsageBridge.tileRemainingBadgeLines(
            remainings: remainings,
            compactLabel: compact,
            percentStyle: store.percentStyle,
            maxLines: 2
        )
    }

    /// Compact-like string for depleted tile countdown extraction.
    private func tileCompactForCountdown(_ surface: PresentationStore.SurfaceRow) -> String {
        if let bucket = surface.buckets.first(where: { $0.remainingPercent == 0 }),
           let reset = bucket.resetLabel, !reset.isEmpty
        {
            return reset
        }
        return surface.statusBarLabel
    }

    // MARK: - Overview stack (all agents detailed)

    private var overviewStack: some View {
        VStack(alignment: .leading, spacing: 18) {
            if enabledAgents.isEmpty {
                Text("No agents enabled. Turn them on in Settings.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                SettingsLink { Text("Open Settings…") }
                    .controlSize(.small)
            } else {
                ForEach(enabledAgents) { surface in
                    agentDetailBlock(surface, showOpenChevron: true)
                    if surface.id != enabledAgents.last?.id {
                        Divider().opacity(0.3)
                    }
                }
            }
        }
    }

    // MARK: - Single agent detailization

    private func agentDetailBlock(
        _ surface: PresentationStore.SurfaceRow,
        showOpenChevron: Bool
    ) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            // Identity header (name · account / updated · plan).
            Button {
                if showOpenChevron {
                    selectedSurfaceId = surface.id
                }
                store.selectUsageSurface(surface.id)
                openWindow(id: "usage")
            } label: {
                VStack(alignment: .leading, spacing: 4) {
                    HStack(alignment: .firstTextBaseline) {
                        Text(surface.label)
                            .font(.title3.weight(.semibold))
                        Spacer(minLength: 8)
                        if let account = accountDisplay(surface) {
                            HStack(spacing: 4) {
                                Text(account)
                                    .font(.caption)
                                    .lineLimit(1)
                                    .truncationMode(.middle)
                                if showOpenChevron {
                                    Image(systemName: "chevron.right")
                                        .font(.caption2.weight(.semibold))
                                        .foregroundStyle(.tertiary)
                                }
                            }
                            .foregroundStyle(.secondary)
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
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .disabled(!surface.enabled)

            if !surface.enabled {
                Text("Disabled — enable in Settings to refresh quotas.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Toggle(
                    "Enable \(surface.label)",
                    isOn: Binding(
                        get: { surface.enabled },
                        set: { store.setEnabled(surfaceId: surface.id, enabled: $0) }
                    )
                )
                .toggleStyle(.switch)
                .controlSize(.small)
            } else {
                let surfaceAccounts = store.accountsForSurface(surface.id)
                if surfaceAccounts.count > 1 {
                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack(spacing: 6) {
                            ForEach(surfaceAccounts) { account in
                                Button {
                                    store.setSelectedAccount(
                                        surfaceId: surface.id,
                                        accountKey: account.accountKey
                                    )
                                } label: {
                                    Text(account.accountLabel)
                                        .font(.caption2.weight(account.selected ? .semibold : .regular))
                                        .lineLimit(1)
                                        .padding(.horizontal, 8)
                                        .padding(.vertical, 4)
                                        .background {
                                            Capsule(style: .continuous)
                                                .fill(
                                                    account.selected
                                                        ? Color.accentColor.opacity(0.9)
                                                        : Color.primary.opacity(0.07)
                                                )
                                        }
                                        .foregroundStyle(account.selected ? Color.white : Color.primary)
                                }
                                .buttonStyle(.plain)
                            }
                        }
                    }
                }

                Divider().opacity(0.25)

                VStack(alignment: .leading, spacing: 12) {
                    if surface.buckets.isEmpty {
                        emptyMetric()
                        Text("No quota data yet. Try Refresh after signing in.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    } else {
                        ForEach(surface.buckets) { bucket in
                            metricBlock(bucket)
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
            }
        }
    }

    private func metricBlock(_ bucket: PresentationStore.BucketRow) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            Text(bucket.label)
                .font(.subheadline.weight(.semibold))

            switch bucketRowShape(
                remainingPercent: bucket.remainingPercent,
                usedLabel: bucket.usedLabel
            ) {
            case .gauge:
                if let remaining = bucket.remainingPercent {
                    remainingBar(remaining: remaining, severity: bucket.severity)
                }
                // OpenUsage: primary remaining/used (or depleted reset) · reset right.
                HStack(alignment: .firstTextBaseline) {
                    Text(metricPrimaryLabel(bucket))
                        .font(.caption.weight(.semibold))
                        .monospacedDigit()
                        .foregroundStyle(severityTint(bucket.severity))
                    Spacer(minLength: 8)
                    if let remaining = bucket.remainingPercent, remaining > 0,
                       let reset = bucket.resetLabel, !reset.isEmpty
                    {
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
            return (bucket.label.isEmpty ? "Spend" : bucket.label, formatMoneyDto(money))
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

    /// Glance metric primary: remaining/used % or depleted countdown (CodexBar).
    private func metricPrimaryLabel(_ bucket: PresentationStore.BucketRow) -> String {
        bucketMetricPrimaryLabel(
            remainingPercent: bucket.remainingPercent,
            usedLabel: bucket.usedLabel,
            resetLabel: bucket.resetLabel,
            percentStyle: store.percentStyle
        )
    }

    @ViewBuilder
    private func paceRow(_ pace: String) -> some View {
        let parts = splitPaceLabel(pace)
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

    private func accountDisplay(_ surface: PresentationStore.SurfaceRow) -> String? {
        if let user = surface.username, !user.isEmpty { return user }
        if !surface.accountLabel.isEmpty { return surface.accountLabel }
        return nil
    }

    private func shortTitle(label: String, id: String) -> String {
        switch id {
        case "grok": return "Grok"
        case "zai": return "z.ai"
        case "minimax": return "MiniMax"
        case "opencode": return "OpenCode"
        default: return label.count <= 8 ? label : String(label.prefix(7))
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

    private func barFill(_ severity: String) -> Color {
        switch severity {
        case "danger": return .red
        case "warn": return .orange
        default: return Color(red: 0.40, green: 0.72, blue: 0.78)
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

    // MARK: - Menu

    private var menuFooter: some View {
        VStack(spacing: 2) {
            Divider().opacity(0.28)
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
                    Image(systemName: "clock")
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                    Text(store.nextRefreshLabel)
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                    Spacer()
                }
                .padding(.horizontal, 14)
                .padding(.vertical, 6)
            }
            menuRow(title: "Quit", systemImage: "xmark.square", shortcut: "⌘Q") {
                NSApplication.shared.terminate(nil)
            }
            .keyboardShortcut("q", modifiers: [.command])
        }
        .padding(.bottom, 6)
        .background {
            GlassFallbacks.footerBarBackground()
        }
    }

    private var emptyCatalog: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("No agents available.")
                .font(.body.weight(.medium))
            Text("Sign in with agent CLIs, then Refresh.")
                .font(.caption)
                .foregroundStyle(.secondary)
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
        .padding(.vertical, 9)
        .contentShape(Rectangle())
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
