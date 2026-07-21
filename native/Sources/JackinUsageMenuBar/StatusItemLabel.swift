// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import JackinUsageBridge
import SwiftUI

/// Template status-item content: SF Symbol gauge + optional Rust compact percent.
struct StatusItemLabel: View {
    @ObservedObject var store: PresentationStore

    var body: some View {
        HStack(spacing: 4) {
            Image(systemName: "gauge.with.needle")
                .symbolRenderingMode(.monochrome)
                .imageScale(.medium)
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

    private var accessibilityText: String {
        if store.showPercentInMenuBar, !store.compactBarLabel.isEmpty {
            return "jackin usage \(store.compactBarLabel)"
        }
        return "jackin usage"
    }
}
