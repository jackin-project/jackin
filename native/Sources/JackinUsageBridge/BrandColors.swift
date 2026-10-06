// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit
import SwiftUI

/// jackin❯ design tokens — phosphor accent system (HTML `--jk` / `--status-high`).
///
/// LG-A9 selective tint · VS-13 brand accent · FB1-43 CTA hairline.
/// Dark `#5CF07A` · light `#0B774E` (AA-friendly). Never system `Color.accentColor`
/// for healthy metrics, brand mark, selection wells, or Open Usage chrome.
public enum JackinBrand {
    /// Dark-theme phosphor (`--jk` / `--status-high`).
    public static let phosphorDarkSRGB = (r: 0x5C / 255.0, g: 0xF0 / 255.0, b: 0x7A / 255.0)
    /// Light-theme phosphor (`--jk` / `--status-high`).
    public static let phosphorLightSRGB = (r: 0x0B / 255.0, g: 0x77 / 255.0, b: 0x4E / 255.0)

    /// Adaptive product phosphor (appearance-aware).
    public static var phosphor: Color {
        Color(nsColor: phosphorNSColor)
    }

    /// Fixed dark phosphor — QI Dark snapshots / unit equality.
    public static var phosphorDark: Color {
        Color(
            red: phosphorDarkSRGB.r,
            green: phosphorDarkSRGB.g,
            blue: phosphorDarkSRGB.b
        )
    }

    /// Fixed light phosphor — QI Light snapshots.
    public static var phosphorLight: Color {
        Color(
            red: phosphorLightSRGB.r,
            green: phosphorLightSRGB.g,
            blue: phosphorLightSRGB.b
        )
    }

    /// Popover selected-account fill from HTML `color-mix(--jk 88%, --label)`.
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

    /// HTML `--jk-ink`: dark ink on bright Dark accent; white on Light accent.
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
    /// Product phosphor accent — prefer over `Color.accentColor` for jackin chrome.
    public static var jackinPhosphor: Color { JackinBrand.phosphor }
}
