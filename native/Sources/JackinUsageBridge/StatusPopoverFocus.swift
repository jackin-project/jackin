// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import Foundation

/// Pure selection rules for status-item → focused provider popover.
///
/// AppKit maps the clicked button to a `surfaceId` (or fallback); this type only
/// decides the store selection — no UI, no string invention.
public enum StatusPopoverFocus: Sendable {
    /// Full provider inventory owns content; compact status-item rows only supply
    /// optional activity metadata. A refresh failure never replaces retained rows.
    public struct Content: Equatable, Sendable {
        public let group: PresentationStore.ProviderGroupRow
        public let surface: PresentationStore.SurfaceRow
        public let accounts: [PresentationStore.AccountRow]
        public let glance: PresentationStore.GlanceProviderRow?
        public let refreshError: String?

        public var surfaceId: String { group.surfaceId }
        public var displayLabel: String { surface.identity?.providerTitle ?? group.displayLabel }
        public var iconKey: String { group.iconKey }
        public var accountLabel: String { surface.identity?.accountLabel ?? surface.accountLabel }
        public var activityLabel: String { surface.identity?.activityLabel ?? surface.updatedLabel }
        public var accessibilityLabel: String {
            surface.identity?.accessibilityLabel ?? group.accessibilityLabel
        }
        public var isRefreshing: Bool { glance?.isRefreshing ?? false }
        public var lastError: String? { surface.lastError ?? glance?.lastError ?? group.lastError }
    }

    public static func content(
        providerGroups: [PresentationStore.ProviderGroupRow],
        surfaces: [PresentationStore.SurfaceRow],
        accounts: [PresentationStore.AccountRow],
        glanceRows: [PresentationStore.GlanceProviderRow],
        selection: String?,
        refreshError: String?
    ) -> Content? {
        guard let selection,
            let group = providerGroups.first(where: { $0.surfaceId == selection }),
            let surface = surfaces.first(where: { $0.id == group.surfaceId })
        else {
            return nil
        }
        let glance = glanceRows.first { $0.surfaceId == group.surfaceId }
        return Content(
            group: group,
            surface: surface,
            accounts: accounts.filter { $0.surfaceId == group.surfaceId },
            glance: glance,
            refreshError: refreshError
        )
    }

    /// Result of resolving a left-click for the focused popover.
    public enum Outcome: Equatable, Sendable {
        /// No provider selection for the empty-set fallback item.
        case overview
        /// Provider focus for this host surface id.
        case provider(String)
    }

    /// Map a resolved click target to popover selection.
    /// - Parameters:
    ///   - surfaceId: Provider id when a provider status item was clicked.
    ///   - isFallbackItem: True when the empty-set fallback status item was clicked.
    /// - Returns: The provider destination, or Overview for the fallback item.
    public static func outcome(surfaceId: String?, isFallbackItem: Bool) -> Outcome {
        if isFallbackItem { return .overview }
        if let surfaceId, !surfaceId.isEmpty { return .provider(surfaceId) }
        return .overview
    }

    /// `PresentationStore.popoverSelection` value for an outcome.
    public static func popoverSelection(for outcome: Outcome) -> String? {
        switch outcome {
        case .overview: return nil
        case .provider(let id): return id
        }
    }

    /// Find which provider owns a button identity (pure map lookup).
    public static func surfaceId(
        matchingButtonIdentity identity: ObjectIdentifier,
        providerButtonIdentities: [String: ObjectIdentifier]
    ) -> String? {
        for (surfaceId, buttonIdentity) in providerButtonIdentities where buttonIdentity == identity
        {
            return surfaceId
        }
        return nil
    }
}
