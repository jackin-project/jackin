# Sanitized host and client inventory

Captured: 2026-09-21 (Asia/Ho_Chi_Minh)
Repository: /Users/donbeave/Projects/tailrocks/jackin-project/jackin
Source HEAD at capture: 2a318440ce2e02a15a76530812f375ee4998a0f3
Tree state: main merge in progress; multiple conflict files; environment probes only. No workspace build/test run by this verifier.

## Host and toolchain

- macOS 27.0, build 26A428; Darwin 27.0.0; Apple Silicon arm64 (T6050).
- OrbStack 2.2.3, build 20963; orbctl status = Running.
- Docker context selected: orbstack; Docker client/server 29.4.0.
- Rust/cargo 1.97.1; cargo-nextest 0.9.140; Swift 6.4.0.34.1; mise 2026.9.11.
- No local macOS 26 guest/second system was found. Tart, UTM, Parallels, VMware Fusion, VirtualBuddy, qemu-system-aarch64, vfkit, vftool, and limactl are absent; only OrbStack is installed. Current host is 27.0, so the repository's exact macOS 26 lane is not proven here. Parent separately found zero registered GitHub self-hosted runners. A GitHub-hosted macOS 26 + OrbStack lane would still need to demonstrate the repository's required transport behavior.

## Client binaries and versions

- Codex CLI 0.155.0 (rtk proxy codex --version)
- Claude Code 2.1.276 (rtk proxy claude --version)
- OpenCode 1.18.30 (rtk proxy opencode --version)
- Amp 0.0.1789848041-gfc88c5 (rtk proxy amp --version)
- Cursor CLI 3.20.17, arm64 (rtk proxy cursor --version)
- Cursor Agent 2026.09.15-d2fe57e (rtk proxy cursor-agent --version)
- Antigravity CLI agy 1.2.7 (rtk proxy agy --version)
- Kimi Code CLI 2.0.0 (rtk proxy kimi --version)
- Muse Code 1.3.0 (R3401.1) (rtk proxy muse --version)
- Grok Build CLI 1.0.30 (rtk proxy grok --version)
- Gemini CLI absent.
- GUI bundles observed: Cursor, Claude, Antigravity, OpenCode.

## Credential/account state (metadata only; no values, tokens, emails, or raw account IDs recorded)

- jackin❯ config metadata was parsed locally and reduced through an isolated sanitized temporary copy. Config file mode is 0600. No registered-account collection exists in this config. It has environment-reference entries for KIMI_CODE_API_KEY, MINIMAX_API_KEY, and ZAI_API_KEY; the reference values/locators were not read or copied.
- Codex login status reports logged in. The default, .codex-chainargos, and .codex-chainargos2 auth documents are regular mode-0600 files, each with an account-ID field; local identity fields differ pairwise. This is local-file evidence only, not a provider usage response or server-verified billing identity. .codex-scentbird has no standard auth file.
- Claude auth status, run with stdin closed under default and the three existing custom CLAUDE_CONFIG_DIR roots, reports not authenticated. The usual JSON credential files are absent. The user keychain list has one login.keychain-db; security show-keychain-info returned <NULL> no-timeout, so native service-item availability/lock state remains unresolved.
- cursor-agent status reports authenticated (response suppressed).
- kimi provider list reports managed Kimi Code OAuth, four configured models, default kimi-code/k3; this is local client config, not a live usage/auth verification.
- opencode auth list reports 0 credentials.
- 1Password CLI 2.39.0 is installed, but `op whoami --format=json` returned exit code 1, so no authenticated 1Password session was available. No `op read` was run and no secret or item locator was inspected.
- Amp, Muse, Grok, and Antigravity installed-client auth state is unverified.
- No provider API-key/token environment variable values were read. A variable-name scan found no provider API-key names.

## Evidence commands

All shell commands used the rtk prefix. Read-only probes included sw_vers, uname -a, docker context ls, docker version, orbctl status, CLI --version and explicit auth-status/list commands, metadata-only filesystem/keychain checks, and cargo metadata --no-deps --format-version 1 --locked (parsed 30 workspace packages). No model prompts, provider usage reads, keychain-item secret reads, Cargo builds, or Git mutations were performed by this verifier.
