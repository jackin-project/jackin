// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import SwiftUI

/// Popover footer: exactly one Refresh button (⌘R). The spinner reflects
/// `refreshInProgress`. No other action, caption, or row.
struct PopoverFooter: View {
    let refreshInProgress: Bool
    let onRefresh: () -> Void

    var body: some View {
        Button(action: onRefresh) {
            HStack(spacing: 6) {
                if refreshInProgress {
                    ProgressView().controlSize(.small)
                } else {
                    Image(systemName: "arrow.clockwise")
                }
                Text("Refresh")
                Spacer()
                Text("⌘R").foregroundStyle(.secondary)
            }
            .contentShape(Rectangle())
            .padding(.horizontal, 8)
            .padding(.vertical, 6)
        }
        .buttonStyle(.plain)
        .keyboardShortcut("r", modifiers: [.command])
    }
}
