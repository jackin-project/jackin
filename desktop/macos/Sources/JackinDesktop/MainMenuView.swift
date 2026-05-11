import AppKit
import SwiftUI

struct MainMenuView: View {
    @ObservedObject var model: StatusBarModel

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            switch model.daemon {
            case .checking:
                Label("Checking daemon", systemImage: "circle.dotted")
            case .connected(let hello):
                Label("Daemon connected", systemImage: "checkmark.circle")
                Text("jackin \(hello.version)")
                Text("Protocol \(hello.minProtocol)-\(hello.maxProtocol)")
                Text("\(hello.capabilities.count) capabilities")
                Text("\(model.sessions.count) running agents")
            case .disconnected(let message):
                Label("Daemon disconnected", systemImage: "exclamationmark.triangle")
                Text(message)
                    .lineLimit(3)
            }

            Divider()

            if !model.workspaces.isEmpty {
                Section("Workspaces") {
                    ForEach(model.workspaces) { workspace in
                        workspaceRow(workspace)
                    }
                }
                Divider()
            }

            Button {
                Task {
                    await model.refresh()
                }
            } label: {
                Label(model.isRefreshing ? "Refreshing..." : "Refresh", systemImage: "arrow.clockwise")
            }

            Button {
                NSApplication.shared.terminate(nil)
            } label: {
                Label("Quit Jackin Desktop", systemImage: "power")
            }
        }
        .padding(8)
    }

    private func workspaceRow(_ workspace: DesktopWorkspace) -> some View {
        let running = model.runningCount(for: workspace)
        return HStack {
            VStack(alignment: .leading) {
                Text(workspace.name)
                Text(workspace.defaultRole ?? workspace.workdir)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            Text(running == 1 ? "1 agent" : "\(running) agents")
                .foregroundStyle(running > 0 ? .primary : .secondary)
        }
    }
}
