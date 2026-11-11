# T22 omp with OpenAI: blocked on this host

State: unavailable.
Reason: no API key on this host.
Key search of 2026-11-08, operator order (host omp store first, then 1Password, then the host agent login; record the source, never the value — rewrite/evidence/operator-order-2026-11-08.md):
- Host omp store: holds only a Z.AI record, so the omp-host source yields no OpenAI key (rewrite/evidence/A14-native-keys.md; rewrite/decisions.tsv D29 key-inventory rows of 2026-11-08).
- 1Password: no OpenAI API key. The OpenAI items are logins with empty password fields (rewrite/decisions.tsv D29 key-inventory rows of 2026-11-08: "no OpenAI API key found in 1Password or the host omp store").
- Host agent login: the ChatGPT login in `~/.codex/auth.json` (`OPENAI_API_KEY` unset, rewrite/evidence/P0.3-codex.md). It is not an API key, and D29 bars OpenAI API keys from Codex tuples.
The catalog row stays `Unavailable` with reason `no API key on this host` (crates/jackin-core/src/agent.rs).
To unblock: put an OpenAI API key in a credential source jackin reads (host omp store, environment, or 1Password). Then run `jackin account add codex --name omp-openai --agent omp --credential env:OPENAI_API_KEY` and the T21-T22 live gate of D29: live gateway evidence that the named key was used at api.openai.com and no other credential or paid fallback can occur. Then flip T22 to supported.
