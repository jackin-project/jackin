# jackin-sentinel

Internal jackin❯ role for manual smoke checks and future PTY, functional, and
Docker E2E coverage.

The canonical manual-test repository is:

```sh
https://github.com/jackin-project/jackin-sentinel
```

Load it from any workspace with:

```sh
jackin load jackin-project/sentinel . --rebuild --debug
```

The role intentionally concentrates the valid role-manifest surface area in one
place:

- every supported agent table
- runtime hooks: `setup_once`, `source`, and `preflight`
- static env defaults
- rich text prompts
- rich select prompts
- skippable prompts and skip cascading
- dependency ordering and `${env.*}` interpolation
- Docker build output from a real `RUN` layer

At runtime the hooks install `jackin-sentinel-report` into the agent user's
`PATH`. Run it from the container to print a deterministic report bounded by
`JACKIN_SENTINEL_REPORT_BEGIN` and `JACKIN_SENTINEL_REPORT_END`. Future E2E
tests should parse that report rather than depending on agent-specific UI text.

Manual verification checklist:

1. Confirm the launch cockpit owns the full screen and no raw Docker output is
   printed over it.
2. Complete every manifest env dialog: required text, defaulted text, select,
   defaulted select, skippable optional value, and dependency-derived values.
3. Open the Docker build log from the cockpit and confirm the
   `jackin-sentinel build layer` line appears there, not on the main screen.
4. Launch at least one agent and run `jackin-sentinel-report`.
5. Confirm the report shows resolved env values, hook markers, runtime
   variables, and Docker CLI availability.
6. Exit the container with a dirty isolated worktree and confirm cleanup is a
   rich dialog, not a plain numbered CLI prompt.

Recommended test tiers:

1. PTY tests drive the rich launch dialogs and assert the visible screen.
2. Functional tests run jackin❯ commands against this role with fake or stubbed
   external dependencies.
3. Docker E2E tests launch the role with a real Docker daemon and assert the
   sentinel report, hook markers, env resolution, and diagnostics/build output.
