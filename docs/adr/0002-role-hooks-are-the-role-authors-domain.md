# Role hooks are the role author's domain

Status: accepted

jackin runs role hooks (`setup-once.sh`, `source.sh`, `preflight.sh`) exactly as the role author wrote them and does not police, sandbox, rewrite, or rate-limit them. jackin's only responsibilities are to run them as the standard start flow and to hard-fail the start, surfacing the error, when a hook exits non-zero. Deterministic dependency installation is steered toward the role's own Dockerfile by documentation, not by any enforcement mechanism.

## Why this is recorded

The runtime-restore work is partly motivated by slow startups, and a tempting "fix" is to make jackin detect heavy hooks (e.g. `mise install`), warn, or run hooks in a no-network / read-only sandbox so runtime installs fail. We explicitly rejected all of that. A future contributor chasing startup speed will likely propose hook-policing again; this records that it is a deliberate non-goal, so the next person does not "fix" something that was decided.

## Considered options

- **Detect installer commands and warn (or `--debug` hard-fail).** Rejected: jackin should not judge hook contents; a hook is the author's chosen way to run things, and detection by text-scan is brittle while detection by timing punishes legitimately slow-but-correct hooks.
- **Run hooks in a restricted sandbox (no network / read-only rootfs) so runtime installs physically fail.** Rejected: it also breaks legitimate hooks that need network (preflight reachability checks) or disk writes (per-instance state), and constrains what roles can do at start.
- **Run hooks faithfully; hard-fail on non-zero exit; document intended use (chosen).** jackin reports failures faithfully and otherwise stays out of the way.

## Consequences

- A role that installs dependencies at runtime stays slow; that latency is the author's to own, by the author's choice. jackin's own contract (no runtime agent install, no runtime plugin install, image-baked default state) is unaffected and remains enforced.
- The intended split — `setup-once.sh` = per-instance state init, `source.sh` = env/PATH, `preflight.sh` = cheap validation, deterministic deps = role Dockerfile — is author-facing guidance, not a jackin-enforced rule.
- No new image-build hook is added; the role's own Dockerfile is the build-time surface.
