// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit
import JackinUsageBridge
import SwiftUI

/// Lazily creates and retains the AppKit Usage window hosting the existing
/// `UsageWindowRoot`. Plan 008 owns the window's content; this controller only
/// owns its lifecycle and focus.
@MainActor
final class UsageWindowController {
    private let store: PresentationStore
    private var window: NSWindow?

    init(store: PresentationStore) {
        self.store = store
    }

    /// Show the Usage window, focused on a provider surface id (`nil` = Overview),
    /// creating it on first use and reusing it afterward.
    func show(focusOn surfaceId: String?) {
        store.selectUsageSurface(surfaceId)
        let window = window ?? makeWindow()
        self.window = window
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    private func makeWindow() -> NSWindow {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 920, height: 620),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.title = "jackin❯ Desktop — Usage"
        window.isReleasedWhenClosed = false
        window.contentView = NSHostingView(rootView: UsageWindowRoot(store: store))
        window.center()
        window.setFrameAutosaveName("jackin.desktop.usage-window")
        return window
    }

    func invalidate() {
        window?.orderOut(nil)
        window?.contentView = nil
        window = nil
    }
}
