// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import Foundation
import JackinUsageBindings

/// Exact provider/account destination carried across popover/window handoff.
public struct UsageNavigationContext: Equatable, Sendable {
    public let surfaceId: String
    public let accountKey: String?

    public init(surfaceId: String, accountKey: String?) {
        self.surfaceId = surfaceId
        self.accountKey = accountKey
    }
}

/// One already-grouped visual line of a ``UsageDetailRow`` (mirror of the Rust
/// `UsagePresentationLine`). `leading`/`trailing` are finished display strings;
/// Swift never splits, joins, or reorders them.
public struct UsagePresentationLine: Equatable, Sendable {
    public let leading: String?
    public let trailing: String?

    public init(leading: String?, trailing: String?) {
        self.leading = leading
        self.trailing = trailing
    }
}

/// Layout kind of a ``UsageDetailRow`` (pure layout metadata, never prose).
public enum UsageDetailRowKind: String, Equatable, Sendable {
    case metadata
    case bucket
    case detail
    /// Any future Rust kind we do not model yet — rendered as a plain metadata row.
    case unknown

    public init(rawKind: String) {
        self = UsageDetailRowKind(rawValue: rawKind) ?? .unknown
    }
}

/// One provider-detail row mirroring the Rust `UsageDetailRow`.
///
/// Every visible
/// string is Rust-owned. `meterPercent`/`severity` are geometry/style metadata
/// the view may use for bar width and color but never turns into text.
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

/// The complete Rust-owned provider detail.
///
/// It mirrors `UsageDetailPresentation`; rows are already in canonical order.
public struct UsageDetailPresentation: Equatable, Sendable {
    public let rows: [UsageDetailRow]

    public init(rows: [UsageDetailRow]) {
        self.rows = rows
    }

    /// No detail (disabled/unavailable surface, or Overview).
    public static let empty = UsageDetailPresentation(rows: [])

    /// Project the generated boltffi DTO verbatim — no reordering, relabeling, or
    /// string synthesis.
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

/// Pure, importable model for the Usage window.
///
/// It preserves the Rust sidebar
/// order, resolves the incoming selection to the selected surface's Rust detail
/// presentation and account rows, and represents Overview/empty without
/// synthesizing any usage string. It writes no persistence and calls no FFI.
public struct UsageWindowModel: Equatable, Sendable {
    /// Sidebar/detail selection.
    public enum Selection: Equatable, Sendable {
        case overview
        case provider(String)
    }

    /// Selected provider content (nil for Overview / empty).
    public struct Content: Equatable, Sendable {
        public let surfaceId: String
        /// Provider display name for detail head (from glance / surface — Rust only).
        public let displayLabel: String
        /// Rust-owned icon key for the detail-head logo plate.
        public let iconKey: String?
        public let fallbackGlyph: String?
        public let usageURL: String?
        public let identity: PresentationStore.IdentityRow
        public let detail: UsageDetailPresentation
        public let accounts: [PresentationStore.AccountRow]
        public let selectedAccountKey: String?

        public init(
            surfaceId: String,
            displayLabel: String,
            iconKey: String? = nil,
            fallbackGlyph: String?,
            usageURL: String?,
            identity: PresentationStore.IdentityRow,
            detail: UsageDetailPresentation,
            accounts: [PresentationStore.AccountRow],
            selectedAccountKey: String?
        ) {
            self.surfaceId = surfaceId
            self.displayLabel = displayLabel
            self.iconKey = iconKey
            self.fallbackGlyph = fallbackGlyph
            self.usageURL = usageURL
            self.identity = identity
            self.detail = detail
            self.accounts = accounts
            self.selectedAccountKey = selectedAccountKey
        }

        /// Selected account for detail-head subtitle (multi-account); else first.
        public var headAccount: PresentationStore.AccountRow? {
            selectedAccountKey.flatMap { key in
                accounts.first(where: { $0.accountKey == key })
            } ?? accounts.first(where: \.selected) ?? accounts.first
        }
    }

    /// Rust-owned sidebar/Overview rows in canonical (Capsule tab) order.
    public let sidebar: [PresentationStore.GlanceProviderRow]
    public let selection: Selection
    public let content: Content?
    /// No providers detected → the empty-state hint.
    public let isEmpty: Bool

    /// The exact empty-state hint (fixed copy; the only allowed fallback string).
    public static let emptyHint = "no agent credentials found"

    public init(
        glanceRows: [PresentationStore.GlanceProviderRow],
        surfaces: [PresentationStore.SurfaceRow],
        accounts: [PresentationStore.AccountRow],
        providerGroups: [PresentationStore.ProviderGroupRow] = [],
        selection surfaceId: String?,
        accountSelection: String? = nil
    ) {
        sidebar = glanceRows
        isEmpty = glanceRows.isEmpty
        // An invalid/disabled incoming selection falls back to Overview; a valid
        // one resolves to that surface's Rust detail presentation + account rows.
        if let surfaceId,
            let surface = surfaces.first(where: { $0.id == surfaceId && $0.enabled })
        {
            selection = .provider(surfaceId)
            let glance = glanceRows.first(where: { $0.surfaceId == surfaceId })
            let group = providerGroups.first(where: { $0.surfaceId == surfaceId })
            if let identity = surface.identity {
                content = Content(
                    surfaceId: surfaceId,
                    displayLabel: identity.providerTitle,
                    iconKey: group?.iconKey ?? glance?.iconKey,
                    fallbackGlyph: group?.fallbackGlyph ?? glance?.fallbackGlyph,
                    usageURL: group?.usageURL ?? glance?.usageURL,
                    identity: identity,
                    detail: surface.detailPresentation,
                    accounts: accounts.filter { $0.surfaceId == surfaceId },
                    selectedAccountKey: accountSelection
                )
            } else {
                content = nil
            }
        } else {
            selection = .overview
            content = nil
        }
    }
}
