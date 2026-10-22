// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import Foundation
import JackinUsageBindings

public struct IdentityRow: Sendable, Equatable {
    public let providerTitle: String
    public let accountLabel: String
    public let activityLabel: String
    public let activityKind: String
    public let accessibilityLabel: String

    public init(dto: UsageIdentityPresentationDto) {
        providerTitle = dto.providerTitle
        accountLabel = dto.accountLabel
        activityLabel = dto.activityLabel
        activityKind = dto.activityKind
        accessibilityLabel = dto.accessibilityLabel
    }
}
public struct SurfaceRow: Identifiable, Sendable, Equatable {
    public let id: String
    public let label: String
    public var enabled: Bool
    public var statusBarLabel: String
    public var status: String
    public var accountLabel: String
    public var username: String?
    public var planLabel: String?
    public var credentialOrigin: String?
    public var estimateCaption: String?
    public var buckets: [BucketRow]
    public var updatedLabel: String
    public var lastError: String?
    public var identity: IdentityRow?
    public var detailPresentation: UsageDetailPresentation

    public init(
        id: String,
        label: String,
        enabled: Bool,
        statusBarLabel: String,
        status: String,
        accountLabel: String,
        username: String?,
        planLabel: String?,
        credentialOrigin: String?,
        estimateCaption: String?,
        buckets: [BucketRow],
        updatedLabel: String,
        lastError: String?,
        detailPresentation: UsageDetailPresentation,
        identity: IdentityRow? = nil
    ) {
        self.id = id
        self.label = label
        self.enabled = enabled
        self.statusBarLabel = statusBarLabel
        self.status = status
        self.accountLabel = accountLabel
        self.username = username
        self.planLabel = planLabel
        self.credentialOrigin = credentialOrigin
        self.estimateCaption = estimateCaption
        self.buckets = buckets
        self.updatedLabel = updatedLabel
        self.lastError = lastError
        self.detailPresentation = detailPresentation
        self.identity = identity
    }
}

public struct CountQuotaModel: Sendable, Equatable {
    public let used: UInt64?
    public let limit: UInt64?
    public let remaining: UInt64?
    public let unit: String
    public let period: String
    public let provenance: String

    public init(dto: CountQuotaDto) {
        used = dto.used
        limit = dto.limit
        remaining = dto.remaining
        unit = dto.unit
        period = dto.period
        provenance = dto.provenance
    }
}

public struct BucketRow: Identifiable, Sendable, Equatable {
    public var id: String { label }
    public let label: String
    public let usedLabel: String?
    public let limitLabel: String?
    public let usedPercent: Double?
    public let usedPercentOrigin: String?
    public let remainingPercent: Double?
    public let remainingPercentOrigin: String?
    public let countQuota: CountQuotaModel?
    public let providerQuotaState: String?
    public let resetsAt: Int64?
    public let resetLabel: String?
    public let paceLabel: String?
    public let statusSlot: String?
    public let severity: String
    public let status: String
    public let usedMoney: MoneyDto?
    public let limitMoney: MoneyDto?
    public let remainingLabel: String?
    public let displaySegments: [String]
    public let displayLabel: String
    public let meterPercent: Double?
    public let meterSide: String?
}

public struct GlanceProviderRow: Identifiable, Sendable, Equatable {
    public var id: String { surfaceId }
    public let surfaceId: String
    public let iconKey: String
    public let fallbackGlyph: String
    public let usageURL: String?
    public let displayLabel: String
    public let accountLabel: String
    public let planLabel: String?
    public let glanceUsedPercent: Double?
    public let glanceUsedPercentOrigin: String?
    public let glanceRemainingPercent: Double?
    public let glanceRemainingPercentOrigin: String?
    public let glanceCountQuota: CountQuotaModel?
    public let glanceProviderQuotaState: String?
    public let glanceResetsAt: Int64?
    public let glanceMeterPercent: Double?
    public let glanceMeterSide: String?
    public let barLabel: String
    public let headline: String
    public let resetLabel: String?
    public let compactResetLabel: String?
    public let exactReset: String?
    public let statusWord: String
    public let isRefreshing: Bool
    public let statusLabel: String
    public let severity: String
    public let updatedLabel: String
    public let activityLabel: String
    public let activityKind: String
    public let accessibilityLabel: String
    public let lastError: String?
    public let dimmed: Bool

    public init(
        surfaceId: String,
        iconKey: String,
        fallbackGlyph: String,
        usageURL: String?,
        displayLabel: String,
        accountLabel: String,
        planLabel: String?,
        glanceRemainingPercent: Double?,
        barLabel: String,
        headline: String,
        resetLabel: String?,
        compactResetLabel: String?,
        exactReset: String?,
        statusWord: String,
        isRefreshing: Bool,
        statusLabel: String,
        severity: String,
        updatedLabel: String,
        activityLabel: String,
        activityKind: String,
        accessibilityLabel: String,
        lastError: String?,
        dimmed: Bool,
        glanceUsedPercent: Double? = nil,
        glanceUsedPercentOrigin: String? = nil,
        glanceRemainingPercentOrigin: String? = nil,
        glanceCountQuota: CountQuotaModel? = nil,
        glanceProviderQuotaState: String? = nil,
        glanceResetsAt: Int64? = nil,
        glanceMeterPercent: Double? = nil,
        glanceMeterSide: String? = nil
    ) {
        self.surfaceId = surfaceId
        self.iconKey = iconKey
        self.fallbackGlyph = fallbackGlyph
        self.usageURL = usageURL
        self.displayLabel = displayLabel
        self.accountLabel = accountLabel
        self.planLabel = planLabel
        self.glanceUsedPercent = glanceUsedPercent
        self.glanceUsedPercentOrigin = glanceUsedPercentOrigin
        self.glanceRemainingPercent = glanceRemainingPercent
        self.glanceRemainingPercentOrigin = glanceRemainingPercentOrigin
        self.glanceCountQuota = glanceCountQuota
        self.glanceProviderQuotaState = glanceProviderQuotaState
        self.glanceResetsAt = glanceResetsAt
        self.glanceMeterPercent = glanceMeterPercent
        self.glanceMeterSide = glanceMeterSide
        self.barLabel = barLabel
        self.headline = headline
        self.resetLabel = resetLabel
        self.compactResetLabel = compactResetLabel
        self.exactReset = exactReset
        self.statusWord = statusWord
        self.isRefreshing = isRefreshing
        self.statusLabel = statusLabel
        self.severity = severity
        self.updatedLabel = updatedLabel
        self.activityLabel = activityLabel
        self.activityKind = activityKind
        self.accessibilityLabel = accessibilityLabel
        self.lastError = lastError
        self.dimmed = dimmed
    }
}

public enum SelectedAccountRoute: Sendable, Equatable {
    case unselected
    case resolving(accountKey: String)
    case available(accountKey: String)
    case unavailable(accountKey: String, notice: String)

    public init?(dto: SelectedAccountRouteDto) {
        let accountKey = dto.accountKey
        let notice = dto.notice
        switch dto.status {
        case "unselected" where accountKey == nil && notice == nil:
            self = .unselected
        case "resolving" where notice == nil:
            guard let accountKey, Self.isValidRouteKey(accountKey) else { return nil }
            self = .resolving(accountKey: accountKey)
        case "available" where notice == nil:
            guard let accountKey, Self.isValidRouteKey(accountKey) else { return nil }
            self = .available(accountKey: accountKey)
        case "unavailable":
            guard let accountKey, Self.isValidRouteKey(accountKey),
                let notice, !notice.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            else { return nil }
            self = .unavailable(accountKey: accountKey, notice: notice)
        default:
            return nil
        }
    }

    private static func isValidRouteKey(_ accountKey: String) -> Bool {
        !accountKey.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && accountKey == accountKey.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}

public struct ProviderGroupRow: Identifiable, Sendable, Equatable {
    public var id: String { surfaceId }
    public let surfaceId: String
    public let displayLabel: String
    public let iconKey: String
    public let fallbackGlyph: String
    public let usageURL: String?
    public let accountColumnLabel: String
    public let planOrStatusLabel: String
    public let remainingLabel: String
    public let resetDisplayLabel: String
    public let accounts: [AccountRow]
    public let accessibilityLabel: String
    public let lastError: String?
    public let selectedAccountRoute: SelectedAccountRoute?

    public init(
        surfaceId: String,
        displayLabel: String,
        iconKey: String,
        fallbackGlyph: String,
        usageURL: String?,
        accountColumnLabel: String,
        planOrStatusLabel: String,
        remainingLabel: String,
        resetDisplayLabel: String,
        accounts: [AccountRow],
        accessibilityLabel: String,
        lastError: String?,
        selectedAccountRoute: SelectedAccountRoute? = .unselected
    ) {
        self.surfaceId = surfaceId
        self.displayLabel = displayLabel
        self.iconKey = iconKey
        self.fallbackGlyph = fallbackGlyph
        self.usageURL = usageURL
        self.accountColumnLabel = accountColumnLabel
        self.planOrStatusLabel = planOrStatusLabel
        self.remainingLabel = remainingLabel
        self.resetDisplayLabel = resetDisplayLabel
        self.accounts = accounts
        self.accessibilityLabel = accessibilityLabel
        self.lastError = lastError
        self.selectedAccountRoute = selectedAccountRoute
    }
}

public struct DiscoveryDiagnostic: Identifiable, Sendable, Equatable {
    public var id: String { "\(surfaceId ?? "global")#\(scopeLabel)#\(issue)" }
    public let surfaceId: String?
    public let scopeLabel: String
    public let issue: String
    public let message: String
    public let displayLabel: String
}

public struct AccountRow: Identifiable, Sendable, Equatable {
    public var id: String { "\(surfaceId)#\(accountKey)" }
    public let surfaceId: String
    public let providerColumnLabel: String
    public let accountKey: String
    public let accountLabel: String
    public let planLabel: String?
    public let selected: Bool
    public let lifecycle: String
    public let lifecycleLabel: String
    public let provenanceLabel: String
    public let planOrStatusLabel: String
    public let usedPercent: Double?
    public let usedPercentOrigin: String?
    public let remainingPercent: Double?
    public let remainingPercentOrigin: String?
    public let countQuota: CountQuotaModel?
    public let providerQuotaState: String?
    public let resetsAt: Int64?
    public let meterPercent: Double?
    public let meterSide: String?
    public let remainingLabel: String
    public let headline: String
    public let resetDisplayLabel: String
    public let statusWord: String
    public let statusLabel: String
    public let severity: String
    public let updatedLabel: String
    public let lastError: String?
    public let dimmed: Bool
    public let accessibilityLabel: String

    public init(
        surfaceId: String,
        providerColumnLabel: String,
        accountKey: String,
        accountLabel: String,
        planLabel: String?,
        selected: Bool,
        lifecycle: String,
        lifecycleLabel: String,
        provenanceLabel: String,
        planOrStatusLabel: String,
        usedPercent: Double? = nil,
        usedPercentOrigin: String? = nil,
        remainingPercent: Double?,
        remainingPercentOrigin: String? = nil,
        countQuota: CountQuotaModel? = nil,
        providerQuotaState: String? = nil,
        resetsAt: Int64? = nil,
        meterPercent: Double? = nil,
        meterSide: String? = nil,
        remainingLabel: String,
        headline: String,
        resetDisplayLabel: String,
        statusWord: String,
        statusLabel: String,
        severity: String,
        updatedLabel: String,
        lastError: String?,
        dimmed: Bool,
        accessibilityLabel: String
    ) {
        self.surfaceId = surfaceId
        self.providerColumnLabel = providerColumnLabel
        self.accountKey = accountKey
        self.accountLabel = accountLabel
        self.planLabel = planLabel
        self.selected = selected
        self.lifecycle = lifecycle
        self.lifecycleLabel = lifecycleLabel
        self.provenanceLabel = provenanceLabel
        self.planOrStatusLabel = planOrStatusLabel
        self.usedPercent = usedPercent
        self.usedPercentOrigin = usedPercentOrigin
        self.remainingPercent = remainingPercent
        self.remainingPercentOrigin = remainingPercentOrigin
        self.countQuota = countQuota
        self.providerQuotaState = providerQuotaState
        self.resetsAt = resetsAt
        self.meterPercent = meterPercent
        self.meterSide = meterSide
        self.remainingLabel = remainingLabel
        self.headline = headline
        self.resetDisplayLabel = resetDisplayLabel
        self.statusWord = statusWord
        self.statusLabel = statusLabel
        self.severity = severity
        self.updatedLabel = updatedLabel
        self.lastError = lastError
        self.dimmed = dimmed
        self.accessibilityLabel = accessibilityLabel
    }
}

public struct QIFixtureProjection: Sendable, Equatable {
    public let glanceRows: [GlanceProviderRow]
    public let statusBarGlanceRows: [GlanceProviderRow]
    public let surfaces: [SurfaceRow]
    public let accounts: [AccountRow]
    public let providerGroups: [ProviderGroupRow]

    public init(
        glanceRows: [GlanceProviderRow],
        statusBarGlanceRows: [GlanceProviderRow],
        surfaces: [SurfaceRow],
        accounts: [AccountRow],
        providerGroups: [ProviderGroupRow]
    ) {
        self.glanceRows = glanceRows
        self.statusBarGlanceRows = statusBarGlanceRows
        self.surfaces = surfaces
        self.accounts = accounts
        self.providerGroups = providerGroups
    }
}
