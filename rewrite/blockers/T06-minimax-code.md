# T06 MiniMax Code with MiniMax: blocked

State: unavailable.
Reason: no stable Linux artifact.
MiniMax publishes MiniMax Code only as an npm package and a desktop app. There is no stable standalone Linux binary: npm latest stable is 0.7.6, and the GitHub release asset v0.7.2 is an npm install package that needs Node.js, not a Linux arm64 or amd64 binary (rewrite/research/agent-artifacts.md, MiniMax Code section).
Jackin does not install an agent from npm in an image (rewrite/research/agent-artifacts.md).
Update 2026-11-08: re-affirmed. The artifact record still lists no standalone Linux binary, so T06 stays unavailable with this reason.
Update 2026-11-10 (Asia/Saigon): the latest operator goal requires the native MiniMax Code subscription login only. No coding-plan API-key fallback is permitted. This authentication clarification does not resolve the artifact blocker.
The pinned 0.7.6 client refresh request requires `audience=agent-backend` at `https://account.minimax.io/oauth2/token` (or the CN equivalent). OpenShell 0.1.7's generic OAuth refresh request has no audience field. Do not assume the field is optional or bypass OpenShell. An audience-capable OpenShell extension or vendor documentation that omission is supported is also required.
To unblock: MiniMax publishes a stable standalone Linux binary and the audience-capable refresh path is established. Then do task A1 again for this agent and run the A5 native-subscription live gate.
