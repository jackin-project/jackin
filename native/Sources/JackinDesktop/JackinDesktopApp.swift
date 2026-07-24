// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//
// jackin❯ Desktop — display-only shell over Rust UniFFI.
// Minimal AppKit menu-agent bootstrap: no SwiftUI App/Scene graph, no window.
// The delegate owns the store, per-provider status items, and the popover.

import AppKit

@main
enum JackinDesktopMain {
    static func main() {
        MainActor.assumeIsolated {
            let application = NSApplication.shared
            // Retained for the whole run loop: `NSApplication.delegate` is weak,
            // and `run()` blocks until termination so this local outlives it.
            let delegate = DesktopAppDelegate()
            application.delegate = delegate
            application.setActivationPolicy(.accessory)
            application.run()
        }
    }
}
