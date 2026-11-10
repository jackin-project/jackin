# T13 Claude Code with Kimi: supervised live proof passed

State: proof passed on both gateways. Production availability remains unchanged pending orchestrator review.
Reason: commit `15b0191fa4b2c3834ae5df3780f1b530783cf7b5` passed the fresh supervised T13 proof on `jackin-docker` and `jackin-local` with exact `kimi-for-coding` settings and `/status` evidence.
Current evidence, 2026-11-10 (Asia/Saigon): both final fresh launches rendered only `model` and the Kimi base URL, delivered `ANTHROPIC_AUTH_TOKEN` without `ANTHROPIC_API_KEY`, ran the real interactive `ls -A /sandbox/project` tool, returned exact `T13-PTY-OK`, and finished at `running 1` with no restart. Both `/status` captures reported `Model: kimi-for-coding`.
Two detach and reattach replays, direct-child preservation, shell persistence, cached reload, uid and placeholder checks, zero host-key disclosure, provider-only egress, subscription usage, exact export, and guarded purge passed on both gateways. The temporary T13 `Supported` enablement was restored after the proof.
An earlier docker driver attempt mishandled the trust screen before submitting a task. Its sandbox and exported ref were cleaned. It is not used as proof and does not affect the two passing final cells.
Evidence: `rewrite/evidence/A14-t13-model-live.md`.
