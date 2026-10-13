// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `ManagerState` refresh and poll operations.

use crate::tui::runtime::BlockingSubscription;
use crate::tui::runtime::SubscriptionPoll;

use crate::tui::subscriptions::{InstanceRefreshThrottleState, instance_refresh_throttle_plan};

use super::super::{
    ManagerConfigSaveResult, ManagerInstanceRefreshSnapshot, ManagerState,
    PendingFileBrowserCommit, PendingFileBrowserListing, PendingMountInfoRefresh,
};

impl ManagerState<'_> {
    pub fn poll_instance_refresh(
        &mut self,
    ) -> Option<Result<ManagerInstanceRefreshSnapshot, String>> {
        self.drain_instance_refresh()
    }

    pub fn next_instance_refresh_generation_if_due(&mut self) -> Option<u64> {
        let now = std::time::Instant::now();
        let plan = instance_refresh_throttle_plan(
            InstanceRefreshThrottleState {
                in_flight: self.instances_refresh_rx.is_some(),
                last_refresh: self.instances_last_refresh,
                interval: self.instances_refresh_interval,
                generation: self.instances_refresh_generation,
            },
            now,
        );
        self.instances_last_refresh = plan.last_refresh;
        self.instances_refresh_generation = plan.generation;
        plan.start_generation
    }

    pub const fn instance_refresh_in_flight(&self) -> bool {
        self.instances_refresh_rx.is_some()
    }

    pub fn begin_instance_refresh(
        &mut self,
        rx: BlockingSubscription<(u64, Result<ManagerInstanceRefreshSnapshot, String>)>,
    ) {
        self.instances_refresh_rx = Some(rx);
    }

    pub const fn mount_info_refresh_in_flight(&self) -> bool {
        self.mount_info_refresh_rx.is_some()
    }

    pub fn begin_mount_info_refresh(&mut self, rx: BlockingSubscription<PendingMountInfoRefresh>) {
        self.mount_info_refresh_rx = Some(rx);
    }

    pub fn begin_file_browser_listing(
        &mut self,
        rx: BlockingSubscription<PendingFileBrowserListing>,
    ) {
        self.file_browser_listing_rx = Some(rx);
    }

    pub const fn file_browser_listing_in_flight(&self) -> bool {
        self.file_browser_listing_rx.is_some()
    }

    pub fn begin_file_browser_commit(
        &mut self,
        rx: BlockingSubscription<PendingFileBrowserCommit>,
    ) {
        self.file_browser_commit_rx = Some(rx);
    }

    pub const fn file_browser_commit_in_flight(&self) -> bool {
        self.file_browser_commit_rx.is_some()
    }

    pub fn begin_config_save(&mut self, rx: BlockingSubscription<ManagerConfigSaveResult>) {
        self.config_save_rx = Some(rx);
    }

    pub const fn config_save_in_flight(&self) -> bool {
        self.config_save_rx.is_some()
    }

    pub fn begin_account_scan(
        &mut self,
        rx: BlockingSubscription<(
            u64,
            Result<crate::tui::screens::settings::model::AccountScanOutcome, String>,
        )>,
    ) {
        self.account_scan_rx = Some(rx);
    }

    pub const fn account_scan_in_flight(&self) -> bool {
        self.account_scan_rx.is_some()
    }

    pub fn poll_account_scan(
        &mut self,
    ) -> Option<(
        u64,
        Result<crate::tui::screens::settings::model::AccountScanOutcome, String>,
    )> {
        let rx = self.account_scan_rx.as_mut()?;
        let result = match rx.poll_next() {
            SubscriptionPoll::Ready(result) => result,
            SubscriptionPoll::Pending => return None,
            SubscriptionPoll::Closed => {
                self.account_scan_rx = None;
                return None;
            }
        };
        self.account_scan_rx = None;
        Some(result)
    }

    pub fn poll_mount_info_refresh(&mut self) -> Option<PendingMountInfoRefresh> {
        let rx = self.mount_info_refresh_rx.as_mut()?;
        let result = match rx.poll_next() {
            SubscriptionPoll::Ready(result) => result,
            SubscriptionPoll::Pending => return None,
            SubscriptionPoll::Closed => {
                self.mount_info_refresh_rx = None;
                return None;
            }
        };
        self.mount_info_refresh_rx = None;
        Some(result)
    }

    pub fn poll_file_browser_listing(&mut self) -> Option<PendingFileBrowserListing> {
        let rx = self.file_browser_listing_rx.as_mut()?;
        let result = match rx.poll_next() {
            SubscriptionPoll::Ready(result) => result,
            SubscriptionPoll::Pending => return None,
            SubscriptionPoll::Closed => {
                self.file_browser_listing_rx = None;
                return None;
            }
        };
        self.file_browser_listing_rx = None;
        Some(result)
    }

    pub fn poll_file_browser_commit(&mut self) -> Option<PendingFileBrowserCommit> {
        let rx = self.file_browser_commit_rx.as_mut()?;
        let result = match rx.poll_next() {
            SubscriptionPoll::Ready(result) => result,
            SubscriptionPoll::Pending => return None,
            SubscriptionPoll::Closed => {
                self.file_browser_commit_rx = None;
                return None;
            }
        };
        self.file_browser_commit_rx = None;
        Some(result)
    }

    pub fn poll_config_save(&mut self) -> Option<ManagerConfigSaveResult> {
        let rx = self.config_save_rx.as_mut()?;
        let result = match rx.poll_next() {
            SubscriptionPoll::Ready(result) => result,
            SubscriptionPoll::Pending => return None,
            SubscriptionPoll::Closed => {
                self.config_save_rx = None;
                return Some(ManagerConfigSaveResult::Settings(Err(anyhow::anyhow!(
                    crate::tui::subscriptions::config_save_worker_disconnected_message()
                ))));
            }
        };
        self.config_save_rx = None;
        Some(result)
    }
}
