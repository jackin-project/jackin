// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0





import AppKit
import JackinDesktopUI

@main
enum JackinDesktopMain {
    static func main() {
        MainActor.assumeIsolated {
            let application = NSApplication.shared


            let delegate = DesktopAppDelegate()
            application.delegate = delegate
            application.setActivationPolicy(.accessory)
            application.run()
        }
    }
}
