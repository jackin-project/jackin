// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import JackinUsageBindings

protocol UsageSelectionClient: Sendable {
    func setSelectedAccount(surfaceId: String, accountKey: String) async throws
    func desktopProjection(statusBarMax: UInt32) async throws -> DesktopProjectionDto
}

extension RefreshScheduler: UsageSelectionClient {}
