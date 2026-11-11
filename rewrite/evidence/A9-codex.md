# A9 live proof — T01 Codex (OpenAI, Supported)

Date: 2026-11-06. Branch: feature/openshell-native. Gateway: jackin-local.
Account: codex-local (credential source `codex-login`). Role: sentinel at jackin-sentinel (feature/openshell-native), `[agents.codex]` model `gpt-5.1-codex`.
Build: `mise exec -- mbx +1.99.1 build -p jackin` PASS.
No credential printed.

## Verdict: PASS (with model finding below)

## Launch

Temp workspace `a9-codex` registered for the sentinel path (removed after the run by direct config edit).

```
DOCKER_CONTEXT=default ./target/debug/jackin load sentinel /Users/donbeave/Projects/jackin-project/jackin-sentinel --agent codex --account codex-local --gateway jackin-local --task "In the working directory, create a file named a9-codex-proof.txt containing exactly the line: a9-codex-ok. Then stop."
```

Result: image built, `provider: skipped, the provider exists`, sandbox ready.
Sandbox: `jackin-a9-code-2u1o` (session `3qwmsrp2zf24zecyivelay223d`).
`agents.json`: `[{"agent":"codex","provider":"codex","account":"codex-local","model":"gpt-5.1-codex","branch":"agent/codex"}]`.
Lead Codex process (PID 691, child of jackin-start PID 374) running at rest; task delivered at `/sandbox/.jackin/task/task.md`.

## File proof

Driven through `openshell sandbox exec` (same drive pattern as A3M-live step 2), workdir `/sandbox/project`, `HOME=/sandbox/.jackin/homes/codex`:

- Codex (codex-cli 0.166.0, provider: openai) ran `printf 'a9-codex-ok\n' > a9-codex-proof.txt`, reported `Created a9-codex-proof.txt containing exactly a9-codex-ok.`, tokens used 3,248, exit 0.
- Verified in sandbox: `od -c /sandbox/project/a9-codex-proof.txt` = `a 9 - c o d e x - o k \n` (12 bytes, exact).

## Billing basis / provider

`openshell provider list` on gateway jackin-local: `jackin-codex-local-codex`, TYPE `codex`, 2 credential keys. The run billed the ChatGPT login behind that provider (account codex-local, source `codex-login`); Codex reported `provider: openai`.

Sandbox `/sandbox/.jackin/homes/codex/.codex/auth.json` (468 bytes): placeholder only — verdict PASS. `OPENAI_API_KEY` null, `access_token` is an `openshell:resolve:env:..._CODEX_AUTH_ACCESS_TOKEN` placeholder reference (not a real token), `refresh_token` empty, `auth_mode` `chatgptAuthTokens`. No real token in the sandbox file.

Host `~/.codex/auth.json` unchanged: mtime `1793289055` (Oct 29 22:50:55 2026), size 4250, identical before and after the run.

## Finding (model)

Role-declared `gpt-5.1-codex` is rejected on this ChatGPT account: `codex exec -m gpt-5.1-codex` returned `ERROR: The 'gpt-5.1-codex' model is not supported when using Codex with a ChatGPT account.` The successful proof run used the account default model (Codex reported `model: gpt-6.1-sol`). The T01 route works; the sentinel role's pinned Codex model does not match this ChatGPT account.

## Cleanup

Sandbox purged with `jackin purge --force jackin-a9-code-2u1o`; temp workspace entry `a9-codex` removed from host config by direct edit. Not committed.
