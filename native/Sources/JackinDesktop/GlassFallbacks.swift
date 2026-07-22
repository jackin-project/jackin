// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//
// Centralized macOS 26 Liquid Glass availability gates.
//
// HIG / Adopting Liquid Glass (Apple):
// - Liquid Glass is for the **navigation / control layer** that floats above content
//   (sidebars, toolbars, popovers, menus, floating controls).
// - Do **not** put Liquid Glass on the content layer (lists of data, provider cards,
//   long-form text). Content uses standard materials / solid fills so hierarchy stays clear.
// - Fallbacks use system materials so Reduce Transparency is honored.
//
// No other source file may contain `#available(macOS 26`.

import AppKit
import SwiftUI

enum GlassFallbacks {
    // MARK: - Corner radii (continuous / concentric family)

    /// Floating glance panel (popover) outer radius.
    static let panelCornerRadius: CGFloat = 20
    /// Inset chrome tiles / control islands inside the panel.
    static let chromeTileCornerRadius: CGFloat = 12
    /// Content-layer cards (standard materials only — not glass).
    static let contentCardCornerRadius: CGFloat = 12
    /// Status-item chip capsule.
    static let chipCornerRadius: CGFloat = 8

    // MARK: - Chrome / navigation layer (Liquid Glass on 26+)

    /// Chrome-only background (footer bar, chips). Never place behind card body text.
    @ViewBuilder
    static func chromeBackground<Content: View>(@ViewBuilder content: () -> Content) -> some View {
        if #available(macOS 26, *) {
            content()
                .glassEffect(.regular, in: .rect(cornerRadius: chromeTileCornerRadius))
        } else {
            content()
                .background(
                    .ultraThinMaterial,
                    in: RoundedRectangle(cornerRadius: chromeTileCornerRadius, style: .continuous)
                )
        }
    }

    @ViewBuilder
    static func footerBarBackground() -> some View {
        if #available(macOS 26, *) {
            Rectangle().fill(.clear).glassEffect(.regular, in: .rect)
        } else {
            Rectangle().fill(.ultraThinMaterial)
        }
    }

    /// Plan / status pill (subtle fill — content-adjacent chrome).
    @ViewBuilder
    static func statusChipBackground(tint: Color) -> some View {
        if #available(macOS 26, *) {
            Capsule().fill(tint.opacity(0.16))
        } else {
            Capsule().fill(tint.opacity(0.14))
        }
    }

    /// Usage-window sidebar chrome (glass on 26, system sidebar material earlier).
    /// Liquid Glass sidebars float above content; content should not use glass.
    @ViewBuilder
    static func sidebarBackground() -> some View {
        if #available(macOS 26, *) {
            Rectangle().fill(.clear).glassEffect(.regular, in: .rect)
        } else {
            Rectangle().fill(.ultraThinMaterial)
        }
    }

    /// Usage-window detail pane background — **standard material only** (content layer).
    @ViewBuilder
    static func windowContentBackground() -> some View {
        // HIG: no Liquid Glass in the content layer.
        Rectangle().fill(Color(nsColor: .windowBackgroundColor).opacity(0.92))
    }

    /// Detached floating-panel surface for the glance popover (chrome only).
    /// Large continuous radius + glass so the panel reads as Tahoe menu chrome.
    @ViewBuilder
    static func panelSurfaceBackground() -> some View {
        if #available(macOS 26, *) {
            RoundedRectangle(cornerRadius: panelCornerRadius, style: .continuous)
                .fill(.clear)
                .glassEffect(.regular, in: .rect(cornerRadius: panelCornerRadius))
                .shadow(color: .black.opacity(0.22), radius: 28, y: 12)
        } else {
            RoundedRectangle(cornerRadius: panelCornerRadius, style: .continuous)
                .fill(.ultraThinMaterial)
                .shadow(color: .black.opacity(0.18), radius: 24, y: 10)
        }
    }

    /// Floating control island behind agent tile grids / toolbar groups.
    @ViewBuilder
    static func floatingChromeIsland() -> some View {
        if #available(macOS 26, *) {
            RoundedRectangle(cornerRadius: chromeTileCornerRadius, style: .continuous)
                .fill(.clear)
                .glassEffect(.regular, in: .rect(cornerRadius: chromeTileCornerRadius))
        } else {
            RoundedRectangle(cornerRadius: chromeTileCornerRadius, style: .continuous)
                .fill(.thinMaterial)
        }
    }

    /// Selected agent tile fill (interactive control — glass-capable).
    @ViewBuilder
    static func selectedControlFill() -> some View {
        if #available(macOS 26, *) {
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .fill(Color.accentColor.opacity(0.92))
        } else {
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .fill(Color.accentColor)
        }
    }

    /// Unselected agent tile fill (subtle elevated control).
    @ViewBuilder
    static func idleControlFill(enabled: Bool) -> some View {
        RoundedRectangle(cornerRadius: 10, style: .continuous)
            .fill(Color.primary.opacity(enabled ? 0.07 : 0.03))
    }

    /// Menu-bar status chip glass capsule (per-provider icon + %).
    @ViewBuilder
    static func statusItemChipBackground(severity: Color) -> some View {
        if #available(macOS 26, *) {
            Capsule(style: .continuous)
                .fill(.clear)
                .glassEffect(.regular, in: Capsule(style: .continuous))
                .overlay {
                    Capsule(style: .continuous)
                        .strokeBorder(severity.opacity(0.22), lineWidth: 0.5)
                }
        } else {
            Capsule(style: .continuous)
                .fill(.ultraThinMaterial)
                .overlay {
                    Capsule(style: .continuous)
                        .strokeBorder(severity.opacity(0.18), lineWidth: 0.5)
                }
        }
    }

    /// Content-layer card fill (standard materials only — HIG content layer).
    @ViewBuilder
    static func contentCardBackground() -> some View {
        RoundedRectangle(cornerRadius: contentCardCornerRadius, style: .continuous)
            .fill(.background.secondary)
    }
}
