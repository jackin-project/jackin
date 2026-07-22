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

/// One menu-bar chip (OpenUsage-style strip segment).
///
/// Strings are Rust-owned compact labels / remaining tokens. Swift only lays out
/// glyph + stacked short percents (OpenUsage menu-bar density).
public struct StatusItemChip: Identifiable, Equatable, Sendable {
    public var id: String { surfaceId }
    public let surfaceId: String
    /// 1–2 letter glyph for the strip (from Rust compact label / surface id).
    public let glyph: String
    /// Up to two short percent lines for stacked display (e.g. `100%`, `79%`).
    public let percentLines: [String]
    /// Full Rust compact label for accessibility / fallback (`Cl 63%`).
    public let compactLabel: String
    /// Driving-bucket remaining for primary mini-indicator.
    public let remainingPercent: UInt8?
    public let severity: String

    public init(
        surfaceId: String,
        glyph: String,
        percentLines: [String],
        compactLabel: String,
        remainingPercent: UInt8?,
        severity: String
    ) {
        self.surfaceId = surfaceId
        self.glyph = glyph
        self.percentLines = percentLines
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

/// Short OpenUsage-style percent token from Rust remaining (e.g. `79%`).
public func statusItemPercentToken(remainingPercent: UInt8) -> String {
    var s = String(remainingPercent)
    s.append("%")
    return s
}

/// Glyph for the menu-bar strip from Rust compact label (`Cl 63%` → `Cl`).
public func statusItemGlyph(compactLabel: String, surfaceId: String) -> String {
    let letters = compactLabel.filter(\.isLetter)
    if letters.count >= 2 {
        return String(letters.prefix(2))
    }
    if letters.count == 1 {
        return String(letters)
    }
    let idLetters = surfaceId.filter(\.isLetter)
    if idLetters.count >= 2 {
        return String(idLetters.prefix(2)).uppercased()
    }
    return "j"
}

/// Up to two short percent lines from numeric remaining values (OpenUsage stack).
public func statusItemPercentLines(remainings: [UInt8], maxLines: Int = 2) -> [String] {
    Array(remainings.prefix(max(0, maxLines)).map(statusItemPercentToken(remainingPercent:)))
}

/// Format Rust `MoneyDto` for display (no `String(format:)`).
public func formatMoneyDto(_ money: MoneyDto) -> String {
    let exp = Int(money.exponent)
    let divisor = Int64(pow(10.0, Double(max(0, exp))))
    let major = money.amountMinor / max(1, divisor)
    let minor = abs(money.amountMinor % max(1, divisor))
    let currency = money.currency.uppercased()
    if exp <= 0 {
        return currency == "USD" ? "$\(major)" : "\(major) \(currency)"
    }
    var frac = String(minor)
    while frac.count < exp {
        frac = "0" + frac
    }
    if currency == "USD" {
        return "$\(major).\(frac)"
    }
    return "\(major).\(frac) \(currency)"
}
