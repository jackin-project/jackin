import SwiftUI

// Canonical fixture records from native/Design/UnifiedAgentUsage/Fixtures.md
// (revision recorded in SIGNOFF.md). Strings here stand in for immutable
// Rust-owned display input; the prototype changes layout only.

enum ProtoState: String, Sendable {
    case current, warning, danger, depleted, stale, refreshing, unavailable
    case needsLogin, needsSecret, unsupported, rateLimited

    var label: String? {
        switch self {
        case .current: nil
        case .warning: "Low"
        case .danger: "Very low"
        case .depleted: "Depleted"
        case .stale: "Stale"
        case .refreshing: "Updating…"
        case .unavailable: "Unavailable"
        case .needsLogin: "Sign in required"
        case .needsSecret: "API key required"
        case .unsupported: "Not supported"
        case .rateLimited: "Rate limited"
        }
    }

    var symbol: String {
        switch self {
        case .current: "checkmark.circle"
        case .warning, .danger: "exclamationmark.triangle.fill"
        case .depleted: "exclamationmark.circle.fill"
        case .stale: "clock.arrow.circlepath"
        case .refreshing: "arrow.triangle.2.circlepath"
        case .unavailable: "exclamationmark.icloud.fill"
        case .needsLogin: "person.crop.circle.badge.exclamationmark"
        case .needsSecret: "key.fill"
        case .unsupported: "minus.circle"
        case .rateLimited: "clock.badge.exclamationmark"
        }
    }

    var exposesQuotaSummary: Bool {
        switch self {
        case .unavailable, .needsLogin, .needsSecret, .unsupported:
            false
        default:
            true
        }
    }
}

struct ProtoQuotaWindow: Identifiable, Sendable {
    let stableID: String
    let label: String
    var category: ProtoQuotaCategory = .other
    var periodTag = ""
    let display: String
    let primaryValue: String
    var secondaryValue: String? = nil
    var resetLabel: String? = nil
    var supplementalValue: String? = nil
    let meter: Int?
    let state: ProtoState
    /// Rust-owned pace phrase (QuotaBucketDto.pace_label): even-burn delta or
    /// exhaustion projection, limits-only — never cost data.
    var pace: String? = nil
    /// Untouched window ("Not started" in OpenUsage terms): full quota, zero
    /// consumption — distinct from merely healthy.
    var notStarted = false
    var id: String { stableID }

    /// The status bar surfaces long-range windows only — the quota that
    /// expires wholesale, so the user can spend it before it lapses.
    /// Hour-range windows (five-hour, session) stay in window surfaces.
    var isLongRange: Bool { category == .longRange }

    var accessibilitySummary: String {
        var parts = [label, primaryValue]
        parts.append(
            contentsOf: [secondaryValue, resetLabel, supplementalValue, pace].compactMap {
                $0
            })
        if notStarted { parts.append("Not started") }
        if let stateLabel = state.label { parts.append(stateLabel) }
        return parts.joined(separator: ", ")
    }
}

/// Semantic order supplied by the fixture projection. Production receives the
/// equivalent order from Rust and never recovers it from display text.
enum ProtoQuotaCategory: Int, Sendable {
    case longRange
    case model
    case general
    case session
    case other
}

enum ProtoQuotaOrdering {
    static func ordered(_ windows: [ProtoQuotaWindow]) -> [ProtoQuotaWindow] {
        windows.enumerated().sorted { left, right in
            if left.element.category != right.element.category {
                return left.element.category.rawValue < right.element.category.rawValue
            }
            return left.offset < right.offset
        }.map(\.element)
    }
}

struct ProtoAccount: Identifiable, Sendable {
    let key: String
    let label: String
    let plan: String
    let remaining: Int?
    let resetText: String?
    let state: ProtoState
    let windows: [ProtoQuotaWindow]
    var username: String? = nil
    var auth: String? = nil
    var id: String { key }
}

struct ProtoProvider: Identifiable, Sendable {
    let key: String
    let name: String
    let state: ProtoState
    let summaryPercent: Int?
    let summaryReset: String?
    let accounts: [ProtoAccount]
    let selectedAccountKey: String?
    let updatedAgo: String?
    let activityText: String?
    let errorText: String?
    var id: String { key }

    var summaryRemainingLeft: String? { summaryPercent.map { "\($0)% left" } }
    var summaryRemainingUsed: String? { summaryPercent.map { "\(100 - $0)% used" } }

    /// Icon key matches the bundled official provider mark names.
    var iconKey: String { key }
    var fallbackGlyph: String { String(name.prefix(1)) }
    var isRefreshing: Bool { state == .refreshing }

    /// One Rust-owned activity phrase for identity rows.
    var activityLabel: String {
        if let activityText { return activityText }
        switch state {
        case .stale:
            return (["Stale", updatedAgo] + [summaryRemainingLeft, summaryReset])
                .compactMap { $0 }
                .joined(separator: " · ")
        case .unavailable, .needsLogin, .needsSecret, .unsupported:
            return state.label ?? "Unavailable"
        default:
            return [summaryRemainingLeft, summaryReset].compactMap { $0 }
                .joined(separator: " · ")
        }
    }

    /// Compact reset countdown for the dual-stack status-item title.
    var compactResetLabel: String? {
        guard let summaryReset else { return nil }
        guard summaryReset.hasPrefix("Resets in ") else { return nil }
        return String(summaryReset.dropFirst("Resets in ".count))
    }

    /// Window driving the status summary: the long-range window whose meter
    /// matches the summary percent, else the first long-range window.
    /// Hour-range windows never drive the status bar.
    var summaryWindow: ProtoQuotaWindow? {
        let longRange = accounts.flatMap(\.windows).filter(\.isLongRange)
        if let percent = summaryPercent,
            let match = longRange.first(where: { $0.meter == percent })
        {
            return match
        }
        return longRange.first
    }
}

struct ProtoChrome: Sendable {
    var refreshTitle = "Refresh"
    var openUsageTitle = "Open Usage"
    var retryTitle = "Retry"
    var locale = Locale(identifier: "en_US")
    var layoutDirection: LayoutDirection = .leftToRight
}

enum ProtoMutationScript: Sendable {
    case standard
    case acceptPercentStyle
    case rejectLowFloor
    case reorderedFloor
}

struct ProtoProjection: Sendable {
    let scenario: String
    let providers: [ProtoProvider]
    let statusRows: [String]
    let isLoading: Bool
    let globalError: String?
    let chrome: ProtoChrome
    let mutationScript: ProtoMutationScript
    let selectedProviderKey: String?
    let selectedAccountKey: String?
}

enum ProtoSymbols {
    static func provider(_ key: String) -> String {
        switch key {
        case "codex": "chevron.left.forwardslash.chevron.right"
        case "claude": "sparkle"
        case "amp": "bolt.fill"
        case "grok": "x.circle"
        case "zai": "z.circle"
        case "kimi": "k.circle"
        case "minimax": "m.circle"
        default: "circle"
        }
    }
}
