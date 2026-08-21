// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import CoreGraphics
import Foundation

/// Explicit process-local controls for deterministic native visual-QA runs.
struct VisualQALaunchOptions: Equatable {
    enum Selection: Equatable {
        case fixtureDefault
        case overview
        case provider(String)
    }

    let fixtureID: VisualQAFixtureID?
    let invalidFixtureID: String?
    let openUsage: Bool
    let openPopover: Bool
    let selection: Selection
    let windowSize: CGSize?
    let elevatesFixtureWindow: Bool

    var usesFixture: Bool { fixtureID != nil || invalidFixtureID != nil }

    static func resolve(
        arguments: [String],
        environment _: [String: String]
    ) -> VisualQALaunchOptions {
        let rawFixture = value(after: "--fixture", in: arguments)
        let fixtureID = rawFixture.flatMap(VisualQAFixtureID.init(rawValue:))
        let rawSelection = value(after: "--selection", in: arguments)
        let selection: Selection
        if rawSelection == "overview" {
            selection = .overview
        } else if let rawSelection {
            selection = .provider(rawSelection)
        } else {
            selection = .fixtureDefault
        }
        let size = value(after: "--window-size", in: arguments).flatMap(parseSize)
        return VisualQALaunchOptions(
            fixtureID: fixtureID,
            invalidFixtureID: rawFixture != nil && fixtureID == nil ? rawFixture : nil,
            openUsage: arguments.contains("--open-usage"),
            openPopover: arguments.contains("--open-popover"),
            selection: selection,
            windowSize: size,
            elevatesFixtureWindow: rawFixture != nil && arguments.contains("--ui-test")
        )
    }

    private static func value(after flag: String, in arguments: [String]) -> String? {
        guard let index = arguments.firstIndex(of: flag), arguments.indices.contains(index + 1)
        else {
            return nil
        }
        return arguments[index + 1]
    }

    private static func parseSize(_ value: String) -> CGSize? {
        let parts = value.lowercased().split(separator: "x", omittingEmptySubsequences: false)
        guard parts.count == 2,
            let width = Double(parts[0]),
            let height = Double(parts[1]),
            width >= UsageWindowMetrics.minimumContentSize.width,
            height >= UsageWindowMetrics.minimumContentSize.height
        else { return nil }
        return CGSize(width: width, height: height)
    }
}
