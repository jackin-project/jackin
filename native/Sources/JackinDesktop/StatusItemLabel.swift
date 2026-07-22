// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit
import JackinUsageBridge
import SwiftUI

/// Menu-bar status item — OpenUsage-inspired strip of mini capacity bars + Rust compact labels.
///
/// Clean-room look-and-feel only (template mark, monospaced chips, thin bars, hide empty).
/// Every displayed string and remaining percent comes from Rust; Swift only lays out.
struct StatusItemLabel: View {
    @ObservedObject var store: PresentationStore

    var body: some View {
        HStack(spacing: 6) {
            statusIcon
                .opacity(itemOpacity)

            if !store.statusItemChips.isEmpty {
                // OpenUsage-style multi-metric strip (one chip when focus/pinned).
                HStack(spacing: 7) {
                    ForEach(store.statusItemChips) { chip in
                        StatusItemChipView(chip: chip)
                            .opacity(itemOpacity)
                    }
                }
            } else if !store.statusItemText.isEmpty {
                // Fallback when chips empty but Rust still has a text label.
                Text(store.statusItemText)
                    .font(Self.chipFont)
                    .monospacedDigit()
                    .opacity(itemOpacity)
            }
        }
        // WHY: MenuBarExtra collapses zero-size labels; pin a minimum hit target.
        .frame(minWidth: 16, minHeight: 18)
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
        if !store.statusItemText.isEmpty {
            return "jackin Desktop \(store.statusItemText)"
        }
        return "jackin Desktop"
    }

    static let chipFont = Font.system(size: 11, weight: .semibold, design: .rounded)

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

/// One OpenUsage-like chip: thin capacity bar + Rust compact label (`Cl 63%`).
private struct StatusItemChipView: View {
    let chip: StatusItemChip

    var body: some View {
        HStack(spacing: 4) {
            if let remaining = chip.remainingPercent {
                miniBar(remaining: remaining, severity: chip.severity)
            }
            Text(chip.compactLabel)
                .font(StatusItemLabel.chipFont)
                .monospacedDigit()
                .lineLimit(1)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(chip.compactLabel)
    }

    /// Thin horizontal used/remaining bar (OpenUsage “Bars” density, severity tint).
    private func miniBar(remaining: UInt8, severity: String) -> some View {
        let used = statusItemUsedFraction(remainingPercent: remaining)
        return GeometryReader { geo in
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(Color.primary.opacity(0.18))
                Capsule()
                    .fill(severityTint(severity))
                    .frame(width: max(2, geo.size.width * used))
            }
        }
        .frame(width: 16, height: 4)
        .accessibilityHidden(true)
    }
}
