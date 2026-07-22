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

/// One menu-bar chip — **per-provider** usage preview (CodexBar-style).
///
/// Each enabled agent with data gets a chip: glyph + up to two short percent
/// lines (e.g. session/weekly). Strings from Rust remaining / compact labels.
public struct StatusItemChip: Identifiable, Equatable, Sendable {
    public var id: String { surfaceId }
    public let surfaceId: String
    /// 1–2 letter glyph for the strip (from Rust compact label / surface id).
    public let glyph: String
    /// Optional SF Symbol name for known surface ids (layout only).
    public let systemImage: String?
    /// Up to two short percent lines for stacked display (e.g. `100%`, `79%`).
    public let percentLines: [String]
    /// Full Rust compact label for accessibility / fallback (`Cl 37%` remaining).
    public let compactLabel: String
    /// Driving-bucket remaining for primary mini-indicator.
    public let remainingPercent: UInt8?
    /// Per-line remainings for mini bars under stacked percents.
    public let remainingPerLine: [UInt8]
    /// Per-line severity (same length as percentLines when available).
    public let severityPerLine: [String]
    public let severity: String

    public init(
        surfaceId: String,
        glyph: String,
        systemImage: String? = nil,
        percentLines: [String],
        compactLabel: String,
        remainingPercent: UInt8?,
        remainingPerLine: [UInt8] = [],
        severityPerLine: [String] = [],
        severity: String
    ) {
        self.surfaceId = surfaceId
        self.glyph = glyph
        self.systemImage = systemImage
        self.percentLines = percentLines
        self.compactLabel = compactLabel
        self.remainingPercent = remainingPercent
        self.remainingPerLine = remainingPerLine
        self.severityPerLine = severityPerLine
        self.severity = severity
    }
}

/// Frozen host surface ids in catalog order (matches Rust `HostSurfaceId::ALL`).
///
/// OpenUsage/CodexBar-style multi-provider strip must be able to show each of
/// these when enabled — never Cursor/Gemini/Copilot.
public let frozenHostSurfaceIds: [String] = [
    "claude",
    "codex",
    "amp",
    "grok",
    "zai",
    "kimi",
    "minimax",
    "opencode",
]

/// SF Symbol for a known host surface id (status-item / tile layout only).
///
/// Every frozen host surface has a distinct mark so the OpenUsage-style strip
/// always shows a provider icon even when the compact glyph is empty.
public func statusItemSystemImage(surfaceId: String) -> String? {
    switch surfaceId {
    case "claude": return "sparkles"
    case "codex": return "circle.hexagongrid.fill"
    case "amp": return "waveform"
    case "grok": return "circle.dashed"
    case "zai": return "z.square.fill"
    case "kimi": return "k.circle"
    case "minimax": return "waveform.path"
    case "opencode": return "chevron.left.forwardslash.chevron.right"
    default: return nil
    }
}

/// Whether every frozen host surface has a status-item system image (displayable strip).
public func allFrozenHostSurfacesHaveSystemImages() -> Bool {
    frozenHostSurfaceIds.allSatisfy { statusItemSystemImage(surfaceId: $0) != nil }
}

/// Stable two-letter mark when SF Symbol is unavailable (matches Rust compact prefixes).
public func statusItemFallbackGlyph(surfaceId: String) -> String {
    switch surfaceId {
    case "claude": return "Cl"
    case "codex": return "Cx"
    case "amp": return "Am"
    case "grok": return "Gr"
    case "zai": return "ZA"
    case "kimi": return "Ki"
    case "minimax": return "MM"
    case "opencode": return "OC"
    default:
        let idLetters = surfaceId.filter(\.isLetter)
        if idLetters.count >= 2 {
            return String(idLetters.prefix(2)).uppercased()
        }
        return "j"
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

/// Whether Settings percent style is used-% (`used`) vs remaining-% (`left`).
public func statusItemShowsUsedPercent(percentStyle: String) -> Bool {
    percentStyle == "used"
}

/// Display percent integer from Rust remaining + Settings style (`left`/`used`).
public func statusItemDisplayPercent(
    remainingPercent: UInt8,
    percentStyle: String = "left"
) -> UInt8 {
    if statusItemShowsUsedPercent(percentStyle: percentStyle) {
        return UInt8(100 - Int(remainingPercent))
    }
    return remainingPercent
}

/// Short OpenUsage-style percent token from Rust remaining (default remaining %, e.g. `79%`).
public func statusItemPercentToken(
    remainingPercent: UInt8,
    percentStyle: String = "left"
) -> String {
    var s = String(statusItemDisplayPercent(
        remainingPercent: remainingPercent,
        percentStyle: percentStyle
    ))
    s.append("%")
    return s
}

/// OpenUsage primary metric line under a capacity bar (`81% left` / `19% used`).
///
/// Prefers Rust `remaining_percent` + Settings style; falls back to `usedLabel`
/// when no numeric remaining (money / unbounded rows).
public func bucketPrimaryPercentLabel(
    remainingPercent: UInt8?,
    usedLabel: String?,
    percentStyle: String = "left"
) -> String {
    if let remaining = remainingPercent {
        let token = statusItemPercentToken(
            remainingPercent: remaining,
            percentStyle: percentStyle
        )
        if statusItemShowsUsedPercent(percentStyle: percentStyle) {
            return "\(token) used"
        }
        return "\(token) left"
    }
    if let used = usedLabel, !used.isEmpty {
        return used
    }
    return "—"
}

/// Glyph for the menu-bar strip from Rust compact label (`Cl 37%` → `Cl`).
///
/// Falls back to the frozen-host two-letter mark so every provider keeps an identity
/// mark next to remaining % (OpenUsage strip).
public func statusItemGlyph(compactLabel: String, surfaceId: String) -> String {
    // Prefer compact-prefix letters only when the label looks like `Cl 37%` / `Cl resets…`
    // (short letter run), not long status phrases.
    let letters = compactLabel.filter(\.isLetter)
    if letters.count >= 2, letters.count <= 3 {
        return String(letters.prefix(2))
    }
    if letters.count == 1 {
        return String(letters)
    }
    return statusItemFallbackGlyph(surfaceId: surfaceId)
}

/// Up to two short percent lines from numeric remaining values (OpenUsage stack).
///
/// `percentStyle` matches Rust format prefs (`left` remaining / `used` used) so
/// chip lines stay consistent with compact labels and a11y.
public func statusItemPercentLines(
    remainings: [UInt8],
    maxLines: Int = 2,
    percentStyle: String = "left"
) -> [String] {
    Array(
        remainings.prefix(max(0, maxLines)).map {
            statusItemPercentToken(remainingPercent: $0, percentStyle: percentStyle)
        }
    )
}

/// Pure snapshot used to build status-item chips without UniFFI/AppKit.
public struct StatusItemSurfaceSnapshot: Sendable, Equatable {
    public let surfaceId: String
    public let label: String
    public let enabled: Bool
    public let statusBarLabel: String
    public let status: String
    /// Rust `compact_status_bar_label_for` result (may be empty).
    public let compactLabel: String
    /// Numeric bucket remainings in display order (session then weekly…).
    public let remainings: [UInt8]
    /// Severity per remaining entry (same length as remainings when possible).
    public let severities: [String]

    public init(
        surfaceId: String,
        label: String,
        enabled: Bool,
        statusBarLabel: String,
        status: String,
        compactLabel: String,
        remainings: [UInt8],
        severities: [String]
    ) {
        self.surfaceId = surfaceId
        self.label = label
        self.enabled = enabled
        self.statusBarLabel = statusBarLabel
        self.status = status
        self.compactLabel = compactLabel
        self.remainings = remainings
        self.severities = severities
    }

    /// Whether this surface can appear in the menu-bar strip (hide empty).
    public var hasPreviewData: Bool {
        guard enabled else { return false }
        if !remainings.isEmpty { return true }
        return !statusBarLabel.isEmpty
            && status != "disabled"
            && status != "unavailable"
    }

    public var drivingRemaining: UInt8? {
        remainings.min()
    }

    public var drivingSeverity: String {
        guard let minRem = drivingRemaining,
              let idx = remainings.firstIndex(of: minRem),
              idx < severities.count
        else {
            return severities.first ?? "ok"
        }
        return severities[idx]
    }
}

/// Whether Rust compact label is a depleted reset countdown (`Cl resets 1h 21m`).
public func statusItemCompactIsResetCountdown(_ compactLabel: String) -> Bool {
    let lower = compactLabel.lowercased()
    return lower.contains("resets")
}

/// Short reset fragment from Rust compact (`Cl resets 1h 21m` → `resets 1h 21m`).
public func statusItemResetCountdownLine(compactLabel: String) -> String? {
    guard statusItemCompactIsResetCountdown(compactLabel) else { return nil }
    let parts = compactLabel.split(separator: " ", maxSplits: 1, omittingEmptySubsequences: true)
    if parts.count == 2 {
        return String(parts[1])
    }
    return compactLabel
}

/// Chip display lines from Rust remainings + compact (OpenUsage/CodexBar).
///
/// Zero remaining lines use the Rust reset countdown when compact is depleted
/// (`Cl resets …`); **non-zero dual-bucket lines stay as percent tokens**
/// (e.g. remainings `[0, 79]` → `["resets 1h 21m", "79%"]`). Countdown is
/// used at most once so dual-zero stacks do not repeat the same string.
public func statusItemChipDisplayLines(
    remainings: [UInt8],
    compactLabel: String,
    percentStyle: String = "left",
    maxLines: Int = 2
) -> [String] {
    let slice = Array(remainings.prefix(max(0, maxLines)))
    guard !slice.isEmpty else { return [] }
    let countdown = statusItemResetCountdownLine(compactLabel: compactLabel)
    var usedCountdown = false
    var lines: [String] = []
    for rem in slice {
        if rem == 0, let countdown, !usedCountdown {
            lines.append(countdown)
            usedCountdown = true
        } else {
            lines.append(
                statusItemPercentToken(remainingPercent: rem, percentStyle: percentStyle)
            )
        }
    }
    return lines
}

/// Build OpenUsage/CodexBar-style per-provider chips from pure snapshots (unit-testable).
///
/// - Catalog order when `preferWorstFirst` is false.
/// - Lowest remaining first when true (focus mode).
/// - `includeAllEnabled`: strip mode shows **every enabled** host surface (icon always),
///   with remaining % when Rust supplies it; empty data shows `—` (never invents %).
/// - Otherwise hides surfaces without preview data.
/// - `percentStyle` (`left`/`used`) shapes stacked percent lines to match
///   Rust compact labels (OpenUsage remaining default).
/// - Depleted + reset countdown prefers Rust compact over bare `0%`.
public func buildStatusItemChips(
    surfaces: [StatusItemSurfaceSnapshot],
    maxCount: Int,
    preferWorstFirst: Bool,
    percentStyle: String = "left",
    includeAllEnabled: Bool = false
) -> [StatusItemChip] {
    let cap = max(1, min(8, maxCount))
    var candidates = surfaces.filter { surface in
        guard surface.enabled else { return false }
        if includeAllEnabled { return true }
        return surface.hasPreviewData
    }
    if preferWorstFirst {
        candidates.sort { lhs, rhs in
            let l = lhs.drivingRemaining ?? 100
            let r = rhs.drivingRemaining ?? 100
            if l != r { return l < r }
            return false
        }
    }
    var chips: [StatusItemChip] = []
    for surface in candidates.prefix(cap) {
        let compact =
            !surface.compactLabel.isEmpty
            ? surface.compactLabel
            : (!surface.statusBarLabel.isEmpty ? surface.statusBarLabel : surface.label)
        let remainings = Array(surface.remainings.prefix(2))
        let lineSeverities = Array(surface.severities.prefix(remainings.count))
        let lines: [String]
        if remainings.isEmpty {
            // Honest placeholder — never invent a percent.
            lines = includeAllEnabled ? ["—"] : []
        } else {
            lines = statusItemChipDisplayLines(
                remainings: remainings,
                compactLabel: compact,
                percentStyle: percentStyle,
                maxLines: 2
            )
        }
        let displayCompact =
            remainings.isEmpty && includeAllEnabled
            ? "\(statusItemFallbackGlyph(surfaceId: surface.surfaceId)) —"
            : compact
        chips.append(
            StatusItemChip(
                surfaceId: surface.surfaceId,
                glyph: statusItemGlyph(compactLabel: compact, surfaceId: surface.surfaceId),
                systemImage: statusItemSystemImage(surfaceId: surface.surfaceId),
                percentLines: lines,
                compactLabel: displayCompact.isEmpty ? surface.label : displayCompact,
                remainingPercent: surface.drivingRemaining,
                remainingPerLine: remainings,
                severityPerLine: lineSeverities.isEmpty ? ["ok"] : lineSeverities,
                severity: remainings.isEmpty ? "ok" : surface.drivingSeverity
            )
        )
    }
    return chips
}

/// Accessibility / VoiceOver string for a multi-provider strip.
///
/// Prefer stacked percent lines when present so dual-bucket chips speak both
/// session and weekly values (OpenUsage/CodexBar parity), not only the driving compact.
/// Depleted reset countdowns use the full Rust compact label.
public func statusItemAccessibilityLabel(chips: [StatusItemChip]) -> String {
    if chips.isEmpty { return "jackin Desktop" }
    let parts = chips.map { chip -> String in
        if statusItemCompactIsResetCountdown(chip.compactLabel) {
            // Dual-bucket: compact is driving countdown; keep secondary percent lines.
            if chip.percentLines.count > 1 {
                let rest = chip.percentLines.dropFirst().joined(separator: " and ")
                return "\(chip.compactLabel) and \(rest)"
            }
            return chip.compactLabel
        }
        if chip.percentLines.count >= 2 {
            return "\(chip.glyph) \(chip.percentLines.joined(separator: " and "))"
        }
        if let line = chip.percentLines.first, !line.isEmpty {
            return "\(chip.glyph) \(line)"
        }
        return chip.compactLabel
    }
    return "jackin Desktop \(parts.joined(separator: ", "))"
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
