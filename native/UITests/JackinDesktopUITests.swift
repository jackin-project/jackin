// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import XCTest

@MainActor
final class JackinDesktopUITests: XCTestCase {
    let application = XCUIApplication()

    struct PopoverAuditSnapshot {
        let limitDescriptions: [String]
        let systemHostVerified: Bool
        let controlsVerified: Bool
    }

    struct UsageAuditSnapshot {
        let rowDescriptions: [String]
        let systemHostVerified: Bool
        let controlsVerified: Bool
    }
}

extension XCUIElement {
    func position(relativeTo element: XCUIElement) -> CGPoint {
        CGPoint(x: frame.midX - element.frame.minX, y: frame.midY - element.frame.minY)
    }

    func waitForPosition(
        _ expectedPosition: CGPoint,
        relativeTo element: XCUIElement,
        accuracy: CGFloat,
        timeout: TimeInterval
    ) -> Bool {
        let predicate = NSPredicate { object, _ in
            guard let subject = object as? XCUIElement else { return false }
            let position = subject.position(relativeTo: element)
            return abs(position.x - expectedPosition.x) <= accuracy
                && abs(position.y - expectedPosition.y) <= accuracy
        }
        let expectation = XCTNSPredicateExpectation(predicate: predicate, object: self)
        return XCTWaiter.wait(for: [expectation], timeout: timeout) == .completed
    }

    func waitForHittable(timeout: TimeInterval) -> Bool {
        let predicate = NSPredicate(format: "isHittable == true")
        let expectation = XCTNSPredicateExpectation(predicate: predicate, object: self)
        return XCTWaiter.wait(for: [expectation], timeout: timeout) == .completed
    }

    func waitForLabelContaining(_ text: String, timeout: TimeInterval) -> Bool {
        let predicate = NSPredicate(format: "label CONTAINS %@", text)
        let expectation = XCTNSPredicateExpectation(predicate: predicate, object: self)
        return XCTWaiter.wait(for: [expectation], timeout: timeout) == .completed
    }

    func waitForValue(_ expectedValue: String, timeout: TimeInterval) -> Bool {
        let predicate = NSPredicate(format: "value == %@", expectedValue)
        let expectation = XCTNSPredicateExpectation(predicate: predicate, object: self)
        return XCTWaiter.wait(for: [expectation], timeout: timeout) == .completed
    }

    func waitForEnabled(timeout: TimeInterval) -> Bool {
        let predicate = NSPredicate(format: "isEnabled == true")
        let expectation = XCTNSPredicateExpectation(predicate: predicate, object: self)
        return XCTWaiter.wait(for: [expectation], timeout: timeout) == .completed
    }
}
