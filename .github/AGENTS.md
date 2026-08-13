# GitHub workflow guidance

- Keep one workflow shape for Velnor, GitHub, and `both` lanes.
- Secret-bearing writer jobs default to pinned `ubuntu-26.04`; Velnor may run
  only read-only comparison work for those workflows.
- Install tools through mise and commit `mise.lock`.
- Pin every third-party action to a full commit SHA.
- Keep permissions least-privilege, concurrency bounded, and every job timed out.
- Preserve identical validation semantics across lanes.
- Keep registry and repository mutation single-writer.
- Use ref-independent compatibility cache prefixes so trusted main seeds PRs;
  PR caches remain merge-ref scoped and successful runs save fresh generations.
- Keep cache state workspace-owned; never repair permissions with `sudo`.
