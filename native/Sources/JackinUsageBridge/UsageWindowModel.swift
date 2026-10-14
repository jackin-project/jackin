// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import Foundation
import JackinUsageBindings


public struct UsageNavigationContext: Equatable, Sendable {
    public let surfaceId: String
    public let accountKey: String?

    public init(surfaceId: String, accountKey: String?) {
        self.surfaceId = surfaceId
        self.accountKey = accountKey
    }
}




public struct UsagePresentationLine: Equatable, Sendable {
    public let leading: String?
    public let trailing: String?

    public init(leading: String?, trailing: String?) {
        self.leading = leading
        self.trailing = trailing
    }
}


public enum UsageDetailRowKind: String, Equatable, Sendable {
    case metadata
    case bucket
    case detail

    case unknown

    public init(rawKind: String) {
        self = UsageDetailRowKind(rawValue: rawKind) ?? .unknown
    }
}






public struct UsageDetailRow: Identifiable, Equatable, Sendable {
    public var id: String { rowId }
    public let rowId: String
    public let kind: UsageDetailRowKind
    public let label: String
    public let layoutLines: [UsagePresentationLine]
    public let displayLabel: String
    public let meterPercent: UInt8?
    public let severity: String

    public init(
        rowId: String,
        kind: UsageDetailRowKind,
        label: String,
        layoutLines: [UsagePresentationLine],
        displayLabel: String,
        meterPercent: UInt8?,
        severity: String
    ) {
        self.rowId = rowId
        self.kind = kind
        self.label = label
        self.layoutLines = layoutLines
        self.displayLabel = displayLabel
        self.meterPercent = meterPercent
        self.severity = severity
    }
}




public struct UsageDetailPresentation: Equatable, Sendable {
    public let rows: [UsageDetailRow]

    public init(rows: [UsageDetailRow]) {
        self.rows = rows
    }


    public static let empty = UsageDetailPresentation(rows: [])



    public init(dto: UsageDetailPresentationDto) {
        rows = dto.rows.map { row in
            UsageDetailRow(
                rowId: row.rowId,
                kind: UsageDetailRowKind(rawKind: row.kind),
                label: row.label,
                layoutLines: row.layoutLines.map {
                    UsagePresentationLine(leading: $0.leading, trailing: $0.trailing)
                },
                displayLabel: row.displayLabel,
                meterPercent: row.meterPercent,
                severity: row.severity
            )
        }
    }
}







public struct UsageWindowModel: Equatable, Sendable {

    public enum Selection: Equatable, Sendable {
        case overview
        case provider(String)
    }

    public struct AccountRouteNotice: Equatable, Sendable {
        public let surfaceId: String
        public let message: String
    }


    public struct Content: Equatable, Sendable {
        public let surfaceId: String

        public let displayLabel: String

        public let iconKey: String?
        public let fallbackGlyph: String?
        public let usageURL: String?
        public let identity: PresentationStore.IdentityRow
        public let detail: UsageDetailPresentation
        public let accounts: [PresentationStore.AccountRow]
        public let selectedAccountRoute: PresentationStore.SelectedAccountRoute

        public init(
            surfaceId: String,
            displayLabel: String,
            iconKey: String? = nil,
            fallbackGlyph: String?,
            usageURL: String?,
            identity: PresentationStore.IdentityRow,
            detail: UsageDetailPresentation,
            accounts: [PresentationStore.AccountRow],
            selectedAccountRoute: PresentationStore.SelectedAccountRoute
        ) {
            self.surfaceId = surfaceId
            self.displayLabel = displayLabel
            self.iconKey = iconKey
            self.fallbackGlyph = fallbackGlyph
            self.usageURL = usageURL
            self.identity = identity
            self.detail = detail
            self.accounts = accounts
            self.selectedAccountRoute = selectedAccountRoute
        }


        public var selectedAccountKey: String? {
            if case .available(let accountKey) = selectedAccountRoute {
                return accountKey
            }
            return nil
        }


        public var headAccount: PresentationStore.AccountRow? {
            selectedAccountKey.flatMap { key in
                accounts.first(where: { $0.accountKey == key })
            }
        }
    }


    public let sidebar: [PresentationStore.GlanceProviderRow]
    public let selection: Selection
    public let content: Content?
    public let routeNotice: AccountRouteNotice?

    public let isEmpty: Bool


    public static let emptyHint = "no agent credentials found"

    public init(
        glanceRows: [PresentationStore.GlanceProviderRow],
        surfaces: [PresentationStore.SurfaceRow],
        accounts: [PresentationStore.AccountRow],
        providerGroups: [PresentationStore.ProviderGroupRow] = [],
        selection surfaceId: String?
    ) {
        sidebar = glanceRows
        isEmpty = glanceRows.isEmpty


        if let surfaceId,
            let surface = surfaces.first(where: { $0.id == surfaceId && $0.enabled })
        {
            let group = providerGroups.first(where: { $0.surfaceId == surfaceId })
            let surfaceAccounts = group?.accounts ?? accounts.filter { $0.surfaceId == surfaceId }
            let route: PresentationStore.SelectedAccountRoute? = group == nil
                ? .unselected : group?.selectedAccountRoute
            guard let route else {
                selection = .overview
                content = nil
                routeNotice = nil
                return
            }
            switch route {
            case .unselected:
                break
            case .resolving(let accountKey):
                guard !surfaceAccounts.contains(where: { $0.accountKey == accountKey }) else {
                    selection = .overview
                    content = nil
                    routeNotice = nil
                    return
                }
            case .available(let accountKey):
                guard surfaceAccounts.contains(where: { $0.accountKey == accountKey }) else {
                    selection = .overview
                    content = nil
                    routeNotice = nil
                    return
                }
            case .unavailable(let accountKey, let notice):
                guard !surfaceAccounts.contains(where: { $0.accountKey == accountKey }),
                    !notice.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                else {
                    selection = .overview
                    content = nil
                    routeNotice = nil
                    return
                }
                selection = .overview
                content = nil
                routeNotice = AccountRouteNotice(surfaceId: surfaceId, message: notice)
                return
            }

            selection = .provider(surfaceId)
            routeNotice = nil
            let glance = glanceRows.first(where: { $0.surfaceId == surfaceId })
            if let identity = surface.identity {
                content = Content(
                    surfaceId: surfaceId,
                    displayLabel: identity.providerTitle,
                    iconKey: group?.iconKey ?? glance?.iconKey,
                    fallbackGlyph: group?.fallbackGlyph ?? glance?.fallbackGlyph,
                    usageURL: group?.usageURL ?? glance?.usageURL,
                    identity: identity,
                    detail: surface.detailPresentation,
                    accounts: surfaceAccounts,
                    selectedAccountRoute: route
                )
            } else {
                content = nil
            }
        } else {
            selection = .overview
            content = nil
            routeNotice = nil
        }
    }
}
