//! Keyboard + mouse event handling for the file browser.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;

use super::git_prompt::{GitPromptFocus, git_prompt_url_row_rect, resolve_git_url};
use super::state::{FileBrowserState, canonicalize_or_self, is_within_root};
use crate::launch::widgets::ModalOutcome;

impl FileBrowserState {
    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<PathBuf> {
        // Git-repo prompt has its own key map; delegate before clearing
        // any state the main handler would otherwise reset.
        if self.pending_git_prompt.is_some() {
            return self.handle_git_prompt_key(key);
        }

        // Clear any stale rejection on the next keypress.
        self.rejected_reason = None;

        match key.code {
            KeyCode::Up | KeyCode::Char('k' | 'K') => {
                self.select_prev();
                ModalOutcome::Continue
            }
            KeyCode::Down | KeyCode::Char('j' | 'J') => {
                self.select_next();
                ModalOutcome::Continue
            }
            KeyCode::Left | KeyCode::Char('h' | 'H') => {
                self.navigate_up();
                ModalOutcome::Continue
            }
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l' | 'L') => self.handle_enter(),
            KeyCode::Char('s' | 'S') => {
                // Prefer the highlighted entry (so the operator can pick a
                // sibling without first Entering). Fall back to cwd when
                // there's no real selection (empty listing, `..` row).
                let target = self
                    .highlighted()
                    .filter(|e| !e.is_parent)
                    .map_or_else(|| self.cwd.clone(), |e| e.path.clone());
                self.commit_or_reject(target)
            }
            KeyCode::Esc => {
                // Esc steps back one directory when the operator has
                // drilled below root — mirroring `h` / `←`. Only cancels
                // the modal when already at root. `rejected_reason` was
                // cleared above; preserve that in both branches.
                if self.cwd == self.root {
                    ModalOutcome::Cancel
                } else {
                    self.navigate_up();
                    ModalOutcome::Continue
                }
            }
            _ => ModalOutcome::Continue,
        }
    }

    /// Shared path for Enter / l / → on a highlighted entry: navigate
    /// into dirs, open the git-repo prompt on repo rows, up on the `..` row.
    pub(super) fn handle_enter(&mut self) -> ModalOutcome<PathBuf> {
        let Some(entry) = self.highlighted().cloned() else {
            return ModalOutcome::Continue;
        };
        if entry.is_parent {
            self.navigate_up();
            return ModalOutcome::Continue;
        }
        if entry.is_git {
            self.pending_git_url = resolve_git_url(&entry.path);
            self.pending_git_prompt = Some(entry.path);
            self.pending_git_focus = GitPromptFocus::MountHere;
            return ModalOutcome::Continue;
        }
        // Plain folder — navigate in. Canonicalize so a legitimate
        // symlinked-dir-inside-root still resolves to its real path;
        // out-of-root symlinks were already filtered by `load_entries`,
        // but this belt-and-suspenders guards against races.
        if is_within_root(&entry.path, &self.root) && entry.path.is_dir() {
            self.cwd = canonicalize_or_self(entry.path);
            self.reload();
        }
        ModalOutcome::Continue
    }

    /// Shared commit-or-reject logic used by `s` and the git-repo prompt's
    /// "Mount this repository" option. Enforces the same sandbox rules.
    pub(super) fn commit_or_reject(&mut self, target: PathBuf) -> ModalOutcome<PathBuf> {
        // Sandbox: belt-and-suspenders check that `target`'s canonical
        // form is inside `root`. Upstream steps (load_entries, set_cwd,
        // handle_enter) already filter escaping symlinks, but a TOCTOU
        // race between listing and commit could still slip one through.
        if !is_within_root(&target, &self.root) {
            self.rejected_reason = Some("Cannot commit a path outside $HOME.".into());
            return ModalOutcome::Continue;
        }
        // Reject root itself — user must navigate into a subfolder.
        if target == self.root {
            self.rejected_reason =
                Some("Cannot use $HOME itself — navigate into a subfolder.".into());
            return ModalOutcome::Continue;
        }
        // Reject jackin's own data directory.
        let jackin_data = self.root.join(".jackin");
        if target.starts_with(&jackin_data) {
            self.rejected_reason =
                Some("Cannot use ~/.jackin/* — those paths are reserved.".into());
            return ModalOutcome::Continue;
        }
        ModalOutcome::Commit(target)
    }

    /// Handle a left-click at `(column, row)` in absolute terminal
    /// coordinates while `modal_area` hosts this browser. Returns `true`
    /// iff the click hit the git-prompt's URL row AND `pending_git_url`
    /// is resolved — in which case `open::that_detached` has been fired
    /// best-effort (errors are swallowed; see the `O` hotkey handler for
    /// the parallel rationale). A `true` return doesn't dismiss the
    /// prompt, matching the keyboard keypath.
    pub fn maybe_open_url_on_click(&self, modal_area: Rect, column: u16, row: u16) -> bool {
        if self.pending_git_prompt.is_none() {
            return false;
        }
        let Some(url) = self.pending_git_url.as_deref() else {
            return false;
        };
        let has_rejection = self.rejected_reason.is_some();
        let Some(url_rect) = git_prompt_url_row_rect(modal_area, has_rejection) else {
            return false;
        };
        if column < url_rect.x || column >= url_rect.x + url_rect.width || row != url_rect.y {
            return false;
        }
        let _ = open::that_detached(url);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use tempfile::tempdir;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn make_state_at(path: PathBuf) -> FileBrowserState {
        FileBrowserState::new_at(path.clone(), path)
    }

    fn state_rooted_at(root: PathBuf, cwd: PathBuf) -> FileBrowserState {
        FileBrowserState::new_at(root, cwd)
    }

    // ── `s` behaviour ─────────────────────────────────────────────────

    #[test]
    fn s_commits_highlighted_entry() {
        let tmp = tempdir().unwrap();
        let parent = tmp.path().join("parent");
        let child = parent.join("child");
        std::fs::create_dir_all(&child).unwrap();

        // root = tmp so that neither parent nor child trip the $HOME guard.
        let mut state = state_rooted_at(tmp.path().to_path_buf(), parent.clone());
        // Highlighted entry at index 0 is `..`; advance to `child`.
        state.handle_key(key(KeyCode::Down));

        let outcome = state.handle_key(key(KeyCode::Char('s')));
        match outcome {
            ModalOutcome::Commit(path) => {
                assert_eq!(path.canonicalize().unwrap(), child.canonicalize().unwrap(),);
            }
            other => panic!("expected Commit, got {other:?}"),
        }
    }

    #[test]
    fn s_falls_back_to_cwd_when_directory_is_empty() {
        let tmp = tempdir().unwrap();
        let empty = tmp.path().join("empty");
        std::fs::create_dir(&empty).unwrap();

        let mut state = state_rooted_at(tmp.path().to_path_buf(), empty.clone());
        // Empty except for `..` — `s` should commit cwd, not `..`.
        let outcome = state.handle_key(key(KeyCode::Char('s')));
        match outcome {
            ModalOutcome::Commit(path) => {
                assert_eq!(path.canonicalize().unwrap(), empty.canonicalize().unwrap(),);
            }
            other => panic!("expected Commit, got {other:?}"),
        }
    }

    #[test]
    fn s_rejects_root_itself() {
        let tmp = tempdir().unwrap();
        let mut state = make_state_at(tmp.path().to_path_buf());
        let outcome = state.handle_key(key(KeyCode::Char('s')));
        assert!(matches!(outcome, ModalOutcome::Continue));
        assert!(state.rejected_reason.is_some());
    }

    #[test]
    fn s_rejects_jackin_data_dir() {
        let tmp = tempdir().unwrap();
        let jackin = tmp.path().join(".jackin").join("workspaces");
        std::fs::create_dir_all(&jackin).unwrap();

        let mut state = state_rooted_at(tmp.path().to_path_buf(), jackin);
        let outcome = state.handle_key(key(KeyCode::Char('s')));
        assert!(matches!(outcome, ModalOutcome::Continue));
        assert!(state.rejected_reason.is_some());
    }

    #[test]
    fn rejection_cleared_on_next_keypress() {
        let tmp = tempdir().unwrap();
        let mut state = make_state_at(tmp.path().to_path_buf());
        state.handle_key(key(KeyCode::Char('s')));
        assert!(state.rejected_reason.is_some());
        state.handle_key(key(KeyCode::Char('j')));
        assert!(state.rejected_reason.is_none());
    }

    // ── Esc step-back navigation ──────────────────────────────────────

    #[test]
    fn esc_at_root_cancels_modal() {
        let tmp = tempdir().unwrap();
        let mut state = make_state_at(tmp.path().to_path_buf());
        let outcome = state.handle_key(key(KeyCode::Esc));
        assert!(matches!(outcome, ModalOutcome::Cancel));
    }

    #[test]
    fn esc_inside_subfolder_navigates_up() {
        let tmp = tempdir().unwrap();
        let sub = tmp.path().join("sub");
        std::fs::create_dir(&sub).unwrap();

        let mut state = state_rooted_at(tmp.path().to_path_buf(), sub.clone());
        let outcome = state.handle_key(key(KeyCode::Esc));
        assert!(matches!(outcome, ModalOutcome::Continue));
        assert_eq!(
            state.cwd.canonicalize().unwrap(),
            tmp.path().canonicalize().unwrap(),
        );
    }

    #[test]
    fn esc_deep_navigates_up_one_level() {
        let tmp = tempdir().unwrap();
        let l1 = tmp.path().join("a");
        let l2 = l1.join("b");
        let l3 = l2.join("c");
        std::fs::create_dir_all(&l3).unwrap();

        let mut state = state_rooted_at(tmp.path().to_path_buf(), l3);
        state.handle_key(key(KeyCode::Esc));
        assert_eq!(
            state.cwd.canonicalize().unwrap(),
            l2.canonicalize().unwrap(),
        );
    }

    #[test]
    fn esc_clears_rejected_reason() {
        let tmp = tempdir().unwrap();
        let mut state = make_state_at(tmp.path().to_path_buf());
        state.rejected_reason = Some("stale reason".into());
        let outcome = state.handle_key(key(KeyCode::Esc));
        assert!(matches!(outcome, ModalOutcome::Cancel));
        assert!(state.rejected_reason.is_none());
    }

    // ── h / l navigation ──────────────────────────────────────────────

    #[test]
    fn h_navigates_up() {
        let tmp = tempdir().unwrap();
        let sub = tmp.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        let mut state = state_rooted_at(tmp.path().to_path_buf(), sub);
        state.handle_key(key(KeyCode::Char('h')));
        assert_eq!(
            state.cwd.canonicalize().unwrap(),
            tmp.path().canonicalize().unwrap(),
        );
    }

    #[test]
    fn h_at_root_is_noop() {
        let tmp = tempdir().unwrap();
        let mut state = make_state_at(tmp.path().to_path_buf());
        state.handle_key(key(KeyCode::Char('h')));
        assert_eq!(state.cwd, tmp.path());
    }

    #[test]
    fn l_navigates_into_highlighted_dir() {
        let tmp = tempdir().unwrap();
        let child = tmp.path().join("child");
        std::fs::create_dir(&child).unwrap();
        let mut state = make_state_at(tmp.path().to_path_buf());
        // No `..` at root — index 0 is `child`.
        state.handle_key(key(KeyCode::Char('l')));
        assert_eq!(
            state.cwd.canonicalize().unwrap(),
            child.canonicalize().unwrap(),
        );
    }

    #[test]
    fn enter_on_plain_folder_still_navigates() {
        let tmp = tempdir().unwrap();
        let parent = tmp.path().join("parent");
        let plain = parent.join("plain");
        std::fs::create_dir_all(&plain).unwrap();

        let mut state = state_rooted_at(tmp.path().to_path_buf(), parent);
        state.handle_key(key(KeyCode::Down));
        state.handle_key(key(KeyCode::Enter));
        assert!(state.pending_git_prompt.is_none());
        assert_eq!(
            state.cwd.canonicalize().unwrap(),
            plain.canonicalize().unwrap(),
        );
    }

    // ── Mouse-click hit-testing on the URL row ────────────────────────

    /// Reference geometry: a 70%-wide, 22-row `modal_area` at term 120x40
    /// gives `modal_area = Rect { x: 18, y: 9, width: 84, height: 22 }`.
    /// The listing chunk (no rejection) = `modal_area` minus a 1-row footer
    /// = `Rect { x: 18, y: 9, width: 84, height: 21 }`. Git-prompt overlay
    /// width = `min(84-4, 80) = 80`, height = 8 (`has_url = true`), centered
    /// inside listing → rect x ≈ 20, y = 9 + (21-8)/2 = 15. URL row sits
    /// at y + 2 = 17.
    fn manufactured_modal_area() -> Rect {
        // Mirrors `file_browser_modal_rect` for a term of 120x40:
        //   w = 120 * 70 / 100 = 84; h = 22.
        //   x = 0 + (120 - 84)/2 = 18; y = 0 + (40 - 22)/2 = 9.
        Rect {
            x: 18,
            y: 9,
            width: 84,
            height: 22,
        }
    }

    #[test]
    fn url_row_rect_none_when_no_url_flag() {
        // The public helper is parameterised on has_rejection; it always
        // assumes the git-prompt would render with a URL. This test pins
        // the returned rect when the overlay would have a URL row.
        let rect = git_prompt_url_row_rect(manufactured_modal_area(), false);
        assert!(rect.is_some(), "URL row should resolve for a valid modal");
    }

    #[test]
    fn click_on_url_row_without_url_returns_false() {
        let tmp = tempdir().unwrap();
        let parent = tmp.path().join("parent");
        let repo = parent.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();

        let mut state = state_rooted_at(tmp.path().to_path_buf(), parent);
        state.handle_key(key(KeyCode::Down));
        state.handle_key(key(KeyCode::Enter));
        assert!(state.pending_git_prompt.is_some());
        assert!(state.pending_git_url.is_none());

        // Click at the URL row's rough centre — still false because no URL.
        let modal = manufactured_modal_area();
        let url_rect = git_prompt_url_row_rect(modal, false).unwrap();
        let opened =
            state.maybe_open_url_on_click(modal, url_rect.x + url_rect.width / 2, url_rect.y);
        assert!(!opened, "click should not open when no URL is resolved");
    }

    #[test]
    fn click_outside_url_row_returns_false_even_with_url() {
        let tmp = tempdir().unwrap();
        let parent = tmp.path().join("parent");
        let repo = parent.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();

        let mut state = state_rooted_at(tmp.path().to_path_buf(), parent);
        state.handle_key(key(KeyCode::Down));
        state.handle_key(key(KeyCode::Enter));
        state.pending_git_url = Some("file:///tmp/definitely-not-real".to_string());

        let modal = manufactured_modal_area();
        let url_rect = git_prompt_url_row_rect(modal, false).unwrap();
        // One row below the URL row — outside.
        let opened =
            state.maybe_open_url_on_click(modal, url_rect.x + url_rect.width / 2, url_rect.y + 1);
        assert!(!opened, "click outside URL row should not open");
        // Column outside the URL row's x-range.
        let opened = state.maybe_open_url_on_click(
            modal, modal.x, // left border column
            url_rect.y,
        );
        assert!(!opened, "click on left border should not open");
    }

    #[test]
    fn click_on_url_row_with_url_returns_true() {
        let tmp = tempdir().unwrap();
        let parent = tmp.path().join("parent");
        let repo = parent.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();

        let mut state = state_rooted_at(tmp.path().to_path_buf(), parent);
        state.handle_key(key(KeyCode::Down));
        state.handle_key(key(KeyCode::Enter));
        state.pending_git_url = Some("file:///tmp/definitely-not-real".to_string());

        let modal = manufactured_modal_area();
        let url_rect = git_prompt_url_row_rect(modal, false).unwrap();
        let opened =
            state.maybe_open_url_on_click(modal, url_rect.x + url_rect.width / 2, url_rect.y);
        assert!(opened, "click on URL row with URL should return true");
        // Click doesn't dismiss the prompt.
        assert!(state.pending_git_prompt.is_some());
    }

    // ── Sandbox commit ─────────────────────────────────────────────────

    #[cfg(unix)]
    #[test]
    fn commit_rejects_out_of_root_target() {
        // TOCTOU defence: even if an escaping path somehow reached
        // `commit_or_reject` (list filtering beaten by a race, or a
        // future bug elsewhere), the belt-and-suspenders check rejects it.
        let tmp = tempdir().unwrap();
        let root = tmp.path().join("home");
        let outside = tmp.path().join("outside");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();

        let mut fb = FileBrowserState::new_at(root.clone(), root);
        let outcome = fb.commit_or_reject(outside);
        assert!(matches!(outcome, ModalOutcome::Continue));
        let reason = fb
            .rejected_reason
            .as_deref()
            .expect("out-of-root commit should set rejected_reason");
        assert!(
            reason.contains("outside"),
            "rejection should cite the sandbox boundary; got {reason:?}",
        );
    }

    #[test]
    fn click_when_no_git_prompt_is_active_returns_false() {
        let tmp = tempdir().unwrap();
        let parent = tmp.path().join("parent");
        std::fs::create_dir(&parent).unwrap();
        let state = state_rooted_at(tmp.path().to_path_buf(), parent);
        assert!(state.pending_git_prompt.is_none());

        let modal = manufactured_modal_area();
        let url_rect = git_prompt_url_row_rect(modal, false).unwrap();
        let opened =
            state.maybe_open_url_on_click(modal, url_rect.x + url_rect.width / 2, url_rect.y);
        assert!(!opened, "click without active git prompt should be inert");
    }
}
