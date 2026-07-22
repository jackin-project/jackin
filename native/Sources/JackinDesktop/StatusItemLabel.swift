// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit
import JackinUsageBridge
import SwiftUI

/// Template status-item content: jackin❯ logomark (or SF Symbol fallback) + optional Rust-owned text.
///
/// Always draws a non-empty label. An empty `HStack` / failed image load makes
/// `MenuBarExtra` invisible on the menu bar with no error.
struct StatusItemLabel: View {
    @ObservedObject var store: PresentationStore

    var body: some View {
        HStack(spacing: 4) {
            statusIcon
                .opacity(store.allEnabledSurfacesDegraded ? 0.45 : 1.0)
            if !store.statusItemText.isEmpty {
                Text(store.statusItemText)
                    .font(.system(size: 12, weight: .medium, design: .default))
                    .monospacedDigit()
                    .opacity(store.allEnabledSurfacesDegraded ? 0.45 : 1.0)
            }
        }
        // WHY: MenuBarExtra collapses zero-size labels; pin a minimum hit target.
        .frame(minWidth: 16, minHeight: 16)
        .accessibilityLabel(accessibilityText)
        // WHY: status item must open HostUsageRuntime on cold launch/login without
        // requiring popover/Settings/Usage first — otherwise focus-percent stays empty.
        .onAppear {
            if !store.isOpen {
                store.openDefault()
            }
        }
    }

    @ViewBuilder
    private var statusIcon: some View {
        if let mark = Self.loadLogomark() {
            Image(nsImage: mark)
                .renderingMode(.template)
                .resizable()
                .interpolation(.high)
                .frame(width: 16, height: 16)
        } else {
            // Always-visible fallback when the PDF resource is missing or unloadable.
            Image(systemName: "gauge.with.needle")
                .symbolRenderingMode(.monochrome)
                .imageScale(.medium)
                .frame(width: 16, height: 16)
        }
    }

    private var accessibilityText: String {
        if !store.statusItemText.isEmpty {
            return "jackin Desktop \(store.statusItemText)"
        }
        return "jackin Desktop"
    }

    /// Load the template logomark from the SwiftPM resource bundle.
    /// Prefer URL → NSImage: `Bundle.image(forResource:)` is flaky for PDF.
    private static func loadLogomark() -> NSImage? {
        let bundle = Bundle.module
        let url =
            bundle.url(forResource: "JackinMark", withExtension: "pdf")
            ?? bundle.url(forResource: "JackinMark", withExtension: "PDF")
        guard let url else { return nil }
        guard let image = NSImage(contentsOf: url) else { return nil }
        image.isTemplate = true
        image.size = NSSize(width: 16, height: 16)
        return image
    }
}
