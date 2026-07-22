// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit
import JackinUsageBridge
import SwiftUI

/// Glance popover — CodexBar-inspired panel (clean-room layout).
///
/// Glyph tile grid for Overview + providers (selected tile filled accent;
/// severity underline on degraded surfaces). Detail pane shows Capsule-parity
/// fields for the selection. Menu-style footer with key equivalents.
/// All numbers/strings are Rust-owned; no provider probes in Swift.
struct PopoverRoot: View {
    @ObservedObject var store: PresentationStore
    @Environment(\.openWindow) private var openWindow

    /// `nil` = Overview tile; otherwise surface id.
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

            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    if enabledSurfaces.isEmpty {
                        emptyState
                    } else {
                        providerTileGrid
                        Divider().opacity(0.35)
                        detailPane
                    }
                }
                .padding(.horizontal, 12)
                .padding(.top, 12)
                .padding(.bottom, 6)
            }
            .frame(maxHeight: 520)

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
            // Prefer first overview row / enabled surface for a useful first paint.
            if selectedSurfaceId == nil,
               let first = store.overviewRows.first?.surfaceId
                ?? enabledSurfaces.first?.id
            {
                selectedSurfaceId = first
            }
        }
    }

    private var enabledSurfaces: [PresentationStore.SurfaceRow] {
        store.surfaces.filter(\.enabled)
    }

    private var selectedSurface: PresentationStore.SurfaceRow? {
        guard let id = selectedSurfaceId else { return nil }
        return enabledSurfaces.first(where: { $0.id == id })
    }

    // MARK: - Provider tile grid (CodexBar switcher)

    private var providerTileGrid: some View {
        LazyVGrid(columns: tileColumns, spacing: 8) {
            tileButton(
                id: nil,
                title: "Overview",
                glyph: "▦",
                severity: "ok",
                systemImage: "square.grid.2x2"
            )
            ForEach(enabledSurfaces) { surface in
                tileButton(
                    id: surface.id,
                    title: shortTitle(surface.label),
                    glyph: statusItemGlyph(compactLabel: surface.label, surfaceId: surface.id),
                    severity: worstSeverity(surface),
                    systemImage: nil
                )
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Providers")
    }

    private func tileButton(
        id: String?,
        title: String,
        glyph: String,
        severity: String,
        systemImage: String?
    ) -> some View {
        let selected = selectedSurfaceId == id
        return Button {
            selectedSurfaceId = id
        } label: {
            VStack(spacing: 4) {
                ZStack {
                    RoundedRectangle(cornerRadius: 10, style: .continuous)
                        .fill(selected ? Color.accentColor : Color.primary.opacity(0.06))
                        .frame(height: 36)
                    if let systemImage {
                        Image(systemName: systemImage)
                            .font(.system(size: 14, weight: .semibold))
                            .foregroundStyle(selected ? Color.white : Color.primary.opacity(0.75))
                    } else {
                        Text(glyph)
                            .font(.system(size: 12, weight: .bold, design: .rounded))
                            .foregroundStyle(selected ? Color.white : Color.primary.opacity(0.85))
                    }
                }
                Text(title)
                    .font(.system(size: 10, weight: selected ? .semibold : .regular))
                    .foregroundStyle(selected ? Color.accentColor : Color.secondary)
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
                // Severity underline (CodexBar Amp red / Grok green style).
                Capsule()
                    .fill(selected ? Color.clear : underlineTint(severity))
                    .frame(width: 22, height: 2)
            }
            .frame(maxWidth: .infinity)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(title)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }

    private func shortTitle(_ label: String) -> String {
        // Keep tile captions compact (Codex / Claude / z.ai).
        if label.count <= 8 { return label }
        return String(label.prefix(7))
    }

    private func underlineTint(_ severity: String) -> Color {
        switch severity {
        case "danger": return .red
        case "warn": return .orange
        case "ok": return .green.opacity(0.0) // healthy: no underline noise
        default: return .clear
        }
    }

    // MARK: - Detail pane

    @ViewBuilder
    private var detailPane: some View {
        if let surface = selectedSurface {
            providerDetail(surface)
        } else {
            overviewDetail
        }
    }

    private var overviewDetail: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Overview")
                .font(.title3.weight(.semibold))
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
                    .padding(.vertical, 4)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
            if store.overviewRows.isEmpty {
                Text("No fresh usage rows yet.")
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

    private func providerDetail(_ surface: PresentationStore.SurfaceRow) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            // Account pill (CodexBar multi-account strip — single pill when one account).
            if let account = accountDisplay(surface) {
                accountPill(account)
            }

            // Identity two-column (name/account · updated/plan).
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

            Divider().opacity(0.25)

            ForEach(surface.buckets) { bucket in
                metricBlock(bucket)
            }

            if surface.buckets.isEmpty {
                emptyMetric()
            }

            // statusSlot rows (e.g. reset-credits style captions from Rust).
            ForEach(statusSlotRows(surface), id: \.self) { slot in
                Text(slot)
                    .font(.caption)
                    .foregroundStyle(.secondary)
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

            if let origin = surface.credentialOrigin, !origin.isEmpty {
                Text("Auth: \(origin)")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }

            Button {
                store.selectUsageSurface(surface.id)
                openWindow(id: "usage")
            } label: {
                HStack {
                    Text("Open full usage")
                    Spacer()
                    Image(systemName: "chevron.right")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.tertiary)
                }
                .font(.body)
                .padding(.top, 2)
            }
            .buttonStyle(.plain)
        }
    }

    private func accountDisplay(_ surface: PresentationStore.SurfaceRow) -> String? {
        if let user = surface.username, !user.isEmpty { return user }
        if !surface.accountLabel.isEmpty { return surface.accountLabel }
        return nil
    }

    private func accountPill(_ account: String) -> some View {
        Text(account)
            .font(.caption.weight(.semibold))
            .padding(.horizontal, 10)
            .padding(.vertical, 5)
            .background {
                Capsule().fill(Color.accentColor)
            }
            .foregroundStyle(.white)
            .lineLimit(1)
            .accessibilityLabel("Account \(account)")
    }

    private func statusSlotRows(_ surface: PresentationStore.SurfaceRow) -> [String] {
        surface.buckets.compactMap { bucket in
            guard let slot = bucket.statusSlot, !slot.isEmpty else { return nil }
            return slot
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
                // Primary captions: used/left · reset (CodexBar).
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
                // Secondary pace row (reserve / lasts-until) when Rust provides it.
                if let pace = bucket.paceLabel, !pace.isEmpty {
                    Text(pace)
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            case .valueOnly:
                HStack {
                    Text(bucket.usedLabel ?? "—")
                        .font(.caption.monospacedDigit())
                        .foregroundStyle(.secondary)
                    Spacer()
                    if let limit = bucket.limitLabel, !limit.isEmpty {
                        Text(limit)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    } else if let reset = bucket.resetLabel {
                        Text(reset)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
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

    private func remainingBar(remaining: UInt8, severity: String) -> some View {
        let frac = Double(remaining) / 100.0
        // Soft tick marks (CodexBar segmented look) — layout only, not new metrics.
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
        // CodexBar teal for codex-ish healthy fill; accent otherwise.
        default: return Color(red: 0.40, green: 0.72, blue: 0.78)
        }
    }

    private func worstSeverity(_ surface: PresentationStore.SurfaceRow) -> String {
        let ranks = ["danger": 0, "warn": 1, "ok": 2, "info": 3]
        return surface.buckets
            .map(\.severity)
            .min(by: { (ranks[$0] ?? 9) < (ranks[$1] ?? 9) })
            ?? "ok"
    }

    // MARK: - Menu footer (CodexBar)

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
