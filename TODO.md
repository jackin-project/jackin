# TODO

Roadmap items — open work and resolved design docs — live in the docs
site, not in this repo. See:

- Overview: [`docs/src/content/docs/reference/roadmap.mdx`](docs/src/content/docs/reference/roadmap.mdx)
- Per-item design docs: [`docs/src/content/docs/reference/roadmap/`](docs/src/content/docs/reference/roadmap/)
- Browsable: <https://jackin.tailrocks.com/reference/roadmap/>

To add a new item, create an MDX page under the directory above and
add a sidebar entry in [`docs/astro.config.ts`](docs/astro.config.ts)
under `Roadmap → Open items`.

Each design doc should include (see any existing page as a template):

- `**Status**: Open | Deferred | Resolved`
- `## Problem`
- `## Why It Matters`
- `## Related Files`
