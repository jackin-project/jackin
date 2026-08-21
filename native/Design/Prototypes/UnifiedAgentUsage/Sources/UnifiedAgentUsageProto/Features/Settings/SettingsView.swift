import SwiftUI

struct SettingsView: View {
    @Bindable var store: ProtoStore
    @State private var selection = SettingsSection.menuBar

    private enum SettingsSection: String, CaseIterable, Identifiable {
        case menuBar = "Menu Bar"
        case providers = "Providers"
        case refresh = "Refresh"
        case general = "General"

        var id: Self { self }
        var symbol: String {
            switch self {
            case .menuBar: "menubar.rectangle"
            case .providers: "square.stack.3d.up"
            case .refresh: "arrow.clockwise"
            case .general: "gearshape"
            }
        }
    }

    private var percentBinding: Binding<PercentStyle> {
        Binding(
            get: { store.percentStyle },
            set: { store.setPercentStyle($0) })
    }

    private var floorBinding: Binding<Double> {
        Binding(
            get: { Double(store.refreshFloorMinutes) },
            set: { store.requestRefreshFloor(Int($0)) })
    }

    var body: some View {
        NavigationSplitView {
            List(SettingsSection.allCases, selection: $selection) { section in
                Label(section.rawValue, systemImage: section.symbol)
                    .tag(section)
            }
            .listStyle(.sidebar)
            .navigationSplitViewColumnWidth(min: 150, ideal: 170, max: 210)
        } detail: {
            Form {
                settingsContent
            }
            .formStyle(.grouped)
            .scrollContentBackground(.hidden)
            .background(JackinBrand.stage)
            .navigationTitle(selection.rawValue)
        }
    }

    @ViewBuilder
    private var settingsContent: some View {
        switch selection {
        case .menuBar:
            Section("Presentation") {
                Picker("Display", selection: $store.displayMode) {
                    Text("All providers (icon + remaining %)").tag(ProtoStore.DisplayMode.strip)
                    Text("Worst provider only").tag(ProtoStore.DisplayMode.focusPercent)
                    Text("Pinned provider").tag(ProtoStore.DisplayMode.pinnedSurface)
                    Text("Icon only").tag(ProtoStore.DisplayMode.iconOnly)
                }
                .pickerStyle(.radioGroup)
                .accessibilityLabel("Status item display mode")
                if store.displayMode == .strip {
                    Text(
                        "Detected providers use native menu-bar items with system-owned appearance."
                    )
                    .font(.caption)
                    .foregroundStyle(JackinBrand.muted)
                }

                if store.displayMode == .pinnedSurface {
                    Picker("Pinned provider", selection: $store.pinnedSurfaceKey) {
                        Text("—").tag("")
                        ForEach(store.projection.providers) { provider in
                            Text(provider.name).tag(provider.key)
                        }
                    }
                    .accessibilityLabel("Pinned provider for status item")
                }

                if store.displayMode == .strip {
                    Picker("Max providers in menu bar", selection: $store.stripMax) {
                        ForEach(1...3, id: \.self) { count in
                            Text("\(count)").tag(count)
                        }
                    }
                    .accessibilityLabel("Maximum providers shown in menu bar strip (1–3)")
                }

                Picker("Percent style", selection: percentBinding) {
                    Text("% left (remaining)").tag(PercentStyle.left)
                    Text("% used").tag(PercentStyle.used)
                }
                .pickerStyle(.radioGroup)
                .accessibilityLabel("Percent format: remaining left or used")
                Text("Menu bar chips and compact labels use this style together.")
                    .font(.caption)
                    .foregroundStyle(JackinBrand.muted)

                Picker("Reset style", selection: $store.resetStyle) {
                    Text("Countdown").tag(ProtoStore.ResetStyle.countdown)
                    Text("Exact time").tag(ProtoStore.ResetStyle.exactClock)
                }
                .pickerStyle(.radioGroup)
                .accessibilityLabel("Reset time format")

                Toggle(
                    "Hide values while screen sharing",
                    isOn: $store.hideWhileScreenSharing
                )
                .accessibilityLabel("Hide values while screen sharing")
            }
        case .providers:
            Section("Usage surfaces") {
                ForEach(store.projection.providers) { provider in
                    Toggle(
                        isOn: Binding(
                            get: { store.surfaceEnabled[provider.key] ?? true },
                            set: { store.surfaceEnabled[provider.key] = $0 }
                        )
                    ) {
                        Label {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(provider.name)
                                Text(
                                    provider.state.label
                                        ?? provider.accounts.first?.auth
                                        ?? "Ready"
                                )
                                .font(.caption)
                                .foregroundStyle(JackinBrand.muted)
                            }
                        } icon: {
                            if let mark = ProviderMarks.swiftUIImage(forIconKey: provider.iconKey) {
                                mark
                                    .resizable()
                                    .scaledToFit()
                                    .frame(width: 16, height: 16)
                            }
                        }
                    }
                    .accessibilityLabel("\(provider.name) enabled")
                }
            }
        case .refresh:
            Section("Provider polling") {
                Slider(
                    value: floorBinding,
                    in: 1...30,
                    step: 1
                ) {
                    Text("Minimum interval")
                } minimumValueLabel: {
                    Text("1m")
                } maximumValueLabel: {
                    Text("30m")
                }
                Text("Probe at most every \(store.refreshFloorMinutes) minutes (Rust floor).")
                    .font(.caption)
                    .foregroundStyle(JackinBrand.muted)
                    .accessibilityLabel("Refresh floor \(store.refreshFloorMinutes) minutes")
                if let error = store.floorError {
                    Label(error, systemImage: "exclamationmark.triangle")
                        .font(.caption)
                        .foregroundStyle(JackinBrand.warning)
                        .fixedSize(horizontal: false, vertical: true)
                    Button(store.chrome.retryTitle) {
                        store.retryRefreshFloor()
                    }
                    .accessibilityIdentifier("settings.floor-retry")
                }
            }
        case .general:
            Section("Startup") {
                Toggle("Launch at login", isOn: $store.launchAtLogin)
                    .accessibilityLabel("Launch at login")
            }
            Section("Privacy and architecture") {
                Text("Account quotas from host credentials via jackin-usage (Rust).")
                    .font(.caption)
                Text(
                    "Refreshing here updates the same account snapshot every jackin❯ container reads (and vice versa)."
                )
                .font(.caption)
                .foregroundStyle(JackinBrand.muted)
                Text("No passwords stored. No Capsule required.")
                    .font(.caption)
                    .foregroundStyle(JackinBrand.muted)
            }
        }
    }
}
