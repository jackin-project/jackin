import SwiftUI

struct DesktopWindowView: View {
    @ObservedObject var model: StatusBarModel
    @State private var selection: DesktopWindowSection? = .pullRequests

    var body: some View {
        NavigationSplitView {
            List(DesktopWindowSection.allCases, selection: $selection) { section in
                Label(section.title, systemImage: section.symbolName)
            }
            .navigationTitle("Jackin")
        } detail: {
            switch selection {
            case .pullRequests:
                pullRequestsView
            case .projects:
                projectsView
            case .workspaces:
                workspacesView
            case .runningAgents:
                runningAgentsView
            case .accounts:
                accountsView
            case .settings:
                settingsView
            case nil:
                ContentUnavailableView("Select a Section", systemImage: "sidebar.left")
            }
        }
        .frame(minWidth: 920, minHeight: 560)
        .toolbar {
            ToolbarItem {
                Button {
                    Task {
                        await model.refresh()
                    }
                } label: {
                    Label("Refresh", systemImage: "arrow.clockwise")
                }
                .disabled(model.isRefreshing)
            }
        }
    }

    private var pullRequestsView: some View {
        List {
            if model.pullRequests.isEmpty {
                ContentUnavailableView(
                    model.pullRequestError ?? "No Open Pull Requests",
                    systemImage: "arrow.triangle.pull"
                )
            } else {
                ForEach(model.pullRequests) { pullRequest in
                    Button {
                        Task {
                            await model.openPullRequest(pullRequest)
                        }
                    } label: {
                        HStack {
                            VStack(alignment: .leading, spacing: 3) {
                                Text(pullRequest.title)
                                    .font(.headline)
                                Text("\(pullRequest.repository) #\(pullRequest.number)")
                                    .foregroundStyle(.secondary)
                            }
                            Spacer()
                            Image(systemName: "arrow.up.forward.square")
                                .foregroundStyle(.secondary)
                        }
                    }
                    .buttonStyle(.plain)
                }
            }
        }
        .navigationTitle("Pull Requests")
    }

    private var projectsView: some View {
        List(projectNames, id: \.self) { project in
            HStack {
                Text(project)
                Spacer()
                Text("\(pullRequestCount(for: project)) PRs")
                    .foregroundStyle(.secondary)
            }
        }
        .navigationTitle("Projects")
    }

    private var workspacesView: some View {
        List(model.workspaces) { workspace in
            HStack {
                VStack(alignment: .leading, spacing: 3) {
                    Text(workspace.name)
                        .font(.headline)
                    Text(workspace.workdir)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Text("\(model.runningCount(for: workspace)) running")
                    .foregroundStyle(.secondary)
            }
        }
        .navigationTitle("Workspaces")
    }

    private var runningAgentsView: some View {
        List(model.sessions) { session in
            VStack(alignment: .leading, spacing: 3) {
                Text(session.displayName)
                    .font(.headline)
                Text([session.workspace, session.role, session.agent].compactMap { $0 }.joined(separator: " / "))
                    .foregroundStyle(.secondary)
            }
        }
        .navigationTitle("Running Agents")
    }

    private var accountsView: some View {
        ContentUnavailableView("Account Status Comes Next", systemImage: "person.crop.circle.badge.checkmark")
            .navigationTitle("Accounts")
    }

    private var settingsView: some View {
        Form {
            LabeledContent("Daemon Socket", value: "~/.jackin/run/jackin-daemon.sock")
            LabeledContent("Protocol", value: "2")
        }
        .padding()
        .navigationTitle("Settings")
    }

    private var projectNames: [String] {
        Array(Set(model.pullRequests.map(\.repository))).sorted()
    }

    private func pullRequestCount(for project: String) -> Int {
        model.pullRequests.filter { $0.repository == project }.count
    }
}

enum DesktopWindowSection: String, CaseIterable, Identifiable {
    case pullRequests
    case projects
    case workspaces
    case runningAgents
    case accounts
    case settings

    var id: Self { self }

    var title: String {
        switch self {
        case .pullRequests:
            "Pull Requests"
        case .projects:
            "Projects"
        case .workspaces:
            "Workspaces"
        case .runningAgents:
            "Running Agents"
        case .accounts:
            "Accounts"
        case .settings:
            "Settings"
        }
    }

    var symbolName: String {
        switch self {
        case .pullRequests:
            "arrow.triangle.pull"
        case .projects:
            "folder"
        case .workspaces:
            "rectangle.3.group"
        case .runningAgents:
            "bolt.horizontal"
        case .accounts:
            "person.crop.circle.badge.checkmark"
        case .settings:
            "gearshape"
        }
    }
}
