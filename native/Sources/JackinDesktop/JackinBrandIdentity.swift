// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit
import SwiftUI

/// Canonical generated jackin❯ identity assets for native surfaces.
@MainActor
public enum JackinBrandIdentity {
    public static func wordmark(for colorScheme: ColorScheme) -> NSImage? {
        loadSVG(named: colorScheme == .dark ? "JackinWordmarkDark" : "JackinWordmarkLight")
    }

    public static func templateMonogram() -> NSImage? {
        guard let image = loadSVG(named: "JackinMonogramDark") else { return nil }
        image.isTemplate = true
        return image
    }

    private static func loadSVG(named name: String) -> NSImage? {
        for bundle in resourceBundles {
            let candidates = [
                bundle.url(forResource: name, withExtension: "svg", subdirectory: "Brand"),
                bundle.url(forResource: name, withExtension: "svg"),
                bundle.resourceURL?.appendingPathComponent("Brand/\(name).svg"),
            ]
            for case let url? in candidates where FileManager.default.fileExists(atPath: url.path) {
                if let image = NSImage(contentsOf: url) {
                    image.isTemplate = false
                    return image
                }
            }
        }
        return nil
    }

    private static var resourceBundles: [Bundle] {
        #if SWIFT_PACKAGE
        [Bundle.module, Bundle.main]
        #else
        [Bundle.main]
        #endif
    }
}

/// Quiet product signature inside the sidebar's system-owned structural plane.
public struct JackinBrandSignature: View {
    @Environment(\.colorScheme) private var colorScheme
    private let width: CGFloat
    private let height: CGFloat

    public init(width: CGFloat = 124, height: CGFloat = 34) {
        self.width = width
        self.height = height
    }

    public var body: some View {
        Group {
            if let wordmark = JackinBrandIdentity.wordmark(for: colorScheme) {
                Image(nsImage: wordmark)
                    .resizable()
                    .scaledToFit()
            }
        }
        .frame(width: width, height: height, alignment: .leading)
        .accessibilityHidden(true)
    }
}
