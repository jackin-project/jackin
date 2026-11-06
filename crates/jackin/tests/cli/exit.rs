// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin::{CommandError, ExitClass};
use jackin_core::{AccountName, ConfigError, GatewayName, ProcessError, Program};
use jackin_image::{EngineError, ImageError};
use jackin_openshell::{GatewayError, GatewayOp};
use jackin_session::{
    AccountEditError, AccountError, AttachError, ConnectError, ExportError, LifecycleError,
    LockError, PlanError, RoleError, WorkspaceGuardError,
};
use jackin_tui::ConsoleError;

#[test]
fn every_exit_class_has_its_documented_number() {
    let table = [
        (ExitClass::Ok, 0_u8),
        (ExitClass::Failure, 1),
        (ExitClass::Usage, 2),
        (ExitClass::Refused, 3),
        (ExitClass::Cancelled, 130),
    ];
    for (class, code) in table {
        assert_eq!(class.code(), code, "{class:?}");
    }
}

#[test]
fn command_errors_preserve_cancellation_and_failure_classes()
-> Result<(), Box<dyn std::error::Error>> {
    let gateway = GatewayName::parse("work")?;
    let account = AccountName::parse("work")?;
    let cancelled = || GatewayError::Cancelled {
        op: GatewayOp::ReadFiles,
        group: None,
    };
    let cases = [
        (
            "attach cancellation",
            CommandError::Attach(AttachError::Gateway(cancelled())),
            ExitClass::Cancelled,
        ),
        (
            "config cancellation",
            CommandError::Config(ConfigError::Cancelled),
            ExitClass::Cancelled,
        ),
        (
            "plan config cancellation",
            CommandError::Plan(PlanError::Config(ConfigError::Cancelled)),
            ExitClass::Cancelled,
        ),
        (
            "console config cancellation",
            CommandError::Console(ConsoleError::Config(ConfigError::Cancelled)),
            ExitClass::Cancelled,
        ),
        (
            "workspace lock cancellation",
            CommandError::WorkspaceInUse(WorkspaceGuardError::Lock(LockError::Cancelled)),
            ExitClass::Cancelled,
        ),
        (
            "workspace listing cancellation",
            CommandError::WorkspaceInUse(WorkspaceGuardError::Listing {
                gateway: gateway.clone(),
                source: LifecycleError::Cancelled,
            }),
            ExitClass::Cancelled,
        ),
        (
            "account edit gateway cancellation",
            CommandError::Accounts(AccountEditError::OnGateway {
                gateway: gateway.clone(),
                source: cancelled(),
            }),
            ExitClass::Cancelled,
        ),
        (
            "account gateway cancellation",
            CommandError::Account(AccountError::Gateway(cancelled())),
            ExitClass::Cancelled,
        ),
        (
            "connection cancellation",
            CommandError::Connect(ConnectError::Gateway(cancelled())),
            ExitClass::Cancelled,
        ),
        (
            "role cancellation",
            CommandError::RoleFindings(vec![RoleError::Cancelled]),
            ExitClass::Cancelled,
        ),
        (
            "image engine cancellation",
            CommandError::Image(ImageError::Engine(EngineError::Cancelled { group: None })),
            ExitClass::Cancelled,
        ),
        (
            "export process cancellation",
            CommandError::Export(ExportError::Process(ProcessError::Cancelled {
                program: Program::Git,
                group: None,
            })),
            ExitClass::Cancelled,
        ),
        (
            "attach gateway failure",
            CommandError::Attach(AttachError::Gateway(GatewayError::Unauthenticated)),
            ExitClass::Failure,
        ),
        (
            "workspace listing failure",
            CommandError::WorkspaceInUse(WorkspaceGuardError::Listing {
                gateway: gateway.clone(),
                source: LifecycleError::Gateway(GatewayError::Unauthenticated),
            }),
            ExitClass::Failure,
        ),
        (
            "mixed role findings",
            CommandError::RoleFindings(vec![RoleError::Cancelled, RoleError::NoRole]),
            ExitClass::Failure,
        ),
        (
            "invalid role",
            CommandError::RoleFindings(vec![RoleError::NoRole]),
            ExitClass::Failure,
        ),
        (
            "empty findings",
            CommandError::RoleFindings(vec![]),
            ExitClass::Failure,
        ),
        (
            "account rollback failure",
            CommandError::Account(AccountError::RollbackFailed {
                account: account.clone(),
                original: Box::new(AccountError::Lock(LockError::Cancelled)),
                rollback: Box::new(AccountError::Gateway(GatewayError::Unauthenticated)),
            }),
            ExitClass::Failure,
        ),
        (
            "account edit rollback failure",
            CommandError::Accounts(AccountEditError::RollbackFailed {
                account,
                gateway,
                original: Box::new(AccountEditError::Config(ConfigError::Cancelled)),
                rollback: AccountError::Gateway(GatewayError::Unauthenticated),
            }),
            ExitClass::Failure,
        ),
    ];
    let mismatches: Vec<_> = cases
        .into_iter()
        .filter_map(|(label, error, expected)| {
            let actual = error.class();
            (actual != expected).then(|| format!("{label}: expected {expected:?}, got {actual:?}"))
        })
        .collect();
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
    Ok(())
}
