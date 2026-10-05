# Security and Codex Review

Status: IN PROGRESS

## Coordinator prerequisite

The coordinator model prerequisite is FAIL. Task section 2.1 explicitly supersedes older instructions and requires Luna/max. The active root process uses Sol/medium.

Work agents are tool-assigned Luna/max. Runtime confirmation remains pending. Delegation does not clear the coordinator failure.

## Codex schema and catalog

- The official [Codex configuration reference](https://developers.openai.com/codex/config-reference) defines `model_reasoning_effort` as a string.
- Supported reasoning levels depend on the selected model.
- Local Codex `0.160.0` catalog output reports Luna supports `max`.
- The same catalog reports Sol supports `max` and `ultra`.
- Current root settings are Sol/medium; task section 2.1 requires Luna/max.
- Runtime confirmation of agent settings remains pending.

## Preliminary security requirements

The preliminary review sets these gates:

- Review root and unprivileged execution boundaries before builds or live accounts.
- Review the exact container and authentication route.
- Treat Docker socket access as root-equivalent.
- Do not copy the complete Codex home.
- Record MBX cache and object provenance before compilation.

The `unprivileged_exec_design` owner is still working. The preliminary security review is complete, but execution remains gated.

## Review owners

| Owner | Work | State |
|---|---|---|
| `preflight_security_review` | Initial security gate | Initial review complete; follow-up pending |
| `unprivileged_exec_design` | Execution boundary | IN PROGRESS |
| `codex_schema_runtime` | Agent settings confirmation | IN PROGRESS |
| `branches` | Fetch passed; full diff review | IN PROGRESS |
| `execution_crosscheck` | Independent gate crosscheck | Complete; implementation acceptance NOT RUN |

See [checklist](checklist.md), [build results](build-results.md), and [Debian results](debian-results.md).
