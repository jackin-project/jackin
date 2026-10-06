// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import SwiftUI

/// Pure style mapping.
///
/// Rust owns the semantic severity and every visible string.
public func severityTint(_ severity: String) -> Color {
    switch severity {
    case "danger": return .red
    case "warn": return .orange
    default: return .jackinPhosphor
    }
}

/// Hard product cap for provider status items.
public let statusBarMaxChips = 3

/// Frozen-fixture filter.
///
/// Live membership and order come from Rust.
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

/// AppKit status-item order follows creation order, so rank changes rebuild items.
public func statusBarOrderRequiresRebuild(previous: [String], next: [String]) -> Bool {
    previous != next
}
