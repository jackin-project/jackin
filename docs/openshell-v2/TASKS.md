# Task index

| ID | Task | Subagent | Dependencies |
| --- | --- | --- | --- |
| P00 | Freeze the work area and source evidence | Integration subagent | None |
| P01 | Inventory every old implementation path | Source audit subagent | P00 |
| P02 | Verify the upstream API and compatibility tuple | Upstream API subagent | P00 |
| P03 | Freeze the internal contracts and schemas | Planning review subagent | P01, P02 |
| P04 | Build the evidence and acceptance harness | Qualification subagent | P03 |
| P05 | Create the clean v2 crate graph | Integration subagent | P03, P04 |
| U00 | Qualify authoritative runtime observations | Upstream API subagent | P02, P03, P04 |
| U01 | Add server-side mutation identity fences | Upstream API subagent | P02, P03, P04 |
| U02 | Add strict verification-worker isolation | Upstream worker subagent | P02, P03, P04 |
| U03 | Qualify rebuilt executable network identity | Upstream worker subagent | P02, P03, P04 |
| U04 | Freeze the upstream companion API and ownership | Companion lifecycle subagent | P02, P03, P04 |
| U05 | Implement companion VM lifecycle and cleanup | Companion lifecycle subagent | U04, U01 |
| U06 | Implement confined companion project projection | Companion data subagent | U04 |
| U07 | Implement companion network ports and budgets | Companion network subagent | U04 |
| U08 | Qualify the complete managed companion | Qualification subagent | U05, U06, U07, U00, U03 |
| T10 | Implement v2 state and operation recovery | State subagent | P04, P05 |
| T11 | Implement the per-user host control service | State subagent | T10 |
| T12 | Implement native OpenShell connection handling | Runtime adapter subagent | P02, P05 |
| T13 | Implement scoped runtime operations and observations | Runtime adapter subagent | T12, T10, U00, U01 |
| T14 | Implement reproducible role resolution | Role-image subagent | P03, P05 |
| T15 | Implement explicit Buildx and image delivery | Role-image subagent | T14, T10, P02 |
| T16 | Build and validate portable role images | Role-image subagent | T15, T13, T18 |
| T17 | Implement the bounded Capsule protocol | Capsule subagent | P04, P05 |
| T18 | Rewrite Capsule as an unprivileged resident service | Capsule subagent | T17 |
| T19 | Implement relay attachment and input ownership | Capsule subagent | T18, T13, T11 |
| T20 | Implement portable project materialization | Project subagent | T13, T10 |
| T21 | Implement account grants and native providers | Account subagent | T13, T10 |
| T22 | Implement native-login files and agent adapters | Account subagent | T21, T20, T14 |
| T23 | Complete the native launch and activation path | Product subagent | T16, T19, T20, T21, T22, T11 |
| T24 | Implement domains sessions and the default shell | Product subagent | T23 |
| T25 | Implement trusted usage and account presentation | Account subagent | T21, T22, T24 |
| T26 | Implement host actions and approved tool domains | Product subagent | T24, T21, T13 |
| T27 | Implement checkpoints and conflict-safe export | Project subagent | T20, T22, T24 |
| T28 | Implement handoff eject exile and purge | Project subagent | T27, T13, T24 |
| T29 | Rebuild CLI Console and fleet observations | Product subagent | T24, T25, T26, T28, T30 |
| T30 | Implement private developer service forwards | Product subagent | T13, T23 |
| T31 | Implement caches prewarm diagnostics and resource views | Operations subagent | T15, T28, T29, T33 |
| T32 | Replace the independent verification worker | Verifier subagent | U02, T13, T20, T10 |
| T33 | Integrate the managed engine capability into Jackin | Companion adapter subagent | U08, T13, T23, T27 |
| T34 | Complete workflow and product behavior coverage | Operations subagent | T32, T26, T27, T29, T30, T31 |
| T35 | Qualify local containers and remote container gateways | Qualification subagent | T29, T30, T31, T34, U03 |
| T36 | Qualify isolated VM profiles and engine domains | Qualification subagent | T29, T30, T31, T33, T34, U03 |
| T37 | Qualify the Kubernetes deployment tuple | Qualification subagent | T29, T30, T31, T34, U03 |
| T38 | Run cross-area failure security and resource tests | Qualification subagent | T35, T36, T37, T32, T33 |
| T39 | Delete all obsolete implementation and rebuild packaging | Integration subagent | P01, T31, T34, T35, T36, T37, T38 |
| T40 | Approve the full v2 release candidate | Independent review subagent | T39 |
