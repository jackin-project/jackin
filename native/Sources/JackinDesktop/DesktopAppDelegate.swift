// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit
import JackinUsageBridge

/// Application delegate for jackin❯ Desktop (menu-bar agent).
///
/// Forces accessory activation (no Dock) and opens the Rust usage runtime on
/// cold launch — independent of MenuBarExtra / Settings / Usage window order.
@MainActor
final class DesktopAppDelegate: NSObject, NSApplicationDelegate {
    /// Wired from `JackinDesktopApp` after the store exists.
    var store: PresentationStore?

    func applicationWillFinishLaunching(_ notification: Notification) {
        // WHY: LSUIElement alone is not always enough when a SwiftUI Window
        // scene is also present; pin accessory before the first window maps.
        NSApp.setActivationPolicy(.accessory)
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)
        // Drop any leftover keepalive chrome from earlier builds (UserDefaults
        // may still restore a 1×N pixel window on-screen).
        for window in NSApp.windows where window.title.contains("Keepalive") {
            window.orderOut(nil)
            window.close()
        }
        openRuntimeIfNeeded()
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        // Menu-bar apps must stay alive when the Usage window is closed.
        false
    }

    func applicationDidBecomeActive(_ notification: Notification) {
        openRuntimeIfNeeded()
    }

    private func openRuntimeIfNeeded() {
        guard let store, !store.isOpen else { return }
        store.openDefault()
    }
}
