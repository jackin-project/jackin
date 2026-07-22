// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import JackinUsageBridge
import ServiceManagement
import SwiftUI

struct SettingsView: View {
    @ObservedObject var store: PresentationStore
    @State private var floorMinutes: Double = 5
    @State private var launchAtLogin: Bool = false
    @State private var launchAtLoginNote: String?

    var body: some View {
        Form {
            Section("Menu bar") {
                Picker("Display", selection: $store.displayMode) {
                    Text("All providers (icon + remaining %)").tag(StatusItemDisplayMode.strip)
                    Text("Worst provider only").tag(StatusItemDisplayMode.focusPercent)
                    Text("Pinned provider").tag(StatusItemDisplayMode.pinnedSurface)
                    Text("Icon only").tag(StatusItemDisplayMode.iconOnly)
                }
                .pickerStyle(.radioGroup)
                .accessibilityLabel("Status item display mode")
                if store.displayMode == .strip {
                    Text("OpenUsage-style strip with Liquid Glass chip capsules on macOS 26+.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }

                if store.displayMode == .pinnedSurface {
                    Picker("Pinned provider", selection: $store.pinnedSurfaceId) {
                        Text("—").tag("")
                        ForEach(store.surfaces) { surface in
                            Text(surface.label).tag(surface.id)
                        }
                    }
                    .accessibilityLabel("Pinned provider for status item")
                }

                if store.displayMode == .strip {
                    Picker("Max providers in menu bar", selection: $store.stripMax) {
                        ForEach(1...8, id: \.self) { n in
                            Text(n == 8 ? "8 (all)" : "\(n)").tag(n)
                        }
                    }
                    .accessibilityLabel("Maximum providers shown in menu bar strip")
                }

                Picker("Percent style", selection: $store.percentStyle) {
                    Text("% left (remaining)").tag("left")
                    Text("% used").tag("used")
                }
                .pickerStyle(.radioGroup)
                .accessibilityLabel("Percent format: remaining left or used")
                Text("Menu bar chips and compact labels use this style together.")
                    .font(.caption)
                    .foregroundStyle(.secondary)

                Picker("Reset style", selection: $store.resetStyle) {
                    Text("Countdown").tag("countdown")
                    Text("Exact time").tag("exact_clock")
                }
                .pickerStyle(.radioGroup)
                .accessibilityLabel("Reset time format")

                Toggle("Hide values while screen sharing", isOn: $store.hideWhileScreenSharing)
                    .accessibilityLabel("Hide values while screen sharing")
            }
            Section("Login") {
                Toggle("Launch at login", isOn: $launchAtLogin)
                    .onChange(of: launchAtLogin) { _, newValue in
                        applyLaunchAtLogin(newValue)
                    }
                    .accessibilityLabel("Launch at login")
                if let note = launchAtLoginNote {
                    Text(note)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            Section("Surfaces") {
                ForEach(store.surfaces) { surface in
                    Toggle(surface.label, isOn: Binding(
                        get: { surface.enabled },
                        set: { store.setEnabled(surfaceId: surface.id, enabled: $0) }
                    ))
                    .accessibilityLabel("\(surface.label) enabled")
                }
            }
            Section("Refresh") {
                // Policy floor lives in Rust (clamped ≥ 60s); UI only projects minutes.
                Slider(
                    value: $floorMinutes,
                    in: 1...30,
                    step: 1
                ) {
                    Text("Minimum interval")
                } minimumValueLabel: {
                    Text("1m")
                } maximumValueLabel: {
                    Text("30m")
                }
                .onChange(of: floorMinutes) { _, newValue in
                    store.setRefreshFloorSecs(UInt64(newValue) * 60)
                }
                Text("Probe at most every \(Int(floorMinutes)) minutes (Rust floor).")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .accessibilityLabel("Refresh floor \(Int(floorMinutes)) minutes")
            }
            Section("About") {
                Text("Account quotas from host credentials via jackin-usage (Rust).")
                    .font(.caption)
                Text("Refreshing here updates the same account snapshot every jackin❯ container reads (and vice versa).")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Text("No passwords stored. No Capsule required.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
        .frame(width: 420, height: 640)
        .onAppear {
            if !store.isOpen {
                store.openDefault()
            }
            floorMinutes = Double(max(store.refreshFloorSecs, 60)) / 60.0
            // Always read truth from SMAppService — never cache as sole source.
            launchAtLogin = SMAppService.mainApp.status == .enabled
            launchAtLoginNote = statusNote(SMAppService.mainApp.status)
        }
    }

    private func applyLaunchAtLogin(_ enabled: Bool) {
        do {
            if enabled {
                try SMAppService.mainApp.register()
            } else {
                try SMAppService.mainApp.unregister()
            }
            launchAtLogin = SMAppService.mainApp.status == .enabled
            launchAtLoginNote = statusNote(SMAppService.mainApp.status)
        } catch {
            launchAtLogin = SMAppService.mainApp.status == .enabled
            launchAtLoginNote = "Could not update login item: \(error.localizedDescription)"
        }
    }

    private func statusNote(_ status: SMAppService.Status) -> String? {
        switch status {
        case .requiresApproval:
            return "Approval needed in System Settings → General → Login Items."
        case .notFound:
            return "Login item not registered for this build (use a signed app bundle)."
        default:
            return nil
        }
    }
}
