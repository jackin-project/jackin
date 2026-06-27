import SwiftUI
import AppKit

@main
struct JackinDesktopApp: App {
    // A SwiftPM executable launches unbundled, so it does not get a Dock
    // icon or foreground focus on its own. The delegate promotes the
    // process to a regular app and brings the window forward so
    // `swift run` actually shows a window during early iteration.
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var delegate

    var body: some Scene {
        WindowGroup("jackin'") {
            ContentView()
        }
        .windowResizability(.contentMinSize)
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        true
    }
}
