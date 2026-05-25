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
                        HStack {
                            pullRequestRow(pullRequest)
                            Spacer()
                            pullRequestActions(pullRequest)
                        }
                    }
                }
            }
        }
        .navigationTitle("Pull Requests")
    }

    private var projectsView: some View {
        VStack(spacing: 0) {
            projectFilters
            List(filteredProjectSummaries) { project in
                VStack(alignment: .leading, spacing: 10) {
                    HStack {
                        VStack(alignment: .leading, spacing: 3) {
                            Text(project.repository)
                                .font(.headline)
                            Text(projectDetailText(project))
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        Button {
                            Task {
                                await model.openRepository(project.repository)
                            }
                        } label: {
                            Label("GitHub", systemImage: "arrow.up.forward.square")
                        }
                        Button {
                            Task {
                                await model.openRepositoryPullRequests(project.repository)
                            }
                        } label: {
                            Label("Pull Requests", systemImage: "arrow.triangle.pull")
                        }
                    }
                    ForEach(project.runningSessions) { session in
                        sessionSummaryRow(session)
                            .padding(.leading, 12)
                    }
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
            VStack(alignment: .leading, spacing: 10) {
                HStack {
                    VStack(alignment: .leading, spacing: 3) {
                        Text(workspace.name)
                            .font(.headline)
                        Text(workspaceSubtitle(workspace))
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    Text("\(model.runningCount(for: workspace)) running")
                        .foregroundStyle(.secondary)
                    Button {
                        Task {
                            await model.launchWorkspace(workspace)
                        }
                    } label: {
                        Label("Launch", systemImage: "terminal")
                    }
                }
                ForEach(model.sessions.filter { $0.workspace == workspace.name }) { session in
                    sessionSummaryRow(session)
                        .padding(.leading, 12)
                }
            }
        }
        .navigationTitle("Workspaces")
    }

    private var runningAgentsView: some View {
        List(model.sessions) { session in
            VStack(alignment: .leading, spacing: 10) {
                HStack {
                    VStack(alignment: .leading, spacing: 3) {
                        Text(session.displayName)
                            .font(.headline)
                        Text(sessionSubtitle(session))
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    Text(session.status)
                        .foregroundStyle(.secondary)
                    Button {
                        Task {
                            await model.openSession(session)
                        }
                    } label: {
                        Label("Open", systemImage: "terminal")
                    }
                }
                sessionSummaryRow(session)
            }
        }
        .navigationTitle("Running Agents")
    }

    private func sessionSummaryRow(_ session: DesktopSession) -> some View {
        HStack(alignment: .firstTextBaseline) {
            VStack(alignment: .leading, spacing: 3) {
                Text(session.repository ?? "No repository detected")
                    .font(.subheadline)
                Text(sessionBranchText(session))
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            if let pullRequest = session.linkedPullRequest {
                pullRequestActions(pullRequest)
            } else if let repository = session.repository {
                Button {
                    Task {
                        await model.openRepositoryPullRequests(repository)
                    }
                } label: {
                    Label("Pull Requests", systemImage: "arrow.triangle.pull")
                }
            } else {
                Text("No PR detected")
                    .foregroundStyle(.secondary)
            }
        }
    }

    private func pullRequestActions(_ pullRequest: GitHubPullRequest) -> some View {
        HStack(spacing: 8) {
            Button {
                Task {
                    await model.openPullRequest(pullRequest)
                }
            } label: {
                Label("GitHub", systemImage: "arrow.up.forward.square")
            }
            Button {
                Task {
                    await model.openPullRequestInDiffsHub(pullRequest)
                }
            } label: {
                Label("DiffsHub", systemImage: "doc.text.magnifyingglass")
            }
        }
    }

    private var accountsView: some View {
        List {
            if let accountStatusError = model.accountStatusError {
                ContentUnavailableView(accountStatusError, systemImage: "exclamationmark.triangle")
            } else if model.accountProviders.isEmpty {
                ContentUnavailableView("No Account Status", systemImage: "person.crop.circle.badge.questionmark")
            } else {
                Section {
                    ForEach(model.accountProviders) { provider in
                        HStack {
                            VStack(alignment: .leading, spacing: 3) {
                                Text(provider.provider)
                                    .font(.headline)
                                Text(provider.detail)
                                    .foregroundStyle(.secondary)
                            }
                            Spacer()
                            VStack(alignment: .trailing, spacing: 3) {
                                HStack(spacing: 6) {
                                    Circle()
                                        .fill(accountStateColor(provider.state))
                                        .frame(width: 8, height: 8)
                                    Text(provider.state)
                                }
                                Text(provider.source ?? "No credential source")
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                        }
                    }
                } footer: {
                    Text(accountStatusFooter)
                }
            }
        }
        .toolbar {
            ToolbarItem {
                Button {
                    Task {
                        await model.refreshAccountStatus()
                    }
                } label: {
                    Label("Refresh Accounts", systemImage: "arrow.clockwise")
                }
            }
        }
        .navigationTitle("Accounts")
    }

    private var settingsView: some View {
        Form {
            LabeledContent("Daemon Socket", value: "~/.jackin/run/jackin-daemon.sock")
            LabeledContent("Protocol", value: "2")
            LabeledContent("Event Streaming", value: model.eventSubscription?.streaming == true ? "Enabled" : "Poll routes")
            LabeledContent("Click Routes", value: "\(model.eventSubscription?.clickRoutes.count ?? 0)")
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

    private var filteredProjectSummaries: [DesktopProjectSummary] {
        model.projectSummaries.filter { project in
            organizationFilter.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ||
                projectOrganization(project.repository).localizedCaseInsensitiveContains(organizationFilter)
        }
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

    private func projectDetailText(_ project: DesktopProjectSummary) -> String {
        let prText = project.pullRequests.count == 1 ? "1 PR" : "\(project.pullRequests.count) PRs"
        let sessionText = project.runningSessions.count == 1 ? "1 running agent" : "\(project.runningSessions.count) running agents"
        let workspaceText = project.workspaceNames.isEmpty ? "No workspace detected" : project.workspaceNames.joined(separator: ", ")
        return "\(prText) / \(sessionText) / \(workspaceText)"
    }

    private var accountStatusFooter: String {
        var parts: [String] = []
        if let fetched = model.accountFetchedAtEpochSeconds {
            parts.append("Fetched \(Date(timeIntervalSince1970: TimeInterval(fetched)).formatted(date: .abbreviated, time: .shortened))")
        }
        if model.accountStatusCacheHit {
            parts.append("cache hit")
        }
        return parts.joined(separator: " / ")
    }

    private func accountStateColor(_ state: String) -> Color {
        switch state {
        case "available":
            .green
        case "missing":
            .orange
        default:
            .secondary
        }
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

    private func workspaceSubtitle(_ workspace: DesktopWorkspace) -> String {
        [workspace.lastRole ?? workspace.defaultRole, workspace.defaultAgent, workspace.workdir]
            .compactMap { $0 }
            .joined(separator: " / ")
    }

    private func sessionSubtitle(_ session: DesktopSession) -> String {
        [session.workspace, session.role, session.agent, session.repository]
            .compactMap { $0 }
            .joined(separator: " / ")
    }

    private func sessionBranchText(_ session: DesktopSession) -> String {
        if let branch = session.branch, let pullRequest = session.linkedPullRequest {
            return "\(branch) / PR #\(pullRequest.number) \(pullRequest.title)"
        }
        if let branch = session.branch {
            return "\(branch) / No PR detected"
        }
        return "No branch detected"
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
