// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit
import JackinUsageBridge
import SwiftUI

/// Menu-bar status item — **OpenUsage-style multi-provider strip** with
/// Liquid Glass chip capsules (macOS 26+) / ultraThinMaterial fallback.
///
/// One chip per enabled provider: **provider icon + remaining %**
/// (stacked session/weekly when Rust supplies dual buckets). Remainings from
/// Rust only — never invents percentages.
struct StatusItemLabel: View {
    @ObservedObject var store: PresentationStore

    var body: some View {
        HStack(spacing: 5) {
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
                // OpenUsage: [icon 99%/63%] [icon 100%/81%] … for each provider.
                ForEach(store.statusItemChips) { chip in
                    StatusItemChipView(chip: chip)
                        .opacity(itemOpacity)
                }
            }
        }
        .frame(minWidth: 18, minHeight: 22)
        .padding(.horizontal, 1)
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

    static let chipFont = Font.system(size: 11, weight: .semibold, design: .rounded)
    static let dualFont = Font.system(size: 10, weight: .semibold, design: .rounded)

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

/// One provider chip: glass capsule + icon + remaining % + CodexBar dual mini bars.
///
/// OpenUsage Text density (stacked %) plus per-line remaining meters when Rust
/// supplies numeric remainings. Countdown / empty lines skip the meter.
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
            } else if chip.percentLines.count == 1 {
                metricLine(
                    line: chip.percentLines[0],
                    severity: chip.severity,
                    remainingIndex: 0,
                    dual: false
                )
            } else {
                // CodexBar dual stack: session on top, weekly under — each with mini bar.
                VStack(alignment: .trailing, spacing: 1) {
                    ForEach(Array(chip.percentLines.enumerated()), id: \.offset) { index, line in
                        let lineSeverity =
                            index < chip.severityPerLine.count
                            ? chip.severityPerLine[index]
                            : chip.severity
                        metricLine(
                            line: line,
                            severity: lineSeverity,
                            remainingIndex: index,
                            dual: true
                        )
                    }
                }
            }
        }
        .padding(.horizontal, 5)
        .padding(.vertical, 2)
        .background {
            GlassFallbacks.statusItemChipBackground(severity: severityTint(chip.severity))
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(
            chip.percentLines.isEmpty
                ? chip.compactLabel
                : "\(chip.glyph) \(chip.percentLines.joined(separator: " "))"
        )
    }

    @ViewBuilder
    private func metricLine(
        line: String,
        severity: String,
        remainingIndex: Int,
        dual: Bool
    ) -> some View {
        HStack(spacing: 3) {
            Text(line)
                .font(dual ? StatusItemLabel.dualFont : StatusItemLabel.chipFont)
                .monospacedDigit()
                .foregroundStyle(severityTint(severity))
                .lineLimit(1)
            // Mini bars only for pure percent tokens — countdown / "—" skip (helper policy).
            if remainingIndex < chip.remainingPerLine.count,
               statusItemLineShowsMiniBar(line)
            {
                miniRemainingBar(
                    remaining: chip.remainingPerLine[remainingIndex],
                    severity: severity,
                    dual: dual
                )
            }
        }
    }

    /// CodexBar-style hairline remaining meter (full = quota left).
    private func miniRemainingBar(
        remaining: UInt8,
        severity: String,
        dual: Bool
    ) -> some View {
        let frac = statusItemRemainingFraction(remainingPercent: remaining)
        let barWidth: CGFloat = dual ? 16 : 20
        let barHeight: CGFloat = dual ? 2.5 : 3
        return GeometryReader { geo in
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(Color.primary.opacity(0.14))
                Capsule()
                    .fill(severityTint(severity).opacity(0.9))
                    .frame(
                        width: remaining == 0
                            ? 0
                            : max(2, geo.size.width * frac)
                    )
            }
        }
        .frame(width: barWidth, height: barHeight)
        .accessibilityHidden(true)
    }

    @ViewBuilder
    private var providerMark: some View {
        ZStack {
            Circle()
                .fill(severityTint(chip.severity).opacity(0.28))
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
}
