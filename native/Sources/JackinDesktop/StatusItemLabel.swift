// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit
import JackinUsageBridge
import SwiftUI

/// Menu-bar status item — **CodexBar-style per-provider usage preview**.
///
/// One chip per enabled provider with data: icon/glyph + stacked short percents
/// (session/weekly). Clean-room look-and-feel; remainings from Rust only.
struct StatusItemLabel: View {
    @ObservedObject var store: PresentationStore

    var body: some View {
        HStack(spacing: 7) {
            if store.statusItemChips.isEmpty {
                statusIcon
                    .opacity(itemOpacity)
                if !store.statusItemText.isEmpty {
                    Text(store.statusItemText)
                        .font(Self.chipFont)
                        .monospacedDigit()
                        .opacity(itemOpacity)
                }
            } else {
                // Per-provider preview strip (CodexBar).
                ForEach(store.statusItemChips) { chip in
                    StatusItemChipView(chip: chip)
                        .opacity(itemOpacity)
                }
            }
        }
        .frame(minWidth: 18, minHeight: 22)
        .padding(.horizontal, 2)
        .accessibilityLabel(accessibilityText)
        .onAppear {
            if !store.isOpen {
                store.openDefault()
            }
        }
    }

    private var itemOpacity: Double {
        store.allEnabledSurfacesDegraded ? 0.45 : 1.0
    }

    @ViewBuilder
    private var statusIcon: some View {
        if let mark = Self.loadLogomark() {
            Image(nsImage: mark)
                .renderingMode(.template)
                .resizable()
                .interpolation(.high)
                .frame(width: 14, height: 14)
        } else {
            Image(systemName: "gauge.with.needle")
                .symbolRenderingMode(.monochrome)
                .imageScale(.small)
                .frame(width: 14, height: 14)
        }
    }

    private var accessibilityText: String {
        if !store.statusItemChips.isEmpty {
            return statusItemAccessibilityLabel(chips: store.statusItemChips)
        }
        if !store.statusItemText.isEmpty {
            return "jackin Desktop \(store.statusItemText)"
        }
        return "jackin Desktop"
    }

    static let chipFont = Font.system(size: 10, weight: .semibold, design: .rounded)

    private static func loadLogomark() -> NSImage? {
        let bundle = Bundle.module
        let url =
            bundle.url(forResource: "JackinMark", withExtension: "pdf")
            ?? bundle.url(forResource: "JackinMark", withExtension: "PDF")
        guard let url else { return nil }
        guard let image = NSImage(contentsOf: url) else { return nil }
        image.isTemplate = true
        image.size = NSSize(width: 14, height: 14)
        return image
    }
}

/// One provider chip: icon + stacked percents (and optional mini remaining bars).
private struct StatusItemChipView: View {
    let chip: StatusItemChip

    var body: some View {
        HStack(spacing: 3) {
            providerMark

            if chip.percentLines.isEmpty {
                Text(chip.compactLabel)
                    .font(StatusItemLabel.chipFont)
                    .monospacedDigit()
                    .foregroundStyle(severityTint(chip.severity))
                    .lineLimit(1)
            } else {
                VStack(alignment: .trailing, spacing: 1) {
                    ForEach(Array(chip.percentLines.enumerated()), id: \.offset) { index, line in
                        let lineSeverity =
                            index < chip.severityPerLine.count
                            ? chip.severityPerLine[index]
                            : chip.severity
                        HStack(spacing: 2) {
                            if index < chip.remainingPerLine.count {
                                miniBar(
                                    remaining: chip.remainingPerLine[index],
                                    severity: lineSeverity
                                )
                            }
                            Text(line)
                                .font(StatusItemLabel.chipFont)
                                .monospacedDigit()
                                .foregroundStyle(severityTint(lineSeverity))
                                .lineLimit(1)
                        }
                    }
                }
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(chip.compactLabel)
    }

    @ViewBuilder
    private var providerMark: some View {
        ZStack {
            Circle()
                .fill(severityTint(chip.severity).opacity(0.22))
                .frame(width: 15, height: 15)
            if let systemImage = chip.systemImage {
                Image(systemName: systemImage)
                    .font(.system(size: 8, weight: .bold))
                    .foregroundStyle(severityTint(chip.severity))
            } else {
                Text(chip.glyph)
                    .font(.system(size: 8, weight: .bold, design: .rounded))
                    .foregroundStyle(.primary)
            }
        }
    }

    private func miniBar(remaining: UInt8, severity: String) -> some View {
        let frac = Double(remaining) / 100.0
        return GeometryReader { geo in
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(Color.primary.opacity(0.15))
                Capsule()
                    .fill(severityTint(severity))
                    .frame(width: max(1.5, geo.size.width * frac))
            }
        }
        .frame(width: 12, height: 3)
        .accessibilityHidden(true)
    }
}
