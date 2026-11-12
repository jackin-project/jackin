# A9 final proof — G-E2E on the real gateway

Date: 2026-11-06. Branch: feature/openshell-native. Gateway: jackin-local (OpenShell 0.1.7).
Role: jackin-sentinel at ac4c595. Jackin head for the run: 79a948d4b plus the A9 role and probe fixes below.
No credential printed.

## Verdict: PASS

## Final run

```
mise exec -- mbx +1.99.1 xtask e2e --gateway jackin-local --role ../jackin-sentinel --builder orbstack --platform linux/arm64 --engine-port 2376 --engine-certs ~/.local/share/jackin-e2e-engine/client
J07 PASS     8.5s codex is the child of jackin-start, exec answered, restart gave a new pid
J09 PASS     0.1s codex reached its API, outside and metadata denied, provider rules rejected
RESULT PASS=19 FAIL=0 SKIP=0 elapsed=201.6s a1_host_login=unchanged
```

All 19 journeys pass. The D24 test engine is recorded in `rewrite/evidence/A9-engine.md`.

## Defects found by the first A9 runs and fixed

1. Role Codex model pin. Sentinel, the-architect, and agent-smith pinned `model = "gpt-5.1-codex"` in `[agents.codex]`. Codex rejects that model on a ChatGPT subscription account (exit 1, no answer), so J07 failed. The roles drop the pin and Codex uses the account default model: sentinel 3f84513, the-architect 65d8997, agent-smith 2c04a68. Live repro: with the pin, `codex exec` failed with `The 'gpt-5.1-codex' model is not supported when using Codex with a ChatGPT account.`; without the pin, the same exec answered `ready` with exit 0. Decision row 2026-11-06T15:00:00Z in `rewrite/decisions.tsv`.

2. Probe J09 provider check. The probe required an HTTP 401 from api.openai.com for a curl request that carries the placeholder. The provider profile allows only `/usr/local/bin/codex` to reach that host, so curl is refused at connect time (curl exit 7, HTTP 000) and the check could never pass. Granting curl egress is not a fix: with a temporary curl endpoint, the gateway resolved the placeholder to the real ChatGPT credential for curl and the answer was HTTP 403 (missing `api.model.read` scope). That breaks D10. The probe now passes the check when the policy refuses the connection, because a refused request never leaves the sandbox and no credential is applied. Timeout (curl exit 28) and DNS failure (curl exit 6) still fail. Sentinel ac4c595. Decision row 2026-11-06T15:40:00Z in `rewrite/decisions.tsv`.

## Red and green proof for the probe change

- Test plan: approved by @"tailrocks-test-reviewer (agent)" in round 2 (round 1 added the timeout case, the unresolved case, and the Codex conjunction).
- Red: the strengthened `j09_provider_refused` case failed on the unmodified probe with the old detail `api.openai.com gave no HTTP response, so the probe cannot show that no credential was applied`; the two new cases passed. Suite: 66 cases, 1 failed.
- Green: after the probe change, egress.sh 66 cases, hooks.sh 23 cases, and j08.sh 202 cases all pass, and `jackin role check .` exits 0.
- Independent red/green verification: see the A9 closure entry in `rewrite/PLAN.md`.

## Tuple evidence

- T01 Codex (OpenAI): this file plus `rewrite/evidence/A9-codex.md` (product-route launch, file proof, placeholder-only auth).
- T09 omp (Z.AI): `rewrite/evidence/A4-omp-zai.md`, section `A9 re-proof (2026-11-06)`.
