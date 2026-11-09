<!-- SPDX-FileCopyrightText: 2026 Alexey Zhokhov -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# S6 static documentation verification

The documentation builds a complete static artifact with stable Fumadocs and Vite dependencies. The browser downloads the real generated search index directly from `/api/search.json`; it needs no server rewrite or application server. Development and preview commands serve the built static output. The CLI reference was generated from the published e1fb608b executable and retains SHA-256 `d902f830a57f750821eea2b1f3698f3304f0f2510c09967a888b8339b3e8f554`.

The source and tool identities are recorded in [source-manifest.json](S6-static-docs-2026-11-09/source-manifest.json) and [browser-provenance.json](S6-static-docs-2026-11-09/browser-provenance.json). The application patch is based on e1fb608b; the later verification-only correction requires the established 404 document's title and heading text, independently of the generated reference body.

## Executed checks

- The restored native build and type check exited 0. The public HTTP check passed for all 31 documentation pages: 33 HTML documents, 31 Markdown files and 31 WebP files. It checks titles, canonicals, referenced assets, sitemap, LLM and search membership, missing resources and exact not-found response status/media/body.
- An independent reviewer built on the root-produced default-endpoint fault and ran the actual browser: `/api/search` redirected to `/api/search/`, returned 404, and the settled `construct` query had no result. Other recorded assets succeeded. This is a behavioral failure, not a browser setup or compilation failure.
- The independent identical-driver run on restored source exited 0: real search hit and miss, result navigation, direct nested reload and title, home navigation, Back and Forward, and no captured runtime/hydration errors. An OS process snapshot found no remaining process matching the private browser package or unique profiles.
- Independent output corruption controls detected wrong page mapping, missing anchor content, a missing referenced script, missing search membership, wrong unknown-route status and differing 404 content. A further counterexample exposed an oracle gap: both 404 responses could be the same home document. The corrected verifier now independently requires the not-found title and heading text; its independent counterexample witness rejects that same corruption at the not-found title assertion and passes on restored output. The [retained reviews](S6-static-docs-2026-11-09/independent-reviews.json) preserve the initial HOLDs, final witnesses and text-identity scope clarification.

Machine receipts in the adjacent directory retain actual child results and primary receipt hashes. The HTML parser checks document identity text; it does not compute CSS visibility. CDP request interception applies to page requests, not general OS network containment. No approved terminal baseline image was changed.

## Hosting and scope

Read-only GitHub Pages metadata identifies `jackin.tailrocks.com`, HTTPS enforcement, workflow publication and `main` as its source. DNS resolves the domain to `jackin-project.github.io`. The artifact provides ordinary static file paths, `index.html`, `404.html`, `CNAME` and `.nojekyll`; search no longer depends on the local `serve` configuration being understood by the host. This establishes the chosen static artifact contract, not deployment of this unmerged source. [GitHub's custom 404 documentation](https://docs.github.com/en/pages/getting-started-with-github-pages/creating-a-custom-404-page-for-your-github-pages-site) describes the host's `404.html` convention.

This evidence does not complete the entire migration or replace final-head G-FULL, D27, real J00–J17, three lanes, Architect live proofs or the five-repository final reviews. The original four pull requests remain open and ready for the operator.
