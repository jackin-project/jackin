import AppKit

@main
enum UnifiedAgentUsageProtoMain {
    static func main() {
        let config = ProtoConfig.parse(CommandLine.arguments)


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
