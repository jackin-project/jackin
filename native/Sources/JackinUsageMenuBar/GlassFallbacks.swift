// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//
// Centralized macOS 26 Liquid Glass availability gates. No other source file
// may contain `#available(macOS 26`. Fallbacks use system materials so Reduce
// Transparency is honored automatically.

import SwiftUI

enum GlassFallbacks {
    /// Chrome-only background (footer bar, chips). Never place behind card body text.
    @ViewBuilder
    static func chromeBackground<Content: View>(@ViewBuilder content: () -> Content) -> some View {
        if #available(macOS 26, *) {
            content()
                .glassEffect(in: .rect(cornerRadius: 8))
        } else {
            content()
                .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 8))
        }
    }

    @ViewBuilder
    static func footerBarBackground() -> some View {
        if #available(macOS 26, *) {
            Rectangle().fill(.clear).glassEffect(in: .rect)
        } else {
            Rectangle().fill(.ultraThinMaterial)
        }
    }

    @ViewBuilder
    static func statusChipBackground(tint: Color) -> some View {
        if #available(macOS 26, *) {
            Capsule().fill(tint.opacity(0.18))
        } else {
            Capsule().fill(tint.opacity(0.15))
        }
    }
}
