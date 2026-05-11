import SwiftUI

@main
struct JackinDesktopApp: App {
    @StateObject private var model = StatusBarModel()

    var body: some Scene {
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
