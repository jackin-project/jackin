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

            Section("Pull Requests") {
                if model.pullRequests.isEmpty {
                    Text(model.pullRequestError ?? "No open pull requests")
                        .foregroundStyle(.secondary)
                } else {
                    ForEach(model.pullRequests) { pullRequest in
                        pullRequestRow(pullRequest)
                    }
                }
            }

            if let openError = model.openError {
                Text(openError)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }

            Divider()

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

    private func pullRequestRow(_ pullRequest: GitHubPullRequest) -> some View {
        Button {
            Task {
                await model.openPullRequest(pullRequest)
            }
        } label: {
            HStack {
                VStack(alignment: .leading) {
                    Text("#\(pullRequest.number) \(pullRequest.title)")
                        .lineLimit(1)
                    Text(pullRequest.repository)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Image(systemName: "arrow.up.forward.square")
                    .foregroundStyle(.secondary)
            }
        }
    }
}
