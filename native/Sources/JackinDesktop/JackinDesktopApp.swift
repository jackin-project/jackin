// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//
// jackin❯ Desktop — display-only shell over Rust UniFFI.
// Clean-room Tahoe/CodexBar conventions; no Swift probes.

import SwiftUI
import JackinUsageBridge

@main
struct JackinDesktopApp: App {
    @NSApplicationDelegateAdaptor(DesktopAppDelegate.self) private var appDelegate
    @StateObject private var store = PresentationStore()

    var body: some Scene {
        MenuBarExtra {
            PopoverRoot(store: store)
                .onAppear { appDelegate.store = store }
        } label: {
            StatusItemLabel(store: store)
                .onAppear { appDelegate.store = store }
        }
        .menuBarExtraStyle(.window)

        Window("jackin❯ Desktop — Usage", id: "usage") {
            UsageWindowRoot(store: store)
        }
        .defaultSize(width: 920, height: 620)
        // Tahoe: unified toolbar sits in the Liquid Glass chrome layer.
        .windowToolbarStyle(.unified)
        .windowResizability(.contentMinSize)

        Settings {
            SettingsView(store: store)
                .formStyle(.grouped)
        }
    }
}
