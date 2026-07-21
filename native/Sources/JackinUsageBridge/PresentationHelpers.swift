// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import SwiftUI

/// Pure mapping: Rust severity string → tint (no arithmetic, no probes).
public func severityTint(_ severity: String) -> Color {
    switch severity {
    case "danger": return .red
    case "warn": return .orange
    default: return .accentColor
    }
}

/// Pure mapping: Rust status → optional SF Symbol badge name.
public func statusBadgeSymbol(_ status: String) -> String? {
    switch status {
    case "error", "needs_login", "needs_secret", "unavailable":
        return "exclamationmark.triangle"
    case "stale":
        return "clock"
    default:
        return nil
    }
}
