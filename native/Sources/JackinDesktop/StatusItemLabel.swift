// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit
import JackinUsageBridge
import SwiftUI

/// Template status-item content: jackin❯ logomark + optional Rust compact percent.
struct StatusItemLabel: View {
    @ObservedObject var store: PresentationStore

    var body: some View {
        HStack(spacing: 4) {
            logomark
                .opacity(store.allEnabledSurfacesDegraded ? 0.45 : 1.0)
            if store.showPercentInMenuBar, !store.compactBarLabel.isEmpty {
                Text(store.compactBarLabel)
                    .font(.system(size: 12, weight: .medium, design: .default))
                    .monospacedDigit()
                    .opacity(store.allEnabledSurfacesDegraded ? 0.45 : 1.0)
            }
        }
        .accessibilityLabel(accessibilityText)
    }

    @ViewBuilder
    private var logomark: some View {
        if let mark = Bundle.module.image(forResource: "JackinMark") {
            Image(nsImage: {
                mark.isTemplate = true
                mark.size = NSSize(width: 16, height: 16)
                return mark
            }())
            .renderingMode(.template)
            .frame(width: 16, height: 16)
        } else {
            // Resource missing only if assembly forgot the SwiftPM resource bundle.
            Image(systemName: "gauge.with.needle")
                .symbolRenderingMode(.monochrome)
                .imageScale(.medium)
        }
    }

    private var accessibilityText: String {
        if store.showPercentInMenuBar, !store.compactBarLabel.isEmpty {
            return "jackin Desktop \(store.compactBarLabel)"
        }
        return "jackin Desktop"
    }
}
