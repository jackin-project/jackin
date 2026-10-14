// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import SwiftUI

public func severityTint(_ severity: String) -> Color {
    switch severity {
    case "danger": return .red
    case "warn": return .orange
    default: return .jackinPhosphor
    }
}

public let statusBarMaxChips = 3

public func selectStatusBarGlanceRows(
    from rows: [PresentationStore.GlanceProviderRow],
    maxCount: Int = statusBarMaxChips
) -> [PresentationStore.GlanceProviderRow] {
    let cap = min(statusBarMaxChips, max(1, maxCount))
    return Array(
        rows
            .filter { ($0.glanceRemainingPercent ?? 0) > 0 }
            .prefix(cap)
    )
}

public func statusBarOrderRequiresRebuild(previous: [String], next: [String]) -> Bool {
    previous != next
}
