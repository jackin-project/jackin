import Foundation

@MainActor
final class StatusBarModel: ObservableObject {
    @Published private(set) var daemon: DaemonHealth = .checking
    @Published private(set) var workspaces: [DesktopWorkspace] = []
    @Published private(set) var sessions: [DesktopSession] = []
    @Published private(set) var isRefreshing = false

    private let client: DaemonClient

    init(client: DaemonClient = DaemonClient()) {
        self.client = client
    }

    var symbolName: String {
        switch daemon {
        case .checking:
            "circle.dotted"
        case .connected:
            sessions.isEmpty ? "bolt.horizontal.circle" : "bolt.horizontal.circle.fill"
        case .disconnected:
            "exclamationmark.circle"
        }
    }

    func refresh() async {
        isRefreshing = true
        defer { isRefreshing = false }

        do {
            let hello = try await client.hello()
            async let workspaceList = client.workspaces()
            async let sessionList = client.sessions()
            workspaces = try await workspaceList.workspaces
            sessions = try await sessionList.sessions
            daemon = .connected(hello)
        } catch {
            workspaces = []
            sessions = []
            daemon = .disconnected(error.localizedDescription)
        }
    }

    func runningCount(for workspace: DesktopWorkspace) -> Int {
        sessions.filter { $0.workspace == workspace.name }.count
    }
}

enum DaemonHealth: Equatable {
    case checking
    case connected(DaemonHello)
    case disconnected(String)
}
