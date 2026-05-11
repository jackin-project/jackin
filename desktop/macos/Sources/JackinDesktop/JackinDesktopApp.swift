import SwiftUI

@main
struct JackinDesktopApp: App {
    @StateObject private var model = StatusBarModel()

    var body: some Scene {
        WindowGroup("Jackin Desktop", id: "main") {
            DesktopWindowView(model: model)
                .task {
                    await model.refresh()
                }
        }

        MenuBarExtra {
            MainMenuView(model: model)
                .task {
                    await model.refresh()
                }
        } label: {
            Label("Jackin", systemImage: model.symbolName)
        }
        .menuBarExtraStyle(.menu)
    }
}
