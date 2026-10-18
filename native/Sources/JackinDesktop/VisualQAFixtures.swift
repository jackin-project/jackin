// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import Foundation
import JackinUsageBindings
import JackinUsageBridge

public enum VisualQAFixtureID: String, CaseIterable, Sendable {
    case noProviders = "F00-no-providers"
    case singleNormal = "F01-single-normal"
    case catalogNormal = "F02-catalog-normal"
    case multiAccount = "F03-multi-account"
    case nearlyExhausted = "F04-nearly-exhausted"
    case exhausted = "F05-exhausted"
    case staleLastGood = "F06-stale-last-good"
    case refreshingLastGood = "F07-refreshing-last-good"
    case partialTimeout = "F08-partial-timeout"
    case permissionDenied = "F09-permission-denied"
    case offlineCached = "F10-offline-cached"
    case longLabels = "F11-long-labels"
    case layoutEnvelope = "F12-layout-envelope"
    case initialLoading = "F13-initial-loading"
    case globalBridgeError = "F14-global-bridge-error"
}

public struct VisualQAFixture: Sendable {
    public let id: VisualQAFixtureID
    public let glanceRows: [GlanceProviderRow]
    public let statusGlanceRows: [GlanceProviderRow]
    public let surfaces: [SurfaceRow]
    public let accounts: [AccountRow]
    public let providerGroups: [ProviderGroupRow]
    public let popoverSelection: String?
    public let usageSelection: String?
    public let nextRefreshLabel: String
    public let isLoading: Bool
    public let isRefreshing: Bool
    public let globalError: String?

    public var allAccounts: [AccountRow] { accounts }

    public var openaiAccounts: [AccountRow] {
        accounts.filter { $0.surfaceId == "codex" }
    }

    public var openaiDetail: UsageDetailPresentation {
        surfaces.first { $0.id == "codex" }?.detailPresentation ?? .empty
    }

    public var projection: QIFixtureProjection {
        VisualQAFixtures.projection(for: self)
    }

    public var refreshingProjection: QIFixtureProjection {
        VisualQAFixtures.refreshingProjection(for: self)
    }

    public var accountProjections: [String: QIFixtureProjection] {
        VisualQAFixtures.accountProjections(for: self)
    }

    public var input: QIFixtureInput {
        QIFixtureInput(
            projection: projection,
            transitions: QIFixtureTransitions(
                refreshing: refreshingProjection,
                accountSelections: accountProjections
            ),
            selection: QIFixtureSelection(popover: popoverSelection, usage: usageSelection),
            status: QIFixtureStatus(
                nextRefreshLabel: nextRefreshLabel,
                isLoading: isLoading,
                isRefreshing: isRefreshing,
                lastError: globalError
            )
        )
    }
}

public enum VisualQAFixtures: Sendable {
    public static let frozenNow = "2026-08-12T12:00:00+07:00"
    public static let localeIdentifier = "en_US"
    public static let calendarIdentifier = "gregorian"
    public static let timeZoneIdentifier = "Asia/Ho_Chi_Minh"

    public static func fixture(id: VisualQAFixtureID) -> VisualQAFixture {
        makeFixture(id: id)
    }
}
