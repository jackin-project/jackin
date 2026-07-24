// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import Foundation

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

/// One provider-detail row (mirror of the Rust `UsageDetailRow`). Every visible
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

/// The complete Rust-owned provider-detail card (mirror of
/// `UsageDetailPresentation`). Rows are already in canonical order.
public struct UsageDetailPresentation: Equatable, Sendable {
    public let rows: [UsageDetailRow]

    public init(rows: [UsageDetailRow]) {
        self.rows = rows
    }

    /// No detail (disabled/unavailable surface, or Overview).
    public static let empty = UsageDetailPresentation(rows: [])

    /// Project the generated UniFFI DTO verbatim — no reordering, relabeling, or
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

/// Pure, importable model for the Usage window. It preserves the Rust sidebar
/// order, resolves the incoming selection to the selected surface's Rust detail
/// presentation and account rows, and represents Overview/empty without
/// synthesizing any usage string. It writes no persistence and calls no FFI;
/// `UsageWindowRoot` routes ``Action`` values to the store's one mutation each.
public struct UsageWindowModel: Equatable, Sendable {
    /// Sidebar/detail selection.
    public enum Selection: Equatable, Sendable {
        case overview
        case provider(String)
    }

    /// Navigation intents carrying only existing surface/account keys.
    public enum Action: Equatable, Sendable {
        case selectOverview
        case selectProvider(String)
        case selectAccount(surfaceId: String, accountKey: String)
    }

    /// Selected provider content (nil for Overview / empty).
    public struct Content: Equatable, Sendable {
        public let surfaceId: String
        public let detail: UsageDetailPresentation
        public let accounts: [PresentationStore.AccountRow]

        public init(
            surfaceId: String,
            detail: UsageDetailPresentation,
            accounts: [PresentationStore.AccountRow]
        ) {
            self.surfaceId = surfaceId
            self.detail = detail
            self.accounts = accounts
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
        selection surfaceId: String?
    ) {
        sidebar = glanceRows
        isEmpty = glanceRows.isEmpty
        // An invalid/disabled incoming selection falls back to Overview; a valid
        // one resolves to that surface's Rust detail presentation + account rows.
        if let surfaceId,
            let surface = surfaces.first(where: { $0.id == surfaceId && $0.enabled })
        {
            selection = .provider(surfaceId)
            content = Content(
                surfaceId: surfaceId,
                detail: surface.detailPresentation,
                accounts: accounts.filter { $0.surfaceId == surfaceId }
            )
        } else {
            selection = .overview
            content = nil
        }
    }

    /// The store selection an ``Action`` resolves to (`nil` = Overview). The
    /// account action keeps the current provider selection.
    public func selection(after action: Action) -> String? {
        switch action {
        case .selectOverview:
            return nil
        case .selectProvider(let surfaceId):
            return surfaceId
        case .selectAccount:
            if case .provider(let surfaceId) = selection {
                return surfaceId
            }
            return nil
        }
    }
}
