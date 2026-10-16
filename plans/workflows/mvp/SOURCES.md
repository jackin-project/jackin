# Sources and verification limits

## Scope

The attached architecture review is the main source for this update.
The prior Workflows pack supplies the original full-product scope.
This update checks selected current source files and official external documentation.
It does not run Jackin, agents, containers, or product tests.

**Current Jackin source:** `0e11cf80957365317cdf48efe4dc5eea239357d7`.  
**Earlier source audit:** `453bf611fd974ff88f18caa97aa225af1f8edb2c`.  
**Document date:** 15 October 2026.

## User-provided sources

| ID | Source | Use |
|---|---|---|
| U1 | Attached `Pasted text(20261014-172855).txt`, titled Jackin Workflows architecture review. | Review findings and proposed first milestone. |
| U2 | Original Jackin Workflows version 1 specification in this thread. | Product purpose, host authority, original-session requirement, and full acceptance scope. |
| U3 | Earlier `jackin-workflows-complete.md` and Markdown archive in this thread. | Prior component boundaries and detailed full-product design. |

U1 sections 2 through 6 supply the main issues addressed in `REVIEW.md`.
The last paragraph of U1 states that no local runtime tests were executed.
The review is not evidence that the product works.

## Current repository sources

All J references below use the current source commit unless explicitly marked historical.

| ID | Source | Verified finding |
|---|---|---|
| J1 | [Main ref](https://api.github.com/repos/jackin-project/jackin/git/ref/heads/main) | The returned source commit is the current audit anchor. |
| J2 | [Root AGENTS.md](https://github.com/jackin-project/jackin/blob/0e11cf80957365317cdf48efe4dc5eea239357d7/AGENTS.md) | Delegate first. Internal breaking changes are permitted. Keep instructions lean. |
| J3 | [Workflows README](https://github.com/jackin-project/jackin/blob/0e11cf80957365317cdf48efe4dc5eea239357d7/plans/workflows/README.md) | The reading order no longer requires the omitted research files. |
| J4 | [Current build sequence](https://github.com/jackin-project/jackin/blob/0e11cf80957365317cdf48efe4dc5eea239357d7/plans/workflows/implementation/01-build-sequence.md) | Real-agent work follows broad infrastructure work. An active instruction still names TESTING.md. |
| J5 | [mise.toml](https://github.com/jackin-project/jackin/blob/0e11cf80957365317cdf48efe4dc5eea239357d7/mise.toml) | The inspected Rust tool version is 1.99.0. The tool file includes mbx and nextest tooling. |
| J6 | [Capsule control types](https://github.com/jackin-project/jackin/blob/0e11cf80957365317cdf48efe4dc5eea239357d7/crates/core/jackin-protocol/src/control.rs) | SessionSent contains a session ID and written byte count, not an acceptance result. |
| J7 | [Docker grant application](https://github.com/jackin-project/jackin/blob/0e11cf80957365317cdf48efe4dc5eea239357d7/crates/services/jackin-runtime-docker-profile/src/docker_profile/effective.rs) | Resource overrides use raised maxima. Privilege fields can expand existing grants. |
| J8 | [.velnor/config.toml](https://github.com/jackin-project/jackin/blob/0e11cf80957365317cdf48efe4dc5eea239357d7/.velnor/config.toml) | Rust uses mbx and cargo_nextest. Several runtime leaf crates are excluded from discovery. |
| J9 | [Usage store backend](https://github.com/jackin-project/jackin/blob/0e11cf80957365317cdf48efe4dc5eea239357d7/crates/services/jackin-usage-store-backend/README.md) | Current search results identify the shared turso connection boundary. Do not share usage-domain tables. |
| J10 | [Workflow directory](https://github.com/jackin-project/jackin/tree/0e11cf80957365317cdf48efe4dc5eea239357d7/plans/workflows) | Retained specification directories are available. The missing research directory is not a new MVP dependency. |

The request for `.github/ci/project.toml` at this commit returned 404.
Do not use that historical path as a required current policy file.
Recheck file locations at implementation time.
These source checks are targeted. They are not a complete audit of the repository.

## Official external sources

| ID | Source | Use and limit |
|---|---|---|
| E1 | [OpenAI: Using Goals in Codex](https://developers.openai.com/cookbook/examples/codex/using_goals_in_codex) | Confirms the /goal completion-contract pattern and supported command lifecycle. Check the installed build. |
| E2 | [Anthropic: Keep Claude working toward a goal](https://code.claude.com/docs/en/goal) | Confirms /goal syntax and the 4,000-character condition limit. The evaluator does not independently run tests. |
| E3 | [Docker: container pause](https://docs.docker.com/reference/cli/docker/container/pause/) | Docker pause suspends container processes. This alone does not certify Jackin's complete source-capture procedure. |
| E4 | [Pkl language reference](https://pkl-lang.org/main/current/language-reference/index.html) | Typed modules, constraints, and amendments provide the proposed schema structure. |
| E5 | [pklr source from the earlier audit](https://github.com/jdx/pklr/blob/ed746fcf308c5510e159064814fafb5dd151ac44/README.md) | Describes evaluator capabilities and evaluator-only use. Recheck the version chosen for implementation. |
| E6 | [ASD-STE100 official site](https://www.asd-ste100.org/) | Identifies the controlled technical language used as the writing target. |
| E7 | [ASD-STE100: About STE](https://www.asd-ste100.org/about_STE.html) | Official search results identify Issue 9 from 15 January 2025. |
| E8 | [Anthropic: subagents](https://code.claude.com/docs/en/sub-agents) | Supports the use of existing subagent functions. It does not prove Jackin workflow isolation. |

The official full STE PDF could not be retrieved through the browser in this update.
Do not treat the sentence-length check as full dictionary compliance certification.
The pack uses short active sentences and the defined technical terms in `TERMS.md`.

## Design decisions, not source claims

One active run, seven components, and the fixed repair sequence are new scope decisions.
The brief checkpoint pause, conservative delivery recovery, and early backup order are proposed implementation choices.
Their test obligations appear in `CHECKS.md`.
No source citation replaces those live checks.
