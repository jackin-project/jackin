import Foundation

@MainActor
final class StatusBarModel: ObservableObject {
    @Published private(set) var daemon: DaemonHealth = .checking
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
            "bolt.horizontal.circle"
        case .disconnected:
            "exclamationmark.circle"
        }
    }

    func refresh() async {
        isRefreshing = true
        defer { isRefreshing = false }

        do {
            let hello = try await client.hello()
            daemon = .connected(hello)
        } catch {
            daemon = .disconnected(error.localizedDescription)
        }
    }
}

enum DaemonHealth: Equatable {
    case checking
    case connected(DaemonHello)
    case disconnected(String)
}
