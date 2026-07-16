// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! GitHub context view types, `PullRequestStatus`, construction and state helpers
//! extracted from the dialog coordinator (types + the two Dialog methods).
//! Free types re-exported from parent to preserve `super::*` / `dialog::` sites
//! in tests and other call sites.

use crate::pull_request::PullRequestInfo;

/// Borrowed snapshot of multiplexer PR state, so `GitHubContext`
/// rendering and dispatch stay live without copying the data into
/// the dialog variant.
#[derive(Debug, Clone, Copy)]
pub struct GithubContextView<'a> {
    pub branch: Option<&'a str>,
    pub status: PullRequestStatus<'a>,
}

pub fn github_context_view_from_state<'a>(
    branch: Option<&'a str>,
    pull_request: Option<&'a PullRequestInfo>,
    loading: bool,
) -> GithubContextView<'a> {
    let status = match pull_request {
        Some(pr) => PullRequestStatus::Loaded(pr),
        None if loading => PullRequestStatus::Resolving,
        None => PullRequestStatus::Idle,
    };
    GithubContextView { branch, status }
}

/// Resolution state of the multiplexer's PR lookup. Mirrors the
/// daemon's `(in_flight, Option<PullRequestInfo>)` pair but rules
/// out the impossible `Loaded + Resolving` combination at the type
/// level — keeps every dialog branch a single exhaustive match.
#[derive(Debug, Clone, Copy)]
pub enum PullRequestStatus<'a> {
    Loaded(&'a PullRequestInfo),
    Resolving,
    Idle,
}

impl<'a> PullRequestStatus<'a> {
    pub fn loaded(&self) -> Option<&'a PullRequestInfo> {
        match self {
            Self::Loaded(pr) => Some(*pr),
            _ => None,
        }
    }
}

use super::Dialog;

impl Dialog {
    pub(crate) fn github_context_state(
        &self,
        github: Option<&GithubContextView<'_>>,
    ) -> Option<crate::tui::components::container_info_surface::ContainerInfoState> {
        let Self::GitHubContext { copied, scroll } = self else {
            return None;
        };
        let branch = github
            .and_then(|view| view.branch)
            .map_or_else(|| "(unknown)".to_owned(), str::to_owned);
        let loading_placeholder =
            if github.is_some_and(|view| matches!(view.status, PullRequestStatus::Resolving)) {
                "resolving…"
            } else {
                "(none)"
            };
        let pr = github.and_then(|view| view.status.loaded());
        let pr_number = pr.map_or_else(
            || loading_placeholder.to_owned(),
            PullRequestInfo::number_label,
        );
        let pr_title = pr.map_or_else(|| loading_placeholder.to_owned(), |p| p.title.clone());
        let pr_url = pr.map_or_else(|| loading_placeholder.to_owned(), |p| p.url.clone());
        let ci = pr.and_then(|p| p.checks.as_ref()).map_or_else(
            || {
                if github.is_some_and(|view| matches!(view.status, PullRequestStatus::Resolving)) {
                    "resolving…"
                } else {
                    "(unknown)"
                }
                .to_owned()
            },
            crate::pull_request::PullRequestChecks::summary,
        );
        let mut rows = vec![
            crate::tui::components::container_info_surface::ContainerInfoRow::new("Branch", branch),
            crate::tui::components::container_info_surface::ContainerInfoRow::new(
                "Pull Request",
                pr_number,
            ),
            crate::tui::components::container_info_surface::ContainerInfoRow::new(
                "PR Title", pr_title,
            ),
        ];
        let mut url_row = crate::tui::components::container_info_surface::ContainerInfoRow::new(
            "GitHub URL",
            pr_url,
        );
        if let Some(pr) = pr {
            url_row = url_row.copyable().hyperlink(pr.url.clone());
        }
        rows.extend([
            url_row,
            crate::tui::components::container_info_surface::ContainerInfoRow::new("CI Status", ci),
        ]);
        if let Some(pr) = pr {
            rows.push(
                crate::tui::components::container_info_surface::ContainerInfoRow::new(
                    "Open PR",
                    pr.url.clone(),
                )
                .hyperlink(pr.url.clone()),
            );
            let ci_url = pr
                .checks
                .as_ref()
                .and_then(crate::pull_request::PullRequestChecks::ci_url);
            let mut ci_row = crate::tui::components::container_info_surface::ContainerInfoRow::new(
                "Open CI",
                ci_url.unwrap_or("(unavailable)"),
            );
            if let Some(ci_url) = ci_url {
                ci_row = ci_row.hyperlink(ci_url.to_owned());
            }
            rows.push(ci_row);
        }
        let mut state = crate::tui::components::container_info_surface::ContainerInfoState::new(
            "GitHub context",
            rows,
        );
        if *copied {
            state.mark_copied(super::GITHUB_URL_ROW);
        }
        state.scroll = scroll.clone();
        Some(state)
    }

    pub fn new_github_context() -> Self {
        Self::GitHubContext {
            copied: false,
            scroll: termrock::layout::DialogBodyScroll::new(),
        }
    }
}
