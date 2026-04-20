# Landing Page Design

This design replaces the current minimal `docs/pages/index.mdx` (a `HomePage.Root` tagline + two buttons) with a full-bleed cyberpunk-native landing page that lives at the root of `jackin.tailrocks.com/`. Existing docs pages keep their current URLs unchanged.

A fully-working reference mockup is checked in at `docs/superpowers/mockups/landing-v2.html` — a single self-contained file (HTML + inlined CSS + vanilla JS, no build step). This spec describes the design intent and the Vocs integration plan; the mockup is the source of truth for pixel-level details.

## Goals

- Replace the current `docs/pages/index.mdx` with a rich cyberpunk-themed landing page under Vocs' `layout: landing` frontmatter.
- Preserve existing docs URLs so deep links and SEO continue to work unchanged.
- Teach a cold visitor what jackin' does, why it exists, and how to install it in a single scroll.
- Use typography, interaction patterns, and copy to carry the cyberpunk theme without literal cliches (digital rain is present but restrained; vocabulary is fully integrated into the product naming).
- Match real jackin' conventions (base image name, CLI selector vs GitHub repo, manifest structure) so the landing doubles as accurate product documentation.

## Non-Goals

- Moving docs to a `/docs/` prefix. All existing docs URLs remain unchanged.
- Adding marketing-adjacent surfaces (blog, changelog, case studies). Landing is one page; if those come later they'll be a separate design.
- A signup form, newsletter, or gated content. jackin' is open source; the primary conversion is `brew install jackin`.
- Hypervisor-level security claims. The landing omits the honest-tradeoffs security section that lives in `docs/pages/guides/security-model.mdx`.

## Current Behavior

`docs/pages/index.mdx` today renders via Vocs' `<HomePage.Root>`:

```mdx
---
layout: landing
---

import { HomePage } from 'vocs/components'

<HomePage.Root>
  <HomePage.Logo />
  <HomePage.Tagline>Isolate AI coding agents in Docker containers...</HomePage.Tagline>
  <HomePage.Buttons>
    <HomePage.Button href="/getting-started/why" variant="accent">Get Started</HomePage.Button>
    <HomePage.Button href="https://github.com/jackin-project/jackin">GitHub</HomePage.Button>
  </HomePage.Buttons>
</HomePage.Root>

## Why jackin'?
...
```

It's a small logo + tagline + two buttons followed by prose sections. Functional but thin — doesn't establish the cyberpunk theme, doesn't demonstrate the product, and doesn't use the vocabulary strategically.

## Chosen Approach

Option 3 from the placement discussion: **landing lives at `/`, docs URLs unchanged.** This is the path Vocs' `layout: landing` was designed for.

The new `docs/pages/index.mdx` keeps `layout: landing` frontmatter but replaces the body with a set of React components that render the eight sections plus the footer wordmark. All styling is Tailwind v4 (matching `docs/pages/_root.css`'s CSS-first setup). The reference mockup uses inlined CSS for iteration speed; the Vocs implementation converts it to Tailwind utilities + a small set of custom CSS for animations and the composition machine.

## Page Structure

Eight sections plus a full-width footer wordmark. Each section has a single editorial job; vocabulary and visual motifs are reused across sections rather than reintroduced.

### 01. Hero

Full-viewport (`min-height: 100vh`) stage with four layers:

1. **Canvas rain** behind everything — faithful port of `src/tui.rs`'s `digital_rain` algorithm (same ASCII char pool, same age-based color gradient from `WHITE` through `MATRIX_DARK`, same 35ms frame rate). Canvas opacity 0.32 with a radial vignette overlay.
2. **Top nav** (no border, thin gradient background, `backdrop-filter: blur`) with logo + "★ Star on GitHub" link. Nav sits on top of the rain so the rain appears from viewport top.
3. **Centered hero content** — Inter 800 headline ("You're the **Operator**. They're already inside."), deck paragraph, primary CTA button, eyebrow row.
4. **Interactive code panel on the right** — tabbed terminal with `$ load`, `$ hardline`, `$ eject`. Click any tab and the body retypes that command's animated output. Loops.

### 02. Vocabulary (scroll-driven dictionary)

Section is 500vh tall. A sticky container holds a two-panel UI: a left rail with nine entries (Operator, The Construct, Agent class, Workspace, Jacking in, The agent inside, Hardline, Pulling out, Exile) and a right detail panel that updates as the viewer scrolls through the section. Clicking any rail item jumps to that entry via smooth-scroll.

Each entry shows: big Fraunces-serif term, italic part-of-speech, em-dash-led definition, optional accent-green `CLI` / `IMAGE` / `DECLARES` callout.

### 03. Pills (red pill / blue pill)

Two cards side-by-side. Each card has:

- A CSS-drawn two-tone capsule pill (blue or red gradient + white half + glossy highlight).
- A **Blue pill** label (babysit-every-prompt = productivity destroyed) or **Red pill** label (full-YOLO-on-host = risk maximum).
- Four bullet points explaining why that option is bad.
- A color-coded verdict pill.
- Four stacked box-shadow layers per card, producing an emissive "light" effect around each pill.

Closing line: "Refuse the pill. **You're the Operator** — define the construct instead."

### 04. The Approach (two routes)

Two cards with green dashed accents. Both are "good" options (unlike the pills which are both bad):

- **Route 01 · Reuse** — prose describing the-architect + a chip row of what it adds on top of the Construct (Rust stable, cargo-nextest, cargo-watch, code-review, feature-dev, superpowers, jackin-dev).
- **Route 02 · Build** — static tabbed builder with two tabs: `jackin.agent.toml` (manifest with identity + plugins + marketplaces) and `Dockerfile` (mise install, USER root / apt-get / USER claude pattern). A `Self-contained ✓` tag in the chrome.

Each card shows the CLI command and the GitHub repo path separately to teach the `owner/agent` (selector) vs `owner/jackin-agent` (repo) convention.

### 05. Cast

Three character cards (Agent Smith / Agent Jones / Agent Brown) plus a full-width dashed invite strip below them. Each character card shows:

- 2-letter avatar (AS / AJ / AB).
- Small uppercase role label in accent green ("General-purpose" / "Backend engineer" / "Frontend engineer").
- Fraunces 28px character name.
- Single-line tagline.

The invite strip has a dashed `+` avatar, a title ("Cast your own role."), a one-sentence description, and a CTA button linking to `https://jackin.tailrocks.com/developing/creating-agents`.

### 06. Mental Model (composition machine)

An interactive machine with three vertical panels (Agent Class × Workspace = Running Agent). Above it, a row of org tabs (jackin-project / chainargos / your-org) scopes the visible options. Clicking radios in the first two panels rewrites the preview panel; unauthorized combinations show a red rejection state.

Below the machine, a two-card Kitchen-Sink vs Role-Specific callout makes the editorial argument ("too much context = worse decisions" vs "focused context = better results").

### 07. How it Works (the daily loop)

Five vertical frames, each a two-column grid: info left (mono number, Fraunces command name, italic mythos line, description paragraph) and full terminal window right (traffic dots + colored output).

Frames:

1. `load` — Jacking in. (shows `cd ~/Projects/my-app` then `jackin load agent-smith`)
2. `clone` — More of me. (shows two `cd` + `jackin load` cycles on different paths)
3. `hardline` — The hardline.
4. `eject` — Pulling out.
5. `exile` — Casting out. (shows exiling three agents by name)

No connecting filmstrip line; a thin 1px dashed border separates frames.

### 08. Jack in (install)

Minimal CTA section:

- `08 · Jack in` section label.
- One-word title: "Install."
- One-line intro: "Homebrew on Mac and Linux. Tap, install, load — you're in."
- Three-line install block (`brew tap`, `brew install`, `jackin load`).
- Two buttons: `Read the Docs →` and `★ Star on GitHub`, linking to real URLs.

### Footer wordmark

A massive Inter 900 "jackin'" centered at the bottom (clamp 120px → 300px), with a green accent apostrophe. Small mono meta row above (GitHub · Docs · Apache 2.0).

## Visual Design System

### Palette

| Token | Value | Use |
|---|---|---|
| `--bg` | `#0a0b0a` | Primary page background |
| `--bg-deep` | `#050605` | Hero stage, deeper surfaces |
| `--panel` | `#0f1110` | Card and terminal backgrounds |
| `--text` | `#f4f7f5` | Primary text |
| `--text-dim` | `#9ca8a1` | Secondary text, descriptions |
| `--text-ghost` | `#5e6a64` | Tertiary text, metadata |
| `--accent` | `#00ff41` | Brand accent (phosphor green) |
| `--danger` | `#ff5e7a` | Denial states, `✕` markers |
| `--ui` | `rgba(244,247,245,0.1)` | Default borders |
| `--ui-strong` | `rgba(244,247,245,0.22)` | Emphasized borders |

Green is used sparingly: only on the *Operator* word in the hero tagline, logo mark, terminal prompts, success marks, and small labels. Never on secondary UI chrome like buttons or pills.

### Typography

Three typefaces, each with one job:

- **Inter** (weights 400, 500, 600, 700, 800, 900) — body text, headlines, buttons, UI.
- **JetBrains Mono** (400, 500, 600) — CLI commands, code, metadata labels, section labels, terminal output.
- **Fraunces** serif (400, 500, 700) — named entities: vocabulary dictionary terms, cyberpunk-genre names, daily-loop verbs. Fraunces appears only on "names of things."

### Interaction patterns

- **Tabs** — used in the hero code panel (commands) and Section 04 builder (manifest vs Dockerfile). Single lightweight pattern, reused.
- **Scroll-driven rail + detail** — used in Section 02 Vocabulary. Section is tall (500vh); sticky UI advances based on scroll progress; rail items click-to-jump.
- **Radio selector machine** — used in Section 06 Mental Model. Three panels, click radios in any two panels to update the third. Denied combinations show an error state instead of a preview.
- **Hover lift** — cards rise by 2px on hover with a stronger border color and a soft shadow bloom. Used on pill cards, approach cards, cast cards, template invite strip.

### Motion + accessibility

- Digital rain, hero typing animation, and Section 04 builder all respect `prefers-reduced-motion: reduce` with static fallbacks rendered immediately.
- Scroll-driven section degrades gracefully on narrow viewports: below `820px` the sticky behavior is disabled and entries render as a stacked list.
- All CTA links are focusable with visible outlines (default browser outline preserved).
- Color contrast targets WCAG AA for all text (accent green on the dark background satisfies AAA).

## Technical Constraints

- **Vocs 1.4+** with React 19.2 (per `docs/package.json`).
- **Tailwind v4** with CSS-first config in `docs/pages/_root.css` (per `docs/AGENTS.md`).
- **Bun** is the only supported package manager.
- **`layout: landing` frontmatter** on the new `docs/pages/index.mdx` strips the Vocs docs chrome (sidebar, right rail) from the page.
- **No external dependencies beyond what Vocs already ships.** Fonts are loaded via Google Fonts `<link>` or self-hosted woff2 (decision deferred to implementation).

## Component Inventory

The implementation should produce a small set of React components placed under `docs/components/landing/` (new directory):

| Component | Responsibility |
|---|---|
| `HeroStage` | Full-viewport rain + nav + content container |
| `DigitalRain` | Canvas element running the `src/tui.rs` port |
| `HeroContent` | Tagline + deck + CTAs + interactive code panel |
| `CodePanel` | Tabbed terminal with typing animation (used in hero) |
| `VocabularyDictionary` | Scroll-driven rail + detail panel (Section 02) |
| `PillCards` | Red/blue pill cards (Section 03) |
| `ApproachCards` | Two-route cards with chips + tabbed builder (Section 04) |
| `TabbedBuilder` | Generic tab + body component for manifest/Dockerfile |
| `CastRoster` | Three character cards + invite strip (Section 05) |
| `CompositionMachine` | Org tabs + three-panel radio selector + preview (Section 06) |
| `FocusCallout` | Kitchen-Sink vs Role-Specific callout (Section 06) |
| `DailyLoop` | Five vertical frames with info + terminal (Section 07) |
| `InstallBlock` | Three-line install recipe + two CTAs (Section 08) |
| `WordmarkFooter` | Meta row + big "jackin'" wordmark |

Each component is self-contained: owns its data (or receives it via props), owns its styles, owns its state.

## Implementation Notes

- Convert the mockup's inline CSS to Tailwind v4 utilities where possible. Custom CSS stays for: keyframe animations, the digital rain canvas styling, the composition machine grid layout, and the pill-capsule visual (complex gradient stack).
- The composition machine's org/class/workspace data should live in a single TypeScript module (`docs/components/landing/machineData.ts`) so it's easy to update without touching UI code.
- The `digital_rain` port should extract the algorithm into a reusable module with configurable density, colors, and frame rate. Future sections (or a 404 page, or the docs sidebar backdrop) may want to reuse it at lower intensity.
- The scroll-driven vocabulary section uses `requestAnimationFrame` throttling; don't regress that when converting to React. Use `useEffect` with a scroll listener + RAF, not a third-party scroll library.

## Open Questions

- Font loading strategy: Google Fonts `<link>` is the fastest path; self-hosted woff2 is better for long-term control. Defer to implementation.
- Do we want the scroll-driven Vocabulary section to also drive a URL hash update as the active entry changes? Lets visitors share a deep link to a specific entry. Nice-to-have; not required for v1.
- Should the hero's `★ 1.2k` star count be dynamic (fetch from GitHub API on page load)? Dynamic is more accurate; static is simpler and faster. Defer to implementation — start with static "★ Star on GitHub" text (no count), add dynamic count later if it's a valuable signal.

## References

- Reference mockup: `docs/superpowers/mockups/landing-v2.html` (1,768 lines, self-contained)
- Rain algorithm source: `src/tui.rs` (functions `digital_rain`, `age_to_color`, `should_mutate`, constants `RAIN_CHARS`, `MATRIX_GREEN`, `MATRIX_DIM`, `MATRIX_DARK`)
- Agent class structure references: `docs/pages/developing/creating-agents.mdx`, `docs/pages/developing/agent-manifest.mdx`, `docs/pages/developing/construct-image.mdx`
- Vocabulary references: `docs/pages/getting-started/why.mdx` (Matrix vocabulary table), `docs/pages/getting-started/concepts.mdx`
- Vocs docs site config: `docs/vocs.config.ts` (rootDir, sidebar, editLink pattern stay unchanged)
