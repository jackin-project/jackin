# Task index

| ID | Task | Owner | Dependencies |
| --- | --- | --- | --- |
| P00 | Freeze the work area and source evidence | Integration owner | None |
| P01 | Inventory every old implementation path | Source auditor | P00 |
| P02 | Verify the upstream API and compatibility tuple | Upstream API engineer | P00 |
| P03 | Freeze the internal contracts and schemas | Planner reviewer | P01, P02 |
| P04 | Build the evidence and acceptance harness | Qualification engineer | P03 |
| P05 | Create the clean v2 crate graph | Integration owner | P03, P04 |
| U00 | Qualify authoritative runtime observations | Upstream API engineer | P02, P03, P04 |
| U01 | Add server-side mutation identity fences | Upstream API engineer | P02, P03, P04 |
| U02 | Add strict verification-worker isolation | Upstream worker engineer | P02, P03, P04 |
| U03 | Qualify rebuilt executable network identity | Upstream worker engineer | P02, P03, P04 |
| U04 | Freeze the upstream companion API and ownership | Companion lifecycle engineer | P02, P03, P04 |
| U05 | Implement companion VM lifecycle and cleanup | Companion lifecycle engineer | U04, U01 |
| U06 | Implement confined companion project projection | Companion data engineer | U04 |
| U07 | Implement companion network ports and budgets | Companion network engineer | U04 |
| U08 | Qualify the complete managed companion | Qualification engineer | U05, U06, U07, U00, U03 |
| T10 | Implement v2 state and operation recovery | State engineer | P04, P05 |
| T11 | Implement the per-user host control service | State engineer | T10 |
| T12 | Implement native OpenShell connection handling | Runtime adapter engineer | P02, P05 |
| T13 | Implement scoped runtime operations and observations | Runtime adapter engineer | T12, T10, U00, U01 |
| T14 | Implement reproducible role resolution | Role engineer | P03, P05 |
| T15 | Implement explicit Buildx and image delivery | Role engineer | T14, T10, P02 |
| T16 | Build and validate portable role images | Role engineer | T15, T13, T18 |
| T17 | Implement the bounded Capsule protocol | Capsule engineer | P04, P05 |
| T18 | Rewrite Capsule as an unprivileged resident service | Capsule engineer | T17 |
| T19 | Implement relay attachment and input ownership | Capsule engineer | T18, T13, T11 |
| T20 | Implement portable project materialization | Project engineer | T13, T10 |
| T21 | Implement account grants and native providers | Account engineer | T13, T10 |
| T22 | Implement native-login files and agent adapters | Account engineer | T21, T20, T14 |
| T23 | Complete the native launch and activation path | Product engineer | T16, T19, T20, T21, T22, T11 |
| T24 | Implement domains sessions and the default shell | Product engineer | T23 |
| T25 | Implement trusted usage and account presentation | Account engineer | T21, T22, T24 |
| T26 | Implement host actions and approved tool domains | Product engineer | T24, T21, T13 |
| T27 | Implement checkpoints and conflict-safe export | Project engineer | T20, T22, T24 |
| T28 | Implement handoff eject exile and purge | Project engineer | T27, T13, T24 |
| T29 | Rebuild CLI Console and fleet observations | Product engineer | T24, T25, T26, T28 |
| T30 | Implement private developer service forwards | Product engineer | T13, T23 |
| T31 | Implement caches prewarm diagnostics and resource views | Operations engineer | T15, T28, T29 |
| T32 | Replace the independent verification worker | Verifier engineer | U02, T13, T20, T10 |
| T33 | Integrate the managed engine capability into Jackin | Companion adapter engineer | U08, T13, T23 |
| T34 | Complete workflow and product behavior coverage | Operations engineer | T32, T26, T27, T29 |
| T35 | Qualify local containers and remote container gateways | Qualification engineer | T29, T30, T31, T34, U03 |
| T36 | Qualify isolated VM profiles and engine domains | Qualification engineer | T29, T30, T31, T33, T34, U03 |
| T37 | Qualify the Kubernetes deployment tuple | Qualification engineer | T29, T30, T31, T34 |
| T38 | Run cross-area failure security and resource tests | Qualification engineer | T35, T36, T37, T32, T33 |
| T39 | Delete all obsolete implementation and rebuild packaging | Integration owner | P01, T31, T34, T35, T36, T37, T38 |
| T40 | Approve the full v2 release candidate | Independent reviewer | T39 |
