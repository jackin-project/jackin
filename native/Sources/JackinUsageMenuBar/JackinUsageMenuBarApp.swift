// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//
// jackin❯ usage menu bar — display-only shell over Rust UniFFI.
// Clean-room Tahoe/CodexBar conventions; no Swift probes.

import SwiftUI
import JackinUsageBridge

@main
struct JackinUsageMenuBarApp: App {
    @StateObject private var store = PresentationStore()

    var body: some Scene {
        // WHY: SettingsLink from MenuBarExtra is unreliable on Tahoe without an
        // existing SwiftUI render tree; a 1×1 hidden window keeps one alive.
        Window("JackinUsageMenuBarKeepalive", id: "keepalive") {
            Color.clear
                .frame(width: 1, height: 1)
                .accessibilityHidden(true)
        }
        .windowResizability(.contentSize)
        .defaultSize(width: 1, height: 1)

        MenuBarExtra {
            PopoverRoot(store: store)
        } label: {
            StatusItemLabel(store: store)
        }
        .menuBarExtraStyle(.window)

        Settings {
            SettingsView(store: store)
        }
    }
}
