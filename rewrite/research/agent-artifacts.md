# Agent artifacts (A1)

Date of check: 2026-11-06. Scope: official vendor sources only. No agent was run. No credential was read.
A pin is set in task A2 from this file. Recheck each version and checksum at pin time.
Grok Build has no vendor checksum file. Record the computed hash as a first-fetch hash, as D02 does for the Docker CLI. Do not call it a vendor hash.
MiniMax Code has no stable standalone Linux binary. Tuple T06 is `unavailable: no stable Linux artifact` until MiniMax publishes one. Do not install it from npm in an image.

## Claude Code (claude)

Latest stable version: `2.1.311`.
The stable channel pointer is the source.
Source: https://downloads.claude.ai/claude-code-releases/stable
The latest channel is `2.1.318`.
The latest channel is not used.
Exact CLI name: `claude`.
Vendor download base: https://downloads.claude.ai/claude-code-releases
Linux arm64 artifact: https://downloads.claude.ai/claude-code-releases/2.1.311/linux-arm64/claude
SHA-256: `2e2496c9a8065df93acb0fca2d341f59a6b8777649b2e025ab8c9bcd5eef61f3`
Linux amd64 artifact: https://downloads.claude.ai/claude-code-releases/2.1.311/linux-x64/claude
SHA-256: `86dc1fcc69498159eaf15f446613e847010f9036bfbbdffd4c49350be8489ada`
Checksums come from the vendor manifest.
Source: https://downloads.claude.ai/claude-code-releases/2.1.311/manifest.json
Musl variants also exist.
Version command: `claude --version`
Version output format: `<version> (Claude Code)`
Example format: `2.1.311 (Claude Code)`
Source: https://code.claude.com/docs/en/setup
State dir: `~/.claude`
Override: `CLAUDE_CONFIG_DIR`
Auth file: `~/.claude/.credentials.json`
Config file: `~/.claude/settings.json`
Global config: `~/.claude.json`
Source: https://code.claude.com/docs/en/authentication
Source: https://code.claude.com/docs/en/settings
Interactive launch: `claude`
Own sandbox: off by default.
Disable value: `"sandbox": {"enabled": false}` in `settings.json`.
Source: https://code.claude.com/docs/en/sandboxing
Disable auto-update: `DISABLE_AUTOUPDATER=1`
Full block: `DISABLE_UPDATES`
Source: https://code.claude.com/docs/en/setup

## Grok Build (grok)

Latest stable version: `1.0.77`.
Source: https://x.ai/cli/stable
Exact CLI name: `grok`.
Vendor installer: https://x.ai/cli/install.sh
Linux arm64 artifact: https://x.ai/cli/grok-1.0.77-linux-aarch64
Linux amd64 artifact: https://x.ai/cli/grok-1.0.77-linux-x86_64
The vendor publishes no checksum file.
Probed `.sha256` files return 404.
The installer checks no checksum.
Computed SHA-256, arm64: `b30eee14a656124ef3c9da8577b5e783ef2cffabeb2b679c5c49633e77eed574`
Computed SHA-256, amd64: `b702950aa5827d855e493278ab6215a4e0f5bceb5bba6c6243a9f55cdd143b0a`
Computed values are not vendor values.
Source: https://docs.x.ai/build/overview
Version command: `grok version`
Also accepted: `grok --version`
Exact output format: not published by vendor.
State dir: `~/.grok`
Override: `GROK_HOME`
Auth file: `~/.grok/auth.json`
Config file: `~/.grok/config.toml`
Interactive launch: `grok`
Own sandbox: off by default.
Disable flag: `--sandbox off`
Disable env: `GROK_SANDBOX=off`
Source: https://docs.x.ai/llms-full.txt
Disable auto-update flag: `--no-auto-update`
Disable auto-update env: `GROK_DISABLE_AUTOUPDATER`
Disable auto-update config: `auto_update = false` under `[cli]`.

## Muse Code (muse)

Latest stable version: `1.4.14-R7309.1`.
Display version: `1.4.14`.
Source: https://api.meta.ai/muse-code/channels/muse-stable
Exact CLI name: `muse`.
Vendor installer: https://dev.meta.ai/install.sh
Linux arm64 artifact: https://lookaside.facebook.com/lookaside/muse/download/?channel=muse&version=1.4.14-R7309.1&file=muse-aarch64-linux
SHA-256: `3dc39b8fa26f9963cc2038f2966c0932ed5a39f0f9efb39cc0266a261cbbcf76`
Linux amd64 artifact: https://lookaside.facebook.com/lookaside/muse/download/?channel=muse&version=1.4.14-R7309.1&file=muse-x86-linux
SHA-256: `0809aac7cb58d18f5022b6d05e02aa1974683c0ec430e5f04d25b60ed17de2da`
Checksums come from the vendor manifest.
Source: https://lookaside.facebook.com/lookaside/muse/download/?channel=muse&version=1.4.14-R7309.1&file=manifest.json
Version command: `muse --version`
Exact output format: not stated in vendor docs.
Observed prior format in repo research: `Muse Code 1.4.14 (1.4.14-R7309.1)`
That format is not vendor-documented.
Source: https://dev.meta.ai/docs/muse-code
State dir: not documented by vendor.
Install dir: `~/.local/bin`
Override: `MUSE_INSTALL_DIR`
Source: https://api.meta.ai/muse-launcher.sh
Auth file: `~/.config/muse/auth.json`
Override: `MUSE_AUTH_PATH`
Source: https://dev.meta.ai/docs/muse-code/auth
Config file: `~/.config/muse/settings.json`
Source: https://dev.meta.ai/docs/muse-code/configuration
Interactive launch: `muse`
Disable own sandbox flag: `--disable-sandbox`
Source: https://dev.meta.ai/docs/muse-code/permissions
Disable auto-update: `MUSE_NO_AUTO_UPDATE=1`
No CLI flag is documented.

## Antigravity (agy)

Latest stable version: `1.5.1`.
Build: `1.5.1-4523430503120896`.
Exact CLI name: `agy`.
Vendor installer: https://antigravity.google/cli/install.sh
Source: https://antigravity.google/docs/getting-started/
Linux arm64 artifact: https://storage.googleapis.com/antigravity-public/antigravity-cli/1.5.1-4523430503120896/linux-arm/cli_linux_arm64.tar.gz
Linux amd64 artifact: https://storage.googleapis.com/antigravity-public/antigravity-cli/1.5.1-4523430503120896/linux-x64/cli_linux_x64.tar.gz
The vendor publishes SHA-512, not SHA-256.
SHA-512, arm64: `fdf8a463e5bf4ecd8a01b68a53e006e7ae0256e7ae351a39e39c66e04e275c6d08b4b93f423cc572d234871f7bf852c4523e017156a4ef9ea7dea1aaffb0a2c6`
SHA-512, amd64: `f70406654ebd966f211424338259e2d38d099c48734e901dc06975063e5a0137d55b6eb9366b75983c6908bbc20ae92a2617656bb191d8ed6fe429d654bca4f5`
Source: https://antigravity-cli-auto-updater-974169037036.us-central1.run.app/manifests/linux_arm64.json
Source: https://antigravity-cli-auto-updater-974169037036.us-central1.run.app/manifests/linux_amd64.json
Computed SHA-256, arm64: `c43a50831006d95a7637aa9cad7000fda9b9bb4b3ece8c53cc6c5089b92542ea`
Computed SHA-256, amd64: `2a64ccfc5f6679c253769f0760614af5de04d8dc1eeeec9b9752ba4c8dd92250`
Computed values are not vendor values.
Recheck 2026-11-09 at the T05 pin: latest stable is `1.5.3`, build `1.5.3-6604829099950080`.
The updater manifests for both platforms answered `1.5.3` with the URLs below, and both tarballs were downloaded and hashed.
Linux arm64 artifact: https://storage.googleapis.com/antigravity-public/antigravity-cli/1.5.3-6604829099950080/linux-arm/cli_linux_arm64.tar.gz
SHA-512 (vendor): `c2fac818b3c1bb224079df03a714ccba99728b6a3a41d86d247ab2feb8582a43840c5032f1c2b75e72667623d5503e7ffda2712c4f37b6ee19f99c3fcfb25bae`
Computed SHA-256, arm64: `2d50060c7b1327995e5c81ec58829c91c2564ebb9fec165d3f9f1325ec188675`
Linux amd64 artifact: https://storage.googleapis.com/antigravity-public/antigravity-cli/1.5.3-6604829099950080/linux-x64/cli_linux_x64.tar.gz
SHA-512 (vendor): `e363e1b17daaac2589f18e6973429c039fd5a5e5c15e9c0852fa349836f3992b10fe84732bc8d8c8208e149f0304f15aba9dec696d12efeed03917492c0580eb`
Computed SHA-256, amd64: `1b4850599cce73a17f94ffb422df3bf9531531b77dbddc40c1a435e0c2c16807`
Both computed SHA-512 values match the manifest values; the SHA-256 values are first-fetch hashes in the sense of D02.
The tarball holds one member, `antigravity`; the vendor installer extracts it and installs it as `agy` (https://antigravity.google/cli/install.sh).
No raw (non-tar) Linux binary exists: probes of `agy`, `antigravity`, and `cli_linux_arm64` under the same prefix, and of the bare `1.5.3` prefix, all returned 404.
Version command: `agy --version`
Exact output format: not documented by vendor.
Prior repo research observed bare `1.5.1`.
State dir: `~/.gemini/antigravity-cli/`
Auth file: none.
Auth storage: OS keyring.
On Linux: secret-service over D-Bus.
Config file: `~/.gemini/antigravity-cli/settings.json`
Source: https://antigravity.google/docs/cli/reference
Source: https://antigravity.google/docs/cli/troubleshooting/
Interactive launch: `agy`
Own sandbox: off by default.
No `--no-sandbox` flag is documented.
Disable value: omit `--sandbox`; keep `enableTerminalSandbox: false`.
Disable auto-update: `AGY_CLI_DISABLE_AUTO_UPDATE=true`
No flag is documented.

## MiniMax Code

- Product: MiniMax Code.
- Binary: `mcode`.
- Binary source: [MiniMax Code repo](https://github.com/MiniMax-AI/minimax-code).
- Latest stable on npm: `0.7.6`.
- npm evidence: [npm package](https://www.npmjs.com/package/@minimax-ai/code).
- npm tarball: https://registry.npmjs.org/@minimax-ai/code/-/code-0.7.6.tgz
- npm integrity: `sha512-JhW+amhgnKKm25YK+35pVW4GFK9lXHxVO+Bk7U7UAGO2ljNsWoKebkiKi205YqBVkcJB4JOCNxljKEXjYlsGYg==`.
- Latest GitHub release seen: `v0.7.2`.
- GitHub asset: `minimax-code-0.7.2.tar.gz`.
- GitHub asset SHA-256: `aa055b696945286cbab62f8b748835de1428e4d08a598bbf5de4c8a5de71f4e4`.
- GitHub release: [v0.7.2](https://github.com/MiniMax-AI/minimax-code/releases/tag/v0.7.2).
- The GitHub asset is an npm install package.
- The package needs Node.js.
- It is not a Linux arm64 binary.
- It is not a Linux amd64 binary.
- unavailable: no stable Linux artifact
- Vendor installer: https://filecdn.minimax.chat/public/install.sh
- Installer source: [README](https://github.com/MiniMax-AI/minimax-code#quick-start).
- Install dir: `~/.minimax-code`.
- Data dir: `~/.minimax`.
- Profile data dir: `~/.minimax-<profile>`.
- Data dir override: `MINIMAX_DATA_DIR`.
- Config file: `<data-dir>/config.yaml` is not fully verified from official docs in this pass.
- Auth path: not verified from official docs in this pass.
- Repo evidence says auth records live under `~/.minimax/auth/prod/en/mcode-public/auth.json`.
- Do not read that file in research.
- Version command: `mcode --version`.
- Exact version output format: unverified.
- Interactive launch: `mcode`.
- Login command: `mcode login`.
- Global login command: `mcode login --region global`.
- Own sandbox disable flag: unverified.
- Auto-update disable flag: unverified.
- Built-in updater follows the npm registry channel.
- Updater source: [installation guide](https://github.com/MiniMax-AI/minimax-code/blob/main/docs/installation.md).

## MiMo Code (mimo)

- Product: MiMo Code.
- Binary: `mimo`.
- Artifact binary name in archive: `mimocode`.
- Binary source: [MiMo-Code repo](https://github.com/XiaomiMiMo/MiMo-Code).
- Latest stable: `0.1.16`.
- Release tag: `v0.1.16`.
- Release date: 2026-10-26.
- Release: [v0.1.16](https://github.com/XiaomiMiMo/MiMo-Code/releases/tag/v0.1.16).
- Vendor installer: https://mimo.xiaomi.com/install
- Installer source: [README](https://github.com/XiaomiMiMo/MiMo-Code#quick-start).
- Linux arm64 artifact: `mimocode-linux-arm64.tar.gz`.
- Linux arm64 URL: https://github.com/XiaomiMiMo/MiMo-Code/releases/download/v0.1.16/mimocode-linux-arm64.tar.gz
- Linux arm64 SHA-256: `220c621eb61a175ba0bc8e9c45c217abfed8dace1f17111c3717bceb95ad2ab3`.
- Linux arm64 musl artifact: `mimocode-linux-arm64-musl.tar.gz`.
- Linux arm64 musl SHA-256: `f2cc7613ffa1f875c1f896b41fa800629c7fe9beaf7d0721667b9ae81b8f5d46`.
- Linux amd64 artifact: `mimocode-linux-x64.tar.gz`.
- Linux amd64 URL: https://github.com/XiaomiMiMo/MiMo-Code/releases/download/v0.1.16/mimocode-linux-x64.tar.gz
- Linux amd64 SHA-256: `3793d8b08ae7c1c6cee707645da1b1c827e0f13faa5d40d076f08ea4ee1d9a5e`.
- Linux amd64 musl SHA-256: `3118c53c8366a29de6bc0fd9a7b11b24b3a3a7925f825c12fcb599a94ec89f9e`.
- Checksums come from GitHub release asset digests.
- No separate vendor checksum file was found in this pass.
- Version command: `mimo --version`.
- Exact version output format: unverified.
- State dir: unverified from official docs in this pass.
- Auth file path: unverified from official docs in this pass.
- Config file path: unverified from official docs in this pass.
- Interactive launch: `mimo`.
- Own sandbox disable flag: unverified.
- Auto-update disable flag: unverified.

## Kimi Code (kimi)

- Product: Kimi Code.
- Binary: `kimi`.
- Binary source: [kimi-code repo](https://github.com/MoonshotAI/kimi-code).
- Latest stable: `2.5.1`.
- Release tag: `@moonshot-ai/kimi-code@2.5.1`.
- Release date: 2026-11-05.
- Release: [2.5.1](https://github.com/MoonshotAI/kimi-code/releases/tag/%40moonshot-ai%2Fkimi-code%402.5.1).
- Vendor installer: https://code.kimi.com/kimi-code/install.sh
- Installer source: [README](https://github.com/MoonshotAI/kimi-code#install).
- Linux arm64 artifact: `kimi-code-linux-arm64.tar.gz`.
- Linux arm64 URL: https://github.com/MoonshotAI/kimi-code/releases/download/%40moonshot-ai%2Fkimi-code%402.5.1/kimi-code-linux-arm64.tar.gz
- Linux arm64 SHA-256: `70c3a09506f7d3d0f211077eb100d34186fef1017347c73ed0d944bb2b474cf3`.
- Vendor checksum file: `kimi-code-linux-arm64.tar.gz.sha256` in the same release.
- Linux amd64 artifact: `kimi-code-linux-x64.tar.gz`.
- Linux amd64 URL: https://github.com/MoonshotAI/kimi-code/releases/download/%40moonshot-ai%2Fkimi-code%402.5.1/kimi-code-linux-x64.tar.gz
- Linux amd64 SHA-256: `d3fe0696e2ed244200c6c42344cbd71429eff40dc9761fdcb5f994057b78bee9`.
- Vendor checksum file: `kimi-code-linux-x64.tar.gz.sha256` in the same release.
- State dir: `~/.kimi-code`.
- State dir override: `KIMI_CODE_HOME`.
- Config file: `~/.kimi-code/config.toml`.
- Config file with override: `$KIMI_CODE_HOME/config.toml`.
- Config source: [config files](https://moonshotai.github.io/kimi-code/en/configuration/config-files).
- Auth file: `<kimi-home>/credentials/kimi-code.json` by default.
- Scoped auth file can differ by provider config.
- Auth source: repo usage research, section Kimi.
- Do not read that file in research.
- Version command: `kimi --version`.
- Repo observed output for 2.4.1: `2.4.1`.
- Exact format: bare version number.
- Interactive launch: `kimi`.
- Own sandbox disable flag: unverified.
- Permission mode is not a sandbox disable flag.
- Auto-update disable flag: unverified.

## omp

- Product: omp (Oh My Pi).
- Binary: `omp`.
- Binary source: [oh-my-pi repo](https://github.com/can1357/oh-my-pi).
- Site: [omp.sh](https://omp.sh/).
- Latest stable: `18.16.0`.
- Release tag: `v18.16.0`.
- Release date: 2026-11-02.
- Release: [v18.16.0](https://github.com/can1357/oh-my-pi/releases/tag/v18.16.0).
- Vendor installer: https://omp.sh/install
- Installer source: [README](https://github.com/can1357/oh-my-pi#install).
- Linux arm64 artifact: `omp-linux-arm64`.
- Linux arm64 URL: https://github.com/can1357/oh-my-pi/releases/download/v18.16.0/omp-linux-arm64
- Linux arm64 SHA-256: `c2be1ea85ce17c4270854b57efff2a7ea1d5a03c7557fd08721f2b9bb32698de`.
- Linux amd64 artifact: `omp-linux-x64`.
- Linux amd64 URL: https://github.com/can1357/oh-my-pi/releases/download/v18.16.0/omp-linux-x64
- Linux amd64 SHA-256: `6498ed785bab00d2926e0387e0ec02362d6d45f1c3d0d1b57c1618b7f95c9264`.
- Vendor checksum file: `SHA256SUMS.txt` in the same release.
- Checksum file URL: https://github.com/can1357/oh-my-pi/releases/download/v18.16.0/SHA256SUMS.txt
- Repo observed output for 18.16.0: `omp/18.16.0`.
- Version command: `omp --version`.
- Exact format: `omp/<version>`.
- State dir: unverified from official docs in this pass.
- Auth file path: unverified from official docs in this pass.
- Config file path: unverified from official docs in this pass.
- Interactive launch: `omp`.
- Own sandbox disable flag: unverified.
- Auto-update disable flag: unverified.

## Codex (codex)

- Product: Codex CLI.
- Binary: `codex`.
- Binary source: [codex repo](https://github.com/OpenAI/codex).
- Known good reference in repo: `0.166.0`.
- Latest stable found: `0.167.0`.
- Release tag: `rust-v0.167.0`.
- Release date: 2026-11-06.
- Release: [rust-v0.167.0](https://github.com/OpenAI/codex/releases/tag/rust-v0.167.0).
- Plan should pin `0.166.0` until the operator approves a bump.
- Pin rule source: repo DESIGN D02.
- Linux arm64 package artifact: `codex-package-aarch64-unknown-linux-musl.tar.gz`.
- Linux arm64 URL: https://github.com/OpenAI/codex/releases/download/rust-v0.167.0/codex-package-aarch64-unknown-linux-musl.tar.gz
- Linux arm64 SHA-256: `0de261ecf1e7f3028a7952f03fca2093f749277d38590b78bb32f8567756bbb7`.
- Linux amd64 package artifact: `codex-package-x86_64-unknown-linux-musl.tar.gz`.
- Linux amd64 URL: https://github.com/OpenAI/codex/releases/download/rust-v0.167.0/codex-package-x86_64-unknown-linux-musl.tar.gz
- Linux amd64 SHA-256: `3bbf75e83de6b58aed28f2cf6e33ad423ec31fd4700a1457b94be557ea5a7c6e`.
- Vendor checksum file: `codex-package_SHA256SUMS` in the same release.
- Repo pinned 0.166.0 arm64 SHA-256: `7821b77dda2e6283fb1f63f3165177a271da8833e857cf673c6ca1c9db3bfcc6`.
- State dir: `~/.codex`.
- State dir env: `CODEX_HOME`.
- Auth file: `$CODEX_HOME/auth.json`.
- Auth file default: `~/.codex/auth.json`.
- Auth source: [Codex auth docs](https://developers.openai.com/codex/auth).
- Config file: `$CODEX_HOME/config.toml`.
- Config file default: `~/.codex/config.toml`.
- Version command: `codex --version`.
- Repo observed output for 0.166.0: `codex-cli 0.166.0`.
- Exact format: `codex-cli <version>`.
- Interactive launch: `codex --sandbox danger-full-access -c check_for_update_on_startup=false`.
- Launch source: repo DESIGN D04.
- Own sandbox disable flag: `--sandbox danger-full-access`.
- Auto-update disable: `-c check_for_update_on_startup=false`.
- Config form: `check_for_update_on_startup = false` in `config.toml`.

## Open items

- Verify MiniMax config and auth paths from official docs.
- Verify MiMo state, auth, and config paths from official docs.
- Verify omp state, auth, and config paths from official docs.
- Verify sandbox disable flags for MiniMax, MiMo, Kimi, and omp.
- Verify auto-update disable flags for MiniMax, MiMo, Kimi, and omp.
- Do not guess these flags.
