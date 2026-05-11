import SwiftUI

struct DesktopWindowView: View {
    @ObservedObject var model: StatusBarModel
    @State private var selection: DesktopWindowSection? = .pullRequests
    @State private var organizationFilter = ""
    @State private var projectFilter = "All"
    @State private var draftFilter: PullRequestDraftFilter = .all

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
        VStack(spacing: 0) {
            pullRequestFilters
            List {
                if filteredPullRequests.isEmpty {
                    ContentUnavailableView(
                        emptyPullRequestsTitle,
                        systemImage: "arrow.triangle.pull"
                    )
                } else {
                    ForEach(filteredPullRequests) { pullRequest in
                        Button {
                            Task {
                                await model.openPullRequest(pullRequest)
                            }
                        } label: {
                            pullRequestRow(pullRequest)
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
        }
        .navigationTitle("Pull Requests")
    }

    private var projectsView: some View {
        VStack(spacing: 0) {
            projectFilters
            List(filteredProjectNames, id: \.self) { project in
                HStack {
                    VStack(alignment: .leading, spacing: 3) {
                        Text(project)
                            .font(.headline)
                        Text(projectOrganization(project))
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    Text("\(pullRequestCount(for: project)) PRs")
                        .foregroundStyle(.secondary)
                }
            }
        }
        .navigationTitle("Projects")
    }

    private var pullRequestFilters: some View {
        HStack(spacing: 12) {
            TextField("Organization", text: $organizationFilter)
                .textFieldStyle(.roundedBorder)
                .frame(maxWidth: 180)

            Picker("Project", selection: $projectFilter) {
                Text("All Projects").tag("All")
                ForEach(projectNamesForCurrentOrganization, id: \.self) { project in
                    Text(project).tag(project)
                }
            }
            .frame(maxWidth: 260)

            Picker("State", selection: $draftFilter) {
                ForEach(PullRequestDraftFilter.allCases) { filter in
                    Text(filter.title).tag(filter)
                }
            }
            .pickerStyle(.segmented)
            .frame(maxWidth: 260)

            Spacer()
            Text("\(filteredPullRequests.count) shown")
                .foregroundStyle(.secondary)
        }
        .padding()
    }

    private var projectFilters: some View {
        HStack(spacing: 12) {
            TextField("Organization", text: $organizationFilter)
                .textFieldStyle(.roundedBorder)
                .frame(maxWidth: 180)

            Spacer()
            Text("\(filteredProjectNames.count) projects")
                .foregroundStyle(.secondary)
        }
        .padding()
    }

    private func pullRequestRow(_ pullRequest: GitHubPullRequest) -> some View {
        HStack {
            VStack(alignment: .leading, spacing: 3) {
                HStack {
                    Text(pullRequest.title)
                        .font(.headline)
                    if pullRequest.isDraft {
                        Text("Draft")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
                Text("\(pullRequest.repository) #\(pullRequest.number)")
                    .foregroundStyle(.secondary)
            }
            Spacer()
            Image(systemName: "arrow.up.forward.square")
                .foregroundStyle(.secondary)
        }
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

    private var projectNamesForCurrentOrganization: [String] {
        if organizationFilter.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            projectNames
        } else {
            projectNames.filter { projectOrganization($0).localizedCaseInsensitiveContains(organizationFilter) }
        }
    }

    private var filteredProjectNames: [String] {
        projectNamesForCurrentOrganization
    }

    private var filteredPullRequests: [GitHubPullRequest] {
        model.pullRequests.filter { pullRequest in
            if !organizationFilter.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
               !projectOrganization(pullRequest.repository).localizedCaseInsensitiveContains(organizationFilter) {
                return false
            }
            if projectFilter != "All", pullRequest.repository != projectFilter {
                return false
            }
            switch draftFilter {
            case .all:
                return true
            case .open:
                return !pullRequest.isDraft
            case .draft:
                return pullRequest.isDraft
            }
        }
    }

    private func pullRequestCount(for project: String) -> Int {
        model.pullRequests.filter { $0.repository == project }.count
    }

    private func projectOrganization(_ repository: String) -> String {
        repository.split(separator: "/", maxSplits: 1).first.map(String.init) ?? repository
    }

    private var emptyPullRequestsTitle: String {
        if let pullRequestError = model.pullRequestError {
            return pullRequestError
        }
        return model.pullRequests.isEmpty ? "No Open Pull Requests" : "No Matching Pull Requests"
    }
}

enum PullRequestDraftFilter: String, CaseIterable, Identifiable {
    case all
    case open
    case draft

    var id: Self { self }

    var title: String {
        switch self {
        case .all:
            "All"
        case .open:
            "Open"
        case .draft:
            "Draft"
        }
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
