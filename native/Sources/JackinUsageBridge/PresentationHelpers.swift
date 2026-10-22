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
    from rows: [GlanceProviderRow],
    maxCount: Int = statusBarMaxChips
) -> [GlanceProviderRow] {
    let cap = min(statusBarMaxChips, max(1, maxCount))
    return Array(
        rows
            .filter { row in
                switch (row.glanceMeterPercent, row.glanceMeterSide) {
                case (.some(let percent), .some("remaining")):
                    percent > 0
                case (.some(let percent), .some("used")):
                    percent < 100
                default:
                    false
                }
            }
            .prefix(cap)
    )
}

public func statusBarOrderRequiresRebuild(previous: [String], next: [String]) -> Bool {
    previous != next
}
