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

/// One menu-bar chip (OpenUsage-style strip segment). Strings come from Rust.
public struct StatusItemChip: Identifiable, Equatable, Sendable {
    public var id: String { surfaceId }
    public let surfaceId: String
    /// Rust compact label, e.g. `Cl 63%` or depleted form.
    public let compactLabel: String
    /// Driving-bucket remaining from Rust (nil → text-only chip, no mini bar).
    public let remainingPercent: UInt8?
    public let severity: String

    public init(
        surfaceId: String,
        compactLabel: String,
        remainingPercent: UInt8?,
        severity: String
    ) {
        self.surfaceId = surfaceId
        self.compactLabel = compactLabel
        self.remainingPercent = remainingPercent
        self.severity = severity
    }
}

/// Pick the driving bucket for status-item chips: lowest remaining among numeric buckets.
public func drivingBucketForStatusItem(
    remainingAndSeverity: [(remaining: UInt8, severity: String)]
) -> (remaining: UInt8, severity: String)? {
    remainingAndSeverity.min(by: { $0.remaining < $1.remaining })
}

/// Used-fraction for mini capacity bars from Rust remaining (display only).
public func statusItemUsedFraction(remainingPercent: UInt8) -> Double {
    Double(100 - Int(remainingPercent)) / 100.0
}
