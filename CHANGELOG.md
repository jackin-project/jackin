# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

<!-- next-header -->

## [Unreleased]

### Added

- **Workspace git pull on entry** — a per-workspace toggle (`git_pull_on_entry`) that runs `git pull` on every mounted git repository from the host before the agent container starts. Opt in with `--git-pull` on `workspace create` or `workspace edit`; toggle in the TUI General tab (Space on the new "Git pull" row). Failures are non-fatal: a warning is printed and the launch continues so the workspace remains usable when offline or the working tree is dirty.
