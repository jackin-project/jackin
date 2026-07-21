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

/// Bucket presentation shape from Rust fields only (no invented gauges).
public enum BucketRowShape: Equatable, Sendable {
    case gauge
    case valueOnly
    case empty
}

public func bucketRowShape(remainingPercent: UInt8?, usedLabel: String?) -> BucketRowShape {
    if remainingPercent != nil {
        return .gauge
    }
    if let used = usedLabel, !used.isEmpty {
        return .valueOnly
    }
    return .empty
}

/// Pure row body selection from Rust overview fields (layout only).
public enum OverviewGlanceBody: Equatable, Sendable {
    case numeric(headline: String, reset: String?)
    case statusWord(String)
}

public func overviewGlanceBody(headline: String, resetLabel: String?, statusWord: String)
    -> OverviewGlanceBody
{
    if !headline.isEmpty {
        return .numeric(headline: headline, reset: resetLabel)
    }
    return .statusWord(statusWord)
}
