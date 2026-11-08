# T06 MiniMax Code with MiniMax: blocked

State: unavailable.
Reason: no stable Linux artifact.
MiniMax publishes MiniMax Code only as an npm package and a desktop app. There is no stable standalone Linux binary: npm latest stable is 0.7.6, and the GitHub release asset v0.7.2 is an npm install package that needs Node.js, not a Linux arm64 or amd64 binary (rewrite/research/agent-artifacts.md, MiniMax Code section).
Jackin does not install an agent from npm in an image (rewrite/research/agent-artifacts.md).
Update 2026-11-08: re-affirmed. The artifact record still lists no standalone Linux binary, so T06 stays unavailable with this reason.
To unblock: MiniMax publishes a stable standalone Linux binary. Then do task A1 again for this agent and run the A5 or A6 live gate.
