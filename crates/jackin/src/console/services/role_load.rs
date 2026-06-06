//! Non-TUI role loading side-effect services.

use futures_util::FutureExt as _;
use jackin_tui::runtime::BlockingSubscription;

pub(crate) fn start_role_registration(
    paths: crate::paths::JackinPaths,
    selector: crate::selector::RoleSelector,
    git_url: String,
) -> BlockingSubscription<anyhow::Result<()>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let mut runner = crate::docker::ShellRunner {
            debug: crate::tui::is_debug_mode(),
        };
        let result = register_with_runner(
            &paths,
            &selector,
            &git_url,
            &mut runner,
            crate::tui::is_debug_mode(),
        )
        .await;
        drop(tx.send(result));
    });
    rx
}

pub(crate) async fn register_with_runner(
    paths: &crate::paths::JackinPaths,
    selector: &crate::selector::RoleSelector,
    git_url: &str,
    runner: &mut impl crate::docker::CommandRunner,
    debug: bool,
) -> anyhow::Result<()> {
    std::panic::AssertUnwindSafe(async {
        crate::runtime::register_agent_repo(paths, selector, git_url, runner, debug).await?;
        Ok::<_, anyhow::Error>(())
    })
    .catch_unwind()
    .await
    .unwrap_or_else(|payload| {
        let panic_message = panic_payload_message(payload.as_ref());
        Err(anyhow::anyhow!("role loader panicked: {panic_message}"))
    })
}

fn panic_payload_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        return (*message).to_owned();
    }
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    "role loader panicked with a non-string payload".to_owned()
}
