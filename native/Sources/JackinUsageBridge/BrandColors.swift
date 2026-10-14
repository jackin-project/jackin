// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit
import SwiftUI

public enum JackinBrand {
    public static let phosphorDarkSRGB = (r: 0x5C / 255.0, g: 0xF0 / 255.0, b: 0x7A / 255.0)
    public static let phosphorLightSRGB = (r: 0x0B / 255.0, g: 0x77 / 255.0, b: 0x4E / 255.0)

    public static var phosphor: Color {
        Color(nsColor: phosphorNSColor)
    }

    public static var phosphorDark: Color {
        Color(
            red: phosphorDarkSRGB.r,
            green: phosphorDarkSRGB.g,
            blue: phosphorDarkSRGB.b
        )
    }

    public static var phosphorLight: Color {
        Color(
            red: phosphorLightSRGB.r,
            green: phosphorLightSRGB.g,
            blue: phosphorLightSRGB.b
        )
    }

    public static func accountSelectionFill(colorScheme: ColorScheme) -> Color {
        if colorScheme == .dark {
            let mixed = NSColor(
                srgbRed: phosphorDarkSRGB.r,
                green: phosphorDarkSRGB.g,
                blue: phosphorDarkSRGB.b,
                alpha: 1
            ).blended(withFraction: 0.12, of: .labelColor)
            return Color(nsColor: mixed ?? phosphorNSColor)
        }
        return phosphorLight
    }

    public static func accountSelectionInk(colorScheme: ColorScheme) -> Color {
        colorScheme == .dark ? .black : .white
    }

    public static let phosphorNSColor = NSColor(
        name: "jackinPhosphor",
        dynamicProvider: { appearance in
            let dark = appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            if dark {
                return NSColor(
                    srgbRed: phosphorDarkSRGB.r,
                    green: phosphorDarkSRGB.g,
                    blue: phosphorDarkSRGB.b,
                    alpha: 1
                )
            }
            return NSColor(
                srgbRed: phosphorLightSRGB.r,
                green: phosphorLightSRGB.g,
                blue: phosphorLightSRGB.b,
                alpha: 1
            )
        }
    )
}

extension Color {
    public static var jackinPhosphor: Color { JackinBrand.phosphor }
}
