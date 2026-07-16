use super::super::EditorState;

impl<
    MountInfoCache,
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
> crate::tui::model::ConsoleEditorModalPresence
    for EditorState<
        MountInfoCache,
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >
{
    fn editor_modal_open(&self) -> bool {
        self.modal.is_some()
    }
}

impl<
    MountInfoCache,
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
> crate::tui::model::ConsoleAnimationTick
    for EditorState<
        MountInfoCache,
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >
where
    Modal: crate::tui::model::ConsoleAnimationTick,
{
    fn tick_active_animation(&mut self) -> bool {
        self.modal
            .as_mut()
            .is_some_and(crate::tui::model::ConsoleAnimationTick::tick_active_animation)
    }
}

impl<
    MountInfoCache,
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    OpRef,
> crate::tui::model::ConsolePendingOpCommit
    for EditorState<
        MountInfoCache,
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        crate::tui::subscriptions::PendingOpCommit<OpRef>,
    >
{
    type OpRef = OpRef;

    fn poll_pending_op_commit(&mut self) -> Option<(Self::OpRef, anyhow::Result<()>)> {
        use termrock::runtime::{Subscription, SubscriptionPoll};

        let pending = self.pending_op_commit.as_mut()?;
        let result = match pending.rx.poll_next() {
            SubscriptionPoll::Ready(result) => result,
            SubscriptionPoll::Pending => return None,
            SubscriptionPoll::Closed => Err(anyhow::anyhow!(
                crate::tui::subscriptions::op_read_worker_disconnected_message()
            )),
        };
        let pending = self.pending_op_commit.take()?;
        Some((pending.op_ref, result))
    }
}

impl<
    MountInfoCache,
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    DriftDetection,
    SavePlan,
    PendingIsolationCleanup,
    PendingOpCommit,
> crate::tui::model::ConsolePendingDriftCheck
    for EditorState<
        MountInfoCache,
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        crate::tui::subscriptions::PendingDriftCheck<DriftDetection, SavePlan>,
        PendingIsolationCleanup,
        PendingOpCommit,
    >
{
    type PendingDriftCheck = crate::tui::subscriptions::PendingDriftCheck<DriftDetection, SavePlan>;
    type DriftDetection = DriftDetection;

    fn poll_pending_drift_check(
        &mut self,
    ) -> Option<(
        Self::PendingDriftCheck,
        anyhow::Result<Self::DriftDetection>,
    )> {
        use termrock::runtime::{Subscription, SubscriptionPoll};

        let check = self.pending_drift_check.as_mut()?;
        let result = match check.rx.poll_next() {
            SubscriptionPoll::Ready(result) => result,
            SubscriptionPoll::Pending => return None,
            SubscriptionPoll::Closed => Err(anyhow::anyhow!(
                crate::tui::subscriptions::drift_check_worker_disconnected_message()
            )),
        };
        let check = self.pending_drift_check.take()?;
        Some((check, result))
    }
}

impl<
    MountInfoCache,
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    SavePlan,
    PendingOpCommit,
> crate::tui::model::ConsolePendingIsolationCleanup
    for EditorState<
        MountInfoCache,
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        crate::tui::subscriptions::PendingIsolationCleanup<SavePlan>,
        PendingOpCommit,
    >
{
    type PendingIsolationCleanup = crate::tui::subscriptions::PendingIsolationCleanup<SavePlan>;

    fn poll_pending_isolation_cleanup(
        &mut self,
    ) -> Option<(Self::PendingIsolationCleanup, anyhow::Result<()>)> {
        use termrock::runtime::{Subscription, SubscriptionPoll};

        let cleanup = self.pending_isolation_cleanup.as_mut()?;
        let result = match cleanup.rx.poll_next() {
            SubscriptionPoll::Ready(result) => result,
            SubscriptionPoll::Pending => return None,
            SubscriptionPoll::Closed => Err(anyhow::anyhow!(
                crate::tui::subscriptions::isolation_cleanup_worker_disconnected_message()
            )),
        };
        let cleanup = self.pending_isolation_cleanup.take()?;
        Some((cleanup, result))
    }
}

impl<
    MountInfoCache,
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    RoleSource,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
> crate::tui::model::ConsolePendingRoleLoad
    for EditorState<
        MountInfoCache,
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        crate::tui::subscriptions::PendingRoleLoad<RoleSource>,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >
{
    type PendingRoleLoad = crate::tui::subscriptions::PendingRoleLoad<RoleSource>;

    fn poll_pending_role_load(&mut self) -> Option<(Self::PendingRoleLoad, anyhow::Result<()>)> {
        use termrock::runtime::{Subscription, SubscriptionPoll};

        let load = self.pending_role_load.as_mut()?;
        let result = match load.rx.poll_next() {
            SubscriptionPoll::Ready(result) => result,
            SubscriptionPoll::Pending => return None,
            SubscriptionPoll::Closed => Err(anyhow::anyhow!(
                crate::tui::subscriptions::role_loader_worker_disconnected_message()
            )),
        };
        let load = self.pending_role_load.take()?;
        Some((load, result))
    }
}

impl<
    MountInfoCache,
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
> crate::tui::model::ConsolePendingTokenGenerate
    for EditorState<
        MountInfoCache,
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >
{
    type PendingTokenGenerate = PendingTokenGenerate;

    fn take_pending_token_generate(&mut self) -> Option<Self::PendingTokenGenerate> {
        self.pending_token_generate.take()
    }
}

impl<
    MountInfoCache,
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
> crate::tui::model::ConsoleEditorFooterHeight
    for EditorState<
        MountInfoCache,
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >
{
    fn editor_cached_footer_height(&self) -> u16 {
        self.cached_footer_h
    }
}
