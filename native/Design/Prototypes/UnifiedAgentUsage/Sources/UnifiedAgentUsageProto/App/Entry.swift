import AppKit

@main
enum UnifiedAgentUsageProtoMain {
    static func main() {
        let config = ProtoConfig.parse(CommandLine.arguments)
        // Unknown scenario names (including the F18/F19/F24 matrix headings)
        // fail loudly here, before any window exists.
        let projection = ProtoFixtures.load(config.scenario)
        let store = ProtoStore(projection: projection)
        let app = NSApplication.shared
        let delegate = ProtoDelegate(config: config)
        let shell = ProtoShell(store: store, config: config)
        delegate.onLaunch = { app in
            shell.install(into: app)
        }
        withExtendedLifetime(shell) {
            app.delegate = delegate
            app.setActivationPolicy(.regular)
            app.run()
        }
    }
}
