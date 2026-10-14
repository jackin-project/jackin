// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import Foundation

public enum StatusPopoverFocus: Sendable {
    public enum Outcome: Equatable, Sendable {
        case overview
        case provider(String)
    }

    public static func outcome(surfaceId: String?, isFallbackItem: Bool) -> Outcome {
        if isFallbackItem { return .overview }
        if let surfaceId, !surfaceId.isEmpty { return .provider(surfaceId) }
        return .overview
    }

    public static func popoverSelection(for outcome: Outcome) -> String? {
        switch outcome {
        case .overview: return nil
        case .provider(let id): return id
        }
    }

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
