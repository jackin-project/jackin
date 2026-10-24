# Rendered documentation verification

## Result

**PASS, bounded to the tested build and browser.** The final parameterized script ran from 2026-10-24 08:42:57 to 08:43:27 UTC and exited 0. It checked **56 routes at desktop 1440×1000 and mobile 390×844: 112/112 page checks passed**. All **718 distinct internal URLs passed HTTP checks**, including **657 fragments checked against the rendered DOM**. Two actual, scoped sidebar navigations reached the expected destination title. No captured console/page error, local request failure or page-wide horizontal overflow occurred. All **58 recorded MDX/config input hashes matched before and after** the sweep.

The script checks each route's visible H1 against its frontmatter title, useful body content, expected Current marker, navigation visibility, page-wide overflow and the absence of the previously literal Mermaid source. On desktop it requires visible navigation with links; on mobile it requires the visible sidebar trigger. Actual sidebar opening/link navigation is exercised on the context overview at each viewport, not every branch. Link availability here is internal to the built static site; it is not an external-source availability claim.

The final exact script, result and argv are [probes/render_pages.mjs](probes/render_pages.mjs), [rendered-pages-results.json](rendered-pages-results.json) and [rendered-qa-command.json](rendered-qa-command.json). Build hashes are in [rendered-build-input-manifest.json](rendered-build-input-manifest.json). This is a rendering check of the coordinator's final build, not a substitute for its separate TypeScript/test/sidebar/repository-link/research gates.

## Visual review

**22 representative captures** cover the context overview, Headroom dossier, architecture, adoption roadmap, multimodal economics and cross-agent portability at both viewports. All eleven desktop captures were directly inspected by `/root/headroom_lemoncrow`; all eleven mobile captures were independently inspected by `/root/caveman_ponytail`. The final parameterized run produced nineteen byte-identical images and three different table captures; the latter three were directly reopened and inspected by `/root/headroom_lemoncrow`. The changed mobile capture also received independent reinspection, so all eleven final mobile images have independent visual coverage. Thus the final 22-image inventory is backed by exact identity or actual reinspection, not an assumption that a rerun looks the same.

Visible headings, prose and table cells are readable, navigation chrome is present, and no overlap or page-wide clipping was observed. Mobile wide tables remain bounded inside their local scroll region. Static captures do not prove touch/horizontal-scroll interaction or visibility of every table column. Table-region screenshots intentionally crop surrounding sections at the viewport edge. This is representative visual inspection, not inspection of every pixel on all 56 pages.

The independent reviewer initially reported missing icons in two mobile images, then withdrew the concern after individually reopening the unchanged hashed files. Both controls were present; this was an image-reading error, not an application defect or a repaired capture. The original independent record preserves that correction. See [rendered-qa-mobile-independent-review.json](rendered-qa-mobile-independent-review.json) and [rendered-qa-screenshots.json](rendered-qa-screenshots.json). Screenshots remain scratch intermediates, with filenames, hashes, dimensions/regions and review ownership recorded here.

## Harness corrections and preserved runs

The first full sweep correctly passed desktop pages but incorrectly demanded mounted navigation links while the mobile drawer was closed. Its interaction also queried mobile links before their mounted elements entered the viewport. Separately, waiting only for the URL could capture the previous H1 after client navigation. The harness now distinguishes closed mobile navigation, waits for the scoped mounted link, clicks that exact navigation element and waits for the target frontmatter title. Earlier independent script review also removed two possible false passes: ignoring the desktop navigation visibility field and clicking a duplicate link outside the intended sidebar.

The corrected sweep passed; the final sweep then proved the shipped parameterized version with the same checks and unchanged build. [rendered-qa-run-history.json](rendered-qa-run-history.json) indexes all three exact script versions, commands, full result JSON and logs. None of these harness corrections changed MDX or application source. Before these runs, the authored architecture Mermaid fence was replaced with a native table because this documentation site rendered the diagram source literally; that content fix was included in the coordinator's final build.

## Browser and scope

Node v24.19.0 and Playwright 1.62.1 drove official Chrome for Testing headless shell 156.0.8078.47. The first task-local Playwright browser-download attempt exited 1 on unusable archive responses. The approved fallback fetched the official Chrome metadata and archive, recorded HTTP status, size and calculated SHA256, validated the ZIP and checked the extracted version. No separately published checksum or signature was validated. Acquisition failure is separate from page verification.

[rendered-qa-environment.json](rendered-qa-environment.json) retains the actual paths, command, retrieval URLs/hashes and isolation limits. The browser and static server were task-local; the server bound only 127.0.0.1. Browser page requests outside the loopback origin were blocked, with none observed in the final run. No provider call, user session, optimizer activation or public service was used.

## Reproduce with existing local tools

Supply an existing built checkout, Playwright package, compatible Chromium executable and the **build-time** input hash manifest. A current source hash generated after an old build is not proof that the build matches. The manifest maps repo-relative input paths to SHA256, as in the included recorded manifest; regenerate it as part of a new build when intentionally verifying different inputs.

Start a loopback server in a separate terminal:

```bash
python3 -m http.server 4188 --bind 127.0.0.1 \
  --directory /absolute/path/to/jackin/docs/.output/public
```

Run the shipped script with explicit paths; it does not install or download anything:

```bash
node /absolute/path/to/render_pages.mjs \
  --repo /absolute/path/to/jackin \
  --manifest /absolute/path/to/build-input-manifest.json \
  --browser /absolute/path/to/chrome-headless-shell \
  --playwright-module /absolute/path/to/playwright/index.js \
  --origin http://127.0.0.1:4188 \
  --out /absolute/path/to/disposable-render-results
```

`QA_REPO`, `QA_MANIFEST`, `QA_BROWSER`, `QA_PLAYWRIGHT_MODULE`, `QA_ORIGIN` and `QA_OUT` are optional environment alternatives. Use the loopback origin without a trailing path/slash, as above. `--help` describes the inputs without launching a browser. Stop the server after inspection. Exact actual argv and runtime paths are retained separately in the command/environment records.

## Limits

This checks one Linux Chromium build and two viewport configurations, not Safari/Firefox, physical devices, all responsive widths, a full accessibility audit, every navigation branch, live external links or every interaction. Wide-table scrolling and keyboard/screen-reader behavior were not independently exercised. These results establish local rendered-page behavior only; they do not validate optimization quality, native host replacement or product performance.
