# Landing Page Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `docs/pages/index.mdx` with a Matrix-native landing page at the root of `jackin.tailrocks.com/`, built as React components under Vocs' `layout: landing` frontmatter. All existing docs URLs remain unchanged.

**Architecture:** Build one React component per section of the approved mockup, plus a thin wrapper that composes them. Pure logic (digital rain algorithm, scroll progress math) lives in plain TypeScript modules with unit tests. Styling is Tailwind v4 utilities where possible, custom CSS for complex motifs (rain canvas, pill capsules, composition machine grid, scroll-driven section).

**Tech Stack:** Vocs 1.4.1 (React 19.2, Vite-based), Tailwind v4 (CSS-first config in `docs/pages/_root.css`), Bun 1.2+ for package management and tests, TypeScript 6.

**Source of truth:** `docs/superpowers/mockups/landing-v2.html` — the approved reference mockup (1,768 lines, self-contained HTML/CSS/JS). Each component task references specific sections of this file.

**Design spec:** `docs/superpowers/specs/2026-04-19-landing-page-design.md`.

---

## File Structure

New files under `docs/components/landing/` (directory doesn't exist yet):

```
docs/components/landing/
  Landing.tsx                 — composes all sections
  HeroStage.tsx               — full-viewport hero container with rain + nav
  DigitalRain.tsx             — React wrapper around <canvas>
  rainEngine.ts               — pure rain algorithm (ported from src/tui.rs)
  rainEngine.test.ts          — unit tests for rain engine
  HeroContent.tsx             — tagline + deck + CTA + code panel column
  CodePanel.tsx               — tabbed terminal with typing animation
  VocabularyDictionary.tsx    — scroll-driven rail + detail panel
  vocabularyData.ts           — 9 dictionary entries
  PillCards.tsx               — red/blue pill cards (Section 03)
  ApproachCards.tsx           — two-route cards (Section 04)
  TabbedBuilder.tsx           — manifest/Dockerfile tabs (used in Approach)
  CastRoster.tsx              — 3 character cards + invite strip (Section 05)
  CompositionMachine.tsx      — org × class × workspace picker (Section 06)
  machineData.ts              — org/class/workspace data
  FocusCallout.tsx            — kitchen-sink vs role-specific (Section 06)
  DailyLoop.tsx               — 5 vertical frames (Section 07)
  loopData.ts                 — loop entry data
  InstallBlock.tsx            — 3-line install + CTAs (Section 08)
  WordmarkFooter.tsx          — meta row + big wordmark
  styles.css                  — custom CSS for things Tailwind can't do cleanly
```

Modified files:

- `docs/pages/index.mdx` — replaced with `<Landing />` import under `layout: landing` frontmatter
- `docs/pages/_root.css` — imports `../components/landing/styles.css`, adds design-token CSS custom properties
- `docs/package.json` — add `bun test` script
- `docs/tsconfig.json` — ensure `"moduleResolution"` and paths work with new components directory (should need no change, but verify)

Unchanged:

- All other `docs/pages/**/*.mdx` files
- `docs/vocs.config.ts` (sidebar, editLink, baseUrl all unchanged)

---

## Task 1: Scaffold landing directory and verify Vocs renders a stub

**Files:**
- Create: `docs/components/landing/Landing.tsx`
- Modify: `docs/pages/index.mdx`

- [ ] **Step 1: Create the landing directory and a stub component**

```bash
mkdir -p docs/components/landing
```

```tsx
// docs/components/landing/Landing.tsx
export function Landing() {
  return (
    <div className="landing-root">
      <h1>jackin' landing — stub</h1>
      <p>This confirms the component is mounted. Tasks 2+ will build the real page.</p>
    </div>
  );
}
```

- [ ] **Step 2: Replace index.mdx with the stub**

```mdx
<!-- docs/pages/index.mdx -->
---
layout: landing
---

import { Landing } from '../components/landing/Landing'

<Landing />
```

- [ ] **Step 3: Start dev server and confirm the stub renders**

```bash
cd docs && bun install --frozen-lockfile && bun run dev
```

Expected: Dev server starts on a port (Vocs logs it). Open the URL, see "jackin' landing — stub" with no Vocs chrome (because `layout: landing`).

- [ ] **Step 4: Commit**

```bash
git add docs/components/landing/Landing.tsx docs/pages/index.mdx
git commit -m "landing: scaffold Landing component and mount under layout: landing"
```

---

## Task 2: Add design tokens and base styles

**Files:**
- Modify: `docs/pages/_root.css`
- Create: `docs/components/landing/styles.css`

- [ ] **Step 1: Create the landing stylesheet with design tokens**

```css
/* docs/components/landing/styles.css */

:root {
  /* Landing page design tokens — mirrors the mockup */
  --landing-bg: #0a0b0a;
  --landing-bg-deep: #050605;
  --landing-panel: #0f1110;
  --landing-text: #f4f7f5;
  --landing-text-dim: #9ca8a1;
  --landing-text-ghost: #5e6a64;
  --landing-accent: #00ff41;
  --landing-danger: #ff5e7a;
  --landing-ui: rgba(244, 247, 245, 0.1);
  --landing-ui-strong: rgba(244, 247, 245, 0.22);
}

/* When landing is the whole page (layout: landing), override body */
.landing-root {
  background: var(--landing-bg);
  color: var(--landing-text);
  font-family: 'Inter', system-ui, sans-serif;
  -webkit-font-smoothing: antialiased;
  min-height: 100vh;
  position: relative;
}

.landing-root * { box-sizing: border-box; }

.landing-shell {
  max-width: 1280px;
  margin: 0 auto;
  padding: 0 40px;
}

/* Dotted grid backdrop behind the page */
.landing-root::before {
  content: "";
  position: fixed;
  inset: 0;
  z-index: 0;
  pointer-events: none;
  background-image: radial-gradient(rgba(244,247,245,0.035) 1px, transparent 1px);
  background-size: 28px 28px;
  mask-image: radial-gradient(ellipse at center top, black 35%, transparent 85%);
}

.landing-section {
  padding: 112px 0;
  position: relative;
}

/* Section label: "02 · Vocabulary" style */
.landing-sec-label {
  font-family: 'JetBrains Mono', monospace;
  font-size: 11px;
  letter-spacing: 0.2em;
  text-transform: uppercase;
  color: var(--landing-text-ghost);
  margin-bottom: 14px;
  display: inline-flex;
  align-items: center;
  gap: 10px;
}
.landing-sec-label::before {
  content: "";
  width: 22px;
  height: 1px;
  background: var(--landing-accent);
}

.landing-sec-title {
  font-size: clamp(28px, 3.6vw, 44px);
  font-weight: 700;
  letter-spacing: -0.028em;
  line-height: 1.1;
  margin: 0 0 18px;
  max-width: 780px;
  color: var(--landing-text);
}
.landing-sec-title .accent { color: var(--landing-accent); }

.landing-sec-intro {
  color: var(--landing-text-dim);
  font-size: 17px;
  line-height: 1.65;
  max-width: 680px;
  margin: 0 0 48px;
}
```

- [ ] **Step 2: Import landing styles and Google Fonts from `_root.css`**

```css
/* docs/pages/_root.css */
@import "tailwindcss";

@source "./";

@custom-variant dark (&:where([style*="color-scheme: dark"], [style*="color-scheme: dark"] *));

/* Landing page typography and tokens */
@import url("https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400;500;600&family=Inter:wght@400;500;600;700;800;900&family=Fraunces:opsz,wght@9..144,400;9..144,500;9..144,700&display=swap");

@import "../components/landing/styles.css";
```

- [ ] **Step 3: Update Landing.tsx to use `landing-root` and show the shell**

```tsx
// docs/components/landing/Landing.tsx
export function Landing() {
  return (
    <div className="landing-root">
      <div className="landing-shell">
        <section className="landing-section">
          <div className="landing-sec-label">00 · Placeholder</div>
          <h2 className="landing-sec-title">Design tokens <span className="accent">are live</span>.</h2>
          <p className="landing-sec-intro">If this paragraph looks right (Inter, dim gray, on a near-black background), Task 2 is complete.</p>
        </section>
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Restart dev server and verify typography**

```bash
cd docs && bun run dev
```

Expected: The stub page shows the dotted grid backdrop, the section label "00 · PLACEHOLDER" in mono, the title in Inter bold with green accent on "are live", and the paragraph in dim gray Inter. Fraunces, Inter, and JetBrains Mono all load from Google Fonts.

- [ ] **Step 5: Commit**

```bash
git add docs/components/landing/styles.css docs/pages/_root.css docs/components/landing/Landing.tsx
git commit -m "landing: add design tokens, base CSS, font imports"
```

---

## Task 3: Build InstallBlock (Section 08)

Starting with the simplest static section to establish the "write component → render in Landing → verify in browser → commit" loop.

**Files:**
- Create: `docs/components/landing/InstallBlock.tsx`
- Modify: `docs/components/landing/styles.css` (append install-block styles)
- Modify: `docs/components/landing/Landing.tsx`

Reference: mockup section 08 (around line 1125-1143 of `landing-v2.html`).

- [ ] **Step 1: Write the InstallBlock component**

```tsx
// docs/components/landing/InstallBlock.tsx
export function InstallBlock() {
  return (
    <section className="landing-section">
      <div className="landing-shell">
        <div className="landing-sec-label">08 · Jack in</div>
        <h2 className="landing-sec-title">Install.</h2>
        <p className="landing-sec-intro">Homebrew on Mac and Linux. Tap, install, load — you're in.</p>

        <div className="landing-install">
          <div className="landing-install-line"><span className="k">brew</span> tap jackin-project/tap</div>
          <div className="landing-install-line"><span className="k">brew</span> install jackin</div>
          <div className="landing-install-line"><span className="k">jackin</span> load agent-smith</div>
        </div>

        <div className="landing-install-ctas">
          <a className="landing-btn-primary" href="https://jackin.tailrocks.com/" target="_blank" rel="noopener">Read the Docs →</a>
          <a className="landing-btn-ghost" href="https://github.com/jackin-project/jackin" target="_blank" rel="noopener">★ Star on GitHub</a>
        </div>
      </div>
    </section>
  );
}
```

- [ ] **Step 2: Append install-block styles**

```css
/* Append to docs/components/landing/styles.css */

.landing-install {
  background: var(--landing-panel);
  border: 1px solid var(--landing-ui);
  border-radius: 10px;
  padding: 24px 28px;
  font-family: 'JetBrains Mono', monospace;
  font-size: 13px;
  color: var(--landing-text);
  line-height: 1.85;
  margin-top: 12px;
}
.landing-install-line .k { color: var(--landing-accent); }

.landing-install-ctas {
  margin-top: 48px;
  display: flex;
  gap: 12px;
  justify-content: center;
  flex-wrap: wrap;
}

.landing-btn-primary, .landing-btn-ghost {
  font-family: 'Inter', sans-serif;
  font-size: 14px;
  font-weight: 600;
  padding: 12px 22px;
  border-radius: 5px;
  cursor: pointer;
  letter-spacing: -0.01em;
  transition: transform 0.15s, background 0.15s;
  text-decoration: none;
  display: inline-block;
}
.landing-btn-primary {
  background: var(--landing-text);
  color: var(--landing-bg-deep);
  border: 1px solid var(--landing-text);
}
.landing-btn-primary:hover { transform: translateY(-1px); }
.landing-btn-ghost {
  background: transparent;
  color: var(--landing-text);
  border: 1px solid var(--landing-ui-strong);
  font-family: 'JetBrains Mono', monospace;
  font-size: 13px;
  font-weight: 500;
}
.landing-btn-ghost:hover {
  background: rgba(255, 255, 255, 0.04);
  border-color: var(--landing-text);
  transform: translateY(-1px);
}
```

- [ ] **Step 3: Mount InstallBlock in Landing**

```tsx
// docs/components/landing/Landing.tsx
import { InstallBlock } from './InstallBlock';

export function Landing() {
  return (
    <div className="landing-root">
      <InstallBlock />
    </div>
  );
}
```

- [ ] **Step 4: Verify in browser**

```bash
cd docs && bun run dev
```

Open the URL. Expected: the InstallBlock renders centered in the viewport: "08 · Jack in" label, "Install." title, "Homebrew on Mac and Linux..." paragraph, three monospace install lines with green `brew` / `jackin` keywords, two centered buttons at the bottom.

Compare side-by-side with `docs/superpowers/mockups/landing-v2.html` rendered Section 08 — they should match.

- [ ] **Step 5: Commit**

```bash
git add docs/components/landing/InstallBlock.tsx docs/components/landing/styles.css docs/components/landing/Landing.tsx
git commit -m "landing: add InstallBlock (Section 08 · Jack in)"
```

---

## Task 4: Build WordmarkFooter

**Files:**
- Create: `docs/components/landing/WordmarkFooter.tsx`
- Modify: `docs/components/landing/styles.css`
- Modify: `docs/components/landing/Landing.tsx`

Reference: mockup footer (around line 1150-1160 of `landing-v2.html`).

- [ ] **Step 1: Write the component**

```tsx
// docs/components/landing/WordmarkFooter.tsx
export function WordmarkFooter() {
  return (
    <footer className="landing-footer">
      <div className="landing-footer-meta">
        <a href="https://github.com/jackin-project/jackin" target="_blank" rel="noopener">GitHub</a>
        <span className="sep">·</span>
        <a href="https://jackin.tailrocks.com/" target="_blank" rel="noopener">Docs</a>
        <span className="sep">·</span>
        <span>Apache 2.0</span>
      </div>
      <div className="landing-footer-wordmark">jackin<span className="tick">'</span></div>
    </footer>
  );
}
```

- [ ] **Step 2: Append footer styles**

```css
/* Append to docs/components/landing/styles.css */

.landing-footer {
  margin-top: 60px;
  padding: 60px 40px 0;
  text-align: center;
  overflow: hidden;
}
.landing-footer-meta {
  display: flex;
  justify-content: center;
  align-items: center;
  gap: 18px;
  flex-wrap: wrap;
  margin-bottom: 64px;
  font-family: 'JetBrains Mono', monospace;
  font-size: 11px;
  color: var(--landing-text-ghost);
  letter-spacing: 0.15em;
  text-transform: uppercase;
}
.landing-footer-meta a {
  color: var(--landing-text-dim);
  text-decoration: none;
  transition: color 0.15s;
}
.landing-footer-meta a:hover { color: var(--landing-text); }
.landing-footer-meta .sep { color: var(--landing-ui-strong); }
.landing-footer-wordmark {
  font-family: 'Inter', sans-serif;
  font-weight: 900;
  font-size: clamp(120px, 24vw, 300px);
  line-height: 0.9;
  letter-spacing: -0.06em;
  color: var(--landing-text);
  margin: 0;
  padding: 0;
  user-select: none;
}
.landing-footer-wordmark .tick {
  color: var(--landing-accent);
  text-shadow: 0 0 0.1em rgba(0, 255, 65, 0.35);
}

@media (max-width: 880px) {
  .landing-footer-wordmark { font-size: clamp(80px, 22vw, 180px); }
}
```

- [ ] **Step 3: Mount footer after InstallBlock**

```tsx
// docs/components/landing/Landing.tsx
import { InstallBlock } from './InstallBlock';
import { WordmarkFooter } from './WordmarkFooter';

export function Landing() {
  return (
    <div className="landing-root">
      <InstallBlock />
      <WordmarkFooter />
    </div>
  );
}
```

- [ ] **Step 4: Verify in browser**

Expected: Below the InstallBlock, a row of three metadata items (GitHub · Docs · Apache 2.0) in dim mono, then a massive "jackin'" wordmark filling the page width with a green accent apostrophe. On narrow viewports the wordmark shrinks responsively.

- [ ] **Step 5: Commit**

```bash
git add docs/components/landing/WordmarkFooter.tsx docs/components/landing/styles.css docs/components/landing/Landing.tsx
git commit -m "landing: add WordmarkFooter"
```

---

## Task 5: Build PillCards (Section 03)

**Files:**
- Create: `docs/components/landing/PillCards.tsx`
- Modify: `docs/components/landing/styles.css`
- Modify: `docs/components/landing/Landing.tsx`

Reference: mockup section 03 (around line 380-425 of `landing-v2.html`) and the corresponding CSS (search `pills-grid`, `pill-card`, `pill-visual`).

- [ ] **Step 1: Write the component**

```tsx
// docs/components/landing/PillCards.tsx
export function PillCards() {
  return (
    <section className="landing-section">
      <div className="landing-shell">
        <div className="landing-sec-label">03 · The Problem</div>
        <h2 className="landing-sec-title">The <span className="accent">false</span> choice.</h2>
        <p className="landing-sec-intro">Every AI coding agent offers you a pill. The false choice is thinking you have to swallow either one.</p>

        <div className="landing-pills">
          <div className="landing-pill-card blue">
            <div className="landing-pill-visual">
              <div className="landing-pill-color" />
              <div className="landing-pill-white" />
            </div>
            <div className="landing-pill-meta">Blue pill</div>
            <h3>Babysit every prompt</h3>
            <ul className="landing-choice-lines">
              <li>"Are you sure?" dialogs every ten seconds.</li>
              <li>Permission gates on every action.</li>
              <li>The agent waits on you, constantly.</li>
              <li>Flow interrupted a hundred times a day.</li>
            </ul>
            <div className="landing-choice-verdict">Productivity · destroyed</div>
          </div>
          <div className="landing-pill-card red">
            <div className="landing-pill-visual">
              <div className="landing-pill-color" />
              <div className="landing-pill-white" />
            </div>
            <div className="landing-pill-meta">Red pill</div>
            <h3>Full YOLO on host</h3>
            <ul className="landing-choice-lines">
              <li>Agent reads every file — SSH keys, <code>.env</code>, cookies.</li>
              <li>Runs any command on your machine.</li>
              <li>Installs any package it wants — supply chain and all.</li>
              <li>One bad prompt is an unrecoverable bad day.</li>
            </ul>
            <div className="landing-choice-verdict">Risk · maximum</div>
          </div>
        </div>

        <div className="landing-choice-transition">
          Refuse the pill. <span className="accent">You're the Operator</span> — define the construct instead.
        </div>
      </div>
    </section>
  );
}
```

- [ ] **Step 2: Copy pill CSS from mockup into styles.css**

Open `docs/superpowers/mockups/landing-v2.html` and find the CSS rules under `/* Section 3: Red pill / blue pill */`. Copy them into `docs/components/landing/styles.css` with these prefix substitutions:

- `.pills-grid` → `.landing-pills`
- `.pill-card` → `.landing-pill-card`
- `.pill-card.pill-blue` → `.landing-pill-card.blue`
- `.pill-card.pill-red` → `.landing-pill-card.red`
- `.pill-visual` → `.landing-pill-visual`
- `.pill-color` → `.landing-pill-color`
- `.pill-white` → `.landing-pill-white`
- `.pill-meta` → `.landing-pill-meta`
- `.choice-lines` → `.landing-choice-lines`
- `.choice-verdict` → `.landing-choice-verdict`
- `.choice-transition` → `.landing-choice-transition`

Also swap color variables: `var(--accent)` → `var(--landing-accent)`, `var(--panel)` → `var(--landing-panel)`, `var(--text)` → `var(--landing-text)`, `var(--text-dim)` → `var(--landing-text-dim)`, `var(--text-ghost)` → `var(--landing-text-ghost)`, `var(--ui)` → `var(--landing-ui)`, `var(--danger)` → `var(--landing-danger)`.

Mobile breakpoint: add `.landing-pills { grid-template-columns: 1fr; }` inside an `@media (max-width: 880px)` block.

- [ ] **Step 3: Mount PillCards before InstallBlock**

```tsx
// docs/components/landing/Landing.tsx
import { InstallBlock } from './InstallBlock';
import { PillCards } from './PillCards';
import { WordmarkFooter } from './WordmarkFooter';

export function Landing() {
  return (
    <div className="landing-root">
      <PillCards />
      <InstallBlock />
      <WordmarkFooter />
    </div>
  );
}
```

- [ ] **Step 4: Verify in browser**

Expected: Two side-by-side cards. Left has a blue capsule visual + "Blue pill" label + "Babysit every prompt" heading + 4 bullet lines with blue `✕` markers + blue "Productivity · destroyed" verdict. Right mirror-image with red pill. On hover each card lifts with a stronger colored glow. Below: italic Fraunces "Refuse the pill. **You're the Operator** — define the construct instead." transition line.

Compare against mockup Section 03.

- [ ] **Step 5: Commit**

```bash
git add docs/components/landing/PillCards.tsx docs/components/landing/styles.css docs/components/landing/Landing.tsx
git commit -m "landing: add PillCards (Section 03)"
```

---

## Task 6: Build TabbedBuilder (used by ApproachCards)

**Files:**
- Create: `docs/components/landing/TabbedBuilder.tsx`
- Modify: `docs/components/landing/styles.css`

This is a reusable piece: a terminal-framed code view with tab switching. Used in Section 04 (manifest/Dockerfile) and conceptually similar to the hero code panel.

Reference: mockup (`.builder`, `.builder-tabbed`, `.builder-tabs`, `.builder-tab`, `.builder-body`).

- [ ] **Step 1: Write the component**

```tsx
// docs/components/landing/TabbedBuilder.tsx
import { useState, ReactNode } from 'react';

export interface BuilderTab {
  id: string;
  title: string;
  body: ReactNode; // pre-rendered spans with syntax highlighting classes
}

export interface TabbedBuilderProps {
  tabs: BuilderTab[];
  statusLabel?: string; // e.g., "Self-contained ✓"
  statusVariant?: 'built' | 'published';
}

export function TabbedBuilder({ tabs, statusLabel, statusVariant = 'built' }: TabbedBuilderProps) {
  const [activeId, setActiveId] = useState(tabs[0]?.id ?? '');
  const active = tabs.find(t => t.id === activeId) ?? tabs[0];

  return (
    <div className="landing-builder landing-builder-tabbed">
      <div className="landing-builder-head">
        <span className="landing-dot r" />
        <span className="landing-dot y" />
        <span className="landing-dot g" />
        <div className="landing-builder-tabs">
          {tabs.map(t => (
            <button
              key={t.id}
              type="button"
              className={'landing-builder-tab' + (t.id === activeId ? ' active' : '')}
              onClick={() => setActiveId(t.id)}
            >
              {t.title}
            </button>
          ))}
        </div>
        {statusLabel && (
          <span className={'landing-builder-tag landing-tag-' + statusVariant}>{statusLabel}</span>
        )}
      </div>
      <pre className="landing-builder-body">{active?.body}</pre>
    </div>
  );
}
```

- [ ] **Step 2: Copy builder CSS with landing- prefix**

From mockup's `.builder`, `.builder-head`, `.builder-tabs`, `.builder-tab`, `.builder-body`, `.builder-tag`, `.dot.r/.y/.g`, color classes `.b-c`, `.b-k`, `.b-s`. Prefix all with `landing-`. Use landing color tokens.

- [ ] **Step 3: Unit sanity check via React dev server**

Create a temporary smoke test by importing TabbedBuilder into Landing with dummy tabs:

```tsx
// Temporarily in Landing.tsx (remove after verification)
import { TabbedBuilder } from './TabbedBuilder';

<TabbedBuilder
  tabs={[
    { id: 'a', title: 'Tab A', body: <><span className="b-k">FROM</span> image</> },
    { id: 'b', title: 'Tab B', body: <><span className="b-c">{"# hello"}</span></> },
  ]}
  statusLabel="Self-contained ✓"
/>
```

Verify: two tabs, clicking switches body content. Remove the smoke test from Landing.tsx before committing.

- [ ] **Step 4: Commit**

```bash
git add docs/components/landing/TabbedBuilder.tsx docs/components/landing/styles.css
git commit -m "landing: add TabbedBuilder component"
```

---

## Task 7: Build ApproachCards (Section 04)

**Files:**
- Create: `docs/components/landing/ApproachCards.tsx`
- Modify: `docs/components/landing/styles.css`
- Modify: `docs/components/landing/Landing.tsx`

Reference: mockup section 04.

- [ ] **Step 1: Write the component**

```tsx
// docs/components/landing/ApproachCards.tsx
import { TabbedBuilder } from './TabbedBuilder';

const manifestBody = (
  <>
    <span className="b-k">dockerfile</span> = <span className="b-s">"Dockerfile"</span>{'\n\n'}
    <span className="b-k">[identity]</span>{'\n'}
    <span className="b-k">name</span> = <span className="b-s">"Backend Engineer"</span>{'\n\n'}
    <span className="b-k">[claude]</span>{'\n'}
    <span className="b-k">plugins</span> = [{'\n'}
    {'  '}<span className="b-s">"superpowers@superpowers-marketplace"</span>,{'\n'}
    ]{'\n\n'}
    <span className="b-k">[[claude.marketplaces]]</span>{'\n'}
    <span className="b-k">source</span> = <span className="b-s">"obra/superpowers-marketplace"</span>
  </>
);

const dockerfileBody = (
  <>
    <span className="b-k">FROM</span> projectjackin/construct:trixie{'\n\n'}
    <span className="b-c"># language toolchains via mise</span>{'\n'}
    <span className="b-k">RUN</span> mise install go@1.23 \{'\n'}
    {'    && mise use --global go@1.23\n\n'}
    <span className="b-c"># system packages</span>{'\n'}
    <span className="b-k">USER</span> root{'\n'}
    <span className="b-k">RUN</span> apt-get update && apt-get install -y \{'\n'}
    {'    postgresql-client redis-tools\n'}
    <span className="b-k">USER</span> claude
  </>
);

export function ApproachCards() {
  return (
    <section className="landing-section">
      <div className="landing-shell">
        <div className="landing-sec-label">04 · The Approach</div>
        <h2 className="landing-sec-title">Draw the boundary <span className="accent">yourself</span>.</h2>
        <p className="landing-sec-intro">jackin' gives you exactly one move: a clear boundary around an AI agent. You decide what's inside — toolchains, plugins, conventions — and what it can reach — paths, tokens, exposed ports. Two ways to get there.</p>

        <div className="landing-approach-grid">
          <div className="landing-approach-card">
            <div className="landing-route">Route 01 · Reuse</div>
            <h3>Pick up an opinionated one</h3>
            <p>Some orgs publish agent classes for their stack. The jackin team ships <em>the-architect</em> — with everything the jackin ecosystem requires to build jackin itself. Zero config: load and start working.</p>
            <div className="landing-toolset">
              <span className="landing-toolset-chip">Rust stable</span>
              <span className="landing-toolset-chip">cargo-nextest</span>
              <span className="landing-toolset-chip">cargo-watch</span>
              <span className="landing-toolset-chip chip-plugin">code-review</span>
              <span className="landing-toolset-chip chip-plugin">feature-dev</span>
              <span className="landing-toolset-chip chip-plugin">superpowers</span>
              <span className="landing-toolset-chip chip-plugin">jackin-dev</span>
            </div>
            <p className="landing-approach-note">Your framework's team can ship one just like it for yours.</p>
            <div className="landing-approach-cmd"><span className="lbl">cli</span>jackin load the-architect</div>
            <div className="landing-approach-repo"><span className="lbl">repo</span><span className="repo-path">github.com/jackin-project/jackin-the-architect</span></div>
          </div>

          <div className="landing-approach-card">
            <div className="landing-route">Route 02 · Build</div>
            <h3>Cast your own</h3>
            <p>Two files, one git repo. A short <code>jackin.agent.toml</code> declares identity and Claude plugins. A Dockerfile installs your language toolchains and system packages. Versioned, reviewable, <em>self-contained</em>:</p>
            <TabbedBuilder
              tabs={[
                { id: 'manifest',   title: 'jackin.agent.toml', body: manifestBody   },
                { id: 'dockerfile', title: 'Dockerfile',        body: dockerfileBody },
              ]}
              statusLabel="Self-contained ✓"
              statusVariant="built"
            />
            <div className="landing-approach-cmd"><span className="lbl">cli</span>jackin load your-org/backend</div>
            <div className="landing-approach-repo"><span className="lbl">repo</span><span className="repo-path">github.com/your-org/jackin-backend</span></div>
          </div>
        </div>

        <div className="landing-approach-transition">Either way — <span className="accent">you</span> draw the boundary.</div>
      </div>
    </section>
  );
}
```

- [ ] **Step 2: Copy approach CSS with landing- prefix**

From mockup: `.approach-grid`, `.approach-card`, `.route`, `.toolset`, `.toolset-chip`, `.approach-note`, `.approach-cmd`, `.approach-repo`, `.approach-transition`. Prefix all with `landing-`. Map color tokens.

- [ ] **Step 3: Mount ApproachCards in Landing between PillCards and InstallBlock**

```tsx
// docs/components/landing/Landing.tsx
import { ApproachCards } from './ApproachCards';
// ... other imports

<PillCards />
<ApproachCards />
<InstallBlock />
<WordmarkFooter />
```

- [ ] **Step 4: Verify in browser**

Compare against mockup Section 04. Tabs in Route 02 should switch body. Both cards' CLI and repo callouts visible.

- [ ] **Step 5: Commit**

```bash
git add docs/components/landing/ApproachCards.tsx docs/components/landing/styles.css docs/components/landing/Landing.tsx
git commit -m "landing: add ApproachCards (Section 04) with TabbedBuilder"
```

---

## Task 8: Build CastRoster (Section 05)

**Files:**
- Create: `docs/components/landing/CastRoster.tsx`
- Modify: `docs/components/landing/styles.css`
- Modify: `docs/components/landing/Landing.tsx`

Reference: mockup section 05.

- [ ] **Step 1: Write the component**

```tsx
// docs/components/landing/CastRoster.tsx
interface Character {
  avatar: string;
  role: string;
  name: string;
  tagline: string;
}

const characters: Character[] = [
  { avatar: 'AS', role: 'General-purpose',   name: 'Agent Smith',  tagline: 'The default starter. Clone, compile, commit.' },
  { avatar: 'AJ', role: 'Backend engineer',  name: 'Agent Jones',  tagline: "Server-side in your company's stack." },
  { avatar: 'AB', role: 'Frontend engineer', name: 'Agent Brown',  tagline: "UI with your team's conventions." },
];

export function CastRoster() {
  return (
    <section className="landing-section">
      <div className="landing-shell">
        <div className="landing-sec-label">05 · Cast</div>
        <h2 className="landing-sec-title">A role for every <span className="accent">job</span>.</h2>
        <p className="landing-sec-intro">Smith, Jones, Brown — archetypes to adopt. Every other role, yours to cast.</p>

        <div className="landing-roster">
          {characters.map(c => (
            <div key={c.avatar} className="landing-agent-card">
              <div className="landing-avatar">{c.avatar}</div>
              <div className="landing-role-label">{c.role}</div>
              <div className="landing-character">{c.name}</div>
              <p>{c.tagline}</p>
            </div>
          ))}
        </div>

        <div className="landing-cast-invite">
          <div className="landing-invite-avatar">+</div>
          <div className="landing-invite-body">
            <h3>Cast your own role.</h3>
            <p>Platform engineer, SRE, security reviewer, ML researcher — whatever your team needs. Write the Dockerfile, declare the manifest, push the repo.</p>
          </div>
          <a className="landing-invite-cta" href="https://jackin.tailrocks.com/developing/creating-agents" target="_blank" rel="noopener">Read the guide →</a>
        </div>
      </div>
    </section>
  );
}
```

- [ ] **Step 2: Copy cast CSS with landing- prefix**

From mockup: `.roster`, `.agent-card`, `.avatar`, `.role-label`, `.character`, `.cast-invite`, `.invite-avatar`, `.invite-body`, `.invite-cta`. Prefix `landing-`.

- [ ] **Step 3: Mount in Landing (between ApproachCards and InstallBlock)**

- [ ] **Step 4: Verify in browser**

Three character cards with AS / AJ / AB avatars, small green uppercase role labels, Fraunces-serif character names, and short taglines. Below them the full-width dashed invite strip with the green CTA link.

- [ ] **Step 5: Commit**

```bash
git add docs/components/landing/CastRoster.tsx docs/components/landing/styles.css docs/components/landing/Landing.tsx
git commit -m "landing: add CastRoster (Section 05)"
```

---

## Task 9: Data module + CompositionMachine (Section 06)

**Files:**
- Create: `docs/components/landing/machineData.ts`
- Create: `docs/components/landing/CompositionMachine.tsx`
- Modify: `docs/components/landing/styles.css`
- Modify: `docs/components/landing/Landing.tsx`

Reference: mockup section 06 + the embedded JS `(function () { const orgs = { ... } })()`.

- [ ] **Step 1: Write the data module**

```ts
// docs/components/landing/machineData.ts

export interface Mount {
  src: string;
  dst: string;
  ro: boolean;
}

export interface Workspace {
  workdir: string;
  mounts: Mount[];
  allowed: string[] | null;
}

export interface AgentClass {
  repo: string;
  tools: string;
  plugins: string;
}

export interface Org {
  classes: Record<string, AgentClass>;
  workspaces: Record<string, Workspace>;
}

export const orgs: Record<string, Org> = {
  'jackin-project': {
    classes: {
      'agent-smith':   { repo: 'jackin-project/jackin-agent-smith',   tools: 'git, gh, mise, zsh',              plugins: 'default starter' },
      'the-architect': { repo: 'jackin-project/jackin-the-architect', tools: 'Rust 1.87, cargo, ripgrep, just', plugins: 'superpowers · rust' },
    },
    workspaces: {
      'current-dir': {
        workdir: '$(pwd)',
        mounts: [{ src: '$(pwd)', dst: '$(pwd)', ro: false }],
        allowed: null,
      },
      'jackin-dev': {
        workdir: '~/Projects/jackin-project/jackin',
        mounts: [{ src: '~/Projects/jackin-project/jackin', dst: '~/Projects/jackin-project/jackin', ro: false }],
        allowed: null,
      },
    },
  },
  'chainargos': {
    classes: {
      'chainargos/backend-engineer':  { repo: 'chainargos/jackin-backend-engineer',  tools: 'Go 1.23, Postgres, grpcurl', plugins: 'API · SQL' },
      'chainargos/frontend-engineer': { repo: 'chainargos/jackin-frontend-engineer', tools: 'Node 22, Playwright, pnpm',  plugins: 'UI · a11y' },
      'chainargos/docs-writer':       { repo: 'chainargos/jackin-docs-writer',       tools: 'MDX, Vale, prettier',        plugins: 'writing' },
    },
    workspaces: {
      'monorepo': {
        workdir: '~/Projects/chainargos/monorepo',
        mounts: [{ src: '~/Projects/chainargos/monorepo', dst: '~/Projects/chainargos/monorepo', ro: false }],
        allowed: null,
      },
      'docs-only': {
        workdir: '~/Projects/chainargos/monorepo/docs',
        mounts: [{ src: '~/Projects/chainargos/monorepo/docs', dst: '~/Projects/chainargos/monorepo/docs', ro: true }],
        allowed: ['chainargos/docs-writer'],
      },
    },
  },
  'your-org': {
    classes: {
      'your-org/frontend-engineer': { repo: 'your-org/jackin-frontend-engineer', tools: 'Node 22, Playwright, pnpm',  plugins: 'UI · a11y' },
      'your-org/backend-engineer':  { repo: 'your-org/jackin-backend-engineer',  tools: 'Go 1.23, Postgres, grpcurl', plugins: 'API · SQL' },
    },
    workspaces: {
      'product': {
        workdir: '~/Projects/your-org/product',
        mounts: [
          { src: '~/Projects/your-org/product',    dst: '~/Projects/your-org/product', ro: false },
          { src: '~/Projects/your-org/shared-lib', dst: '/shared',                     ro: true },
        ],
        allowed: null,
      },
      'platform-api': {
        workdir: '~/Projects/your-org/platform',
        mounts: [
          { src: '~/Projects/your-org/platform', dst: '~/Projects/your-org/platform', ro: false },
          { src: '~/Projects/your-org/proto',    dst: '/proto',                      ro: true },
        ],
        allowed: ['your-org/backend-engineer'],
      },
    },
  },
};
```

- [ ] **Step 2: Write the CompositionMachine component**

```tsx
// docs/components/landing/CompositionMachine.tsx
import { useState } from 'react';
import { orgs } from './machineData';

export function CompositionMachine() {
  const orgKeys = Object.keys(orgs);
  const [activeOrg, setActiveOrg] = useState(orgKeys[0]);
  const org = orgs[activeOrg];
  const classKeys = Object.keys(org.classes);
  const wsKeys    = Object.keys(org.workspaces);
  const [activeClass, setActiveClass] = useState(classKeys[0]);
  const [activeWs, setActiveWs] = useState(wsKeys[0]);

  // When org changes, reset class/ws defaults
  function switchOrg(o: string) {
    setActiveOrg(o);
    const next = orgs[o];
    setActiveClass(Object.keys(next.classes)[0]);
    setActiveWs(Object.keys(next.workspaces)[0]);
  }

  const cl = org.classes[activeClass];
  const ws = org.workspaces[activeWs];
  const denied = ws?.allowed && !ws.allowed.includes(activeClass);
  const shortClass = activeClass.split('/').pop() ?? activeClass;

  return (
    <section id="concepts" className="landing-section">
      <div className="landing-shell">
        <div className="landing-sec-label">06 · Mental Model</div>
        <h2 className="landing-sec-title">Think in <span className="accent">two dimensions</span>.</h2>
        <p className="landing-sec-intro">Same agent in different workspaces. Same workspace with different agents. Pick both — see what runs.</p>

        <div className="landing-machine-wrapper">
          <div className="landing-org-tabs">
            {orgKeys.map(o => (
              <div
                key={o}
                className={'landing-org-tab' + (o === activeOrg ? ' active' : '')}
                onClick={() => switchOrg(o)}
              >
                <span className="at">@</span>{o}
              </div>
            ))}
          </div>

          <div className="landing-machine">
            <div className="landing-machine-panel">
              <div className="landing-machine-label">Agent Class</div>
              <div className="landing-machine-sublabel">the tool profile</div>
              <div className="landing-machine-options">
                {classKeys.map(name => (
                  <div
                    key={name}
                    className={'landing-machine-opt' + (name === activeClass ? ' active' : '')}
                    onClick={() => setActiveClass(name)}
                  >
                    <span className="landing-radio" />{name}
                  </div>
                ))}
              </div>
            </div>

            <div className="landing-machine-op">×</div>

            <div className="landing-machine-panel">
              <div className="landing-machine-label">Workspace</div>
              <div className="landing-machine-sublabel">workdir + mounts</div>
              <div className="landing-machine-options">
                {wsKeys.map(name => (
                  <div
                    key={name}
                    className={'landing-machine-opt' + (name === activeWs ? ' active' : '')}
                    onClick={() => setActiveWs(name)}
                  >
                    <span className="landing-radio" />{name}
                  </div>
                ))}
              </div>
            </div>

            <div className="landing-machine-op">=</div>

            <div className="landing-machine-panel preview">
              <div className="landing-machine-label">Running Agent</div>
              <div className="landing-machine-sublabel">the resulting container</div>
              <div className="landing-preview">
                {denied ? (
                  <div className="landing-preview-denied">
                    <span className="label">✕ not loaded</span>
                    Workspace "{activeWs}" declares <code>allowed-agents: [{ws?.allowed?.join(', ')}]</code>.
                    Rejected before the container starts.
                  </div>
                ) : cl && ws ? (
                  <>
                    <PreviewRow k="container"><span className="hl">jackin-{shortClass}</span></PreviewRow>
                    <PreviewRow k="class">{activeClass}</PreviewRow>
                    <PreviewRow k="repo">github.com/{cl.repo}</PreviewRow>
                    <PreviewRow k="tools">{cl.tools}</PreviewRow>
                    <PreviewRow k="plugins">{cl.plugins}</PreviewRow>
                    <PreviewRow k="workdir">{ws.workdir}</PreviewRow>
                    <PreviewRow k="mounts">
                      <div className="landing-mount-list">
                        {ws.mounts.map((m, i) => (
                          <div key={i} className="landing-mount-item">
                            {m.src === m.dst ? (
                              <span className="src">{m.src}</span>
                            ) : (
                              <>
                                <span className="src">{m.src}</span>
                                <span className="arrow">→</span>
                                <span className="dst">{m.dst}</span>
                              </>
                            )}
                            <span className={'perm ' + (m.ro ? 'ro' : 'rw')}>{m.ro ? 'ro' : 'rw'}</span>
                          </div>
                        ))}
                      </div>
                    </PreviewRow>
                    <PreviewRow k="network">jackin-{shortClass}-net</PreviewRow>
                  </>
                ) : null}
              </div>
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}

function PreviewRow({ k, children }: { k: string; children: React.ReactNode }) {
  return (
    <div className="landing-preview-row">
      <span className="k">{k}</span>
      <span className="v">{children}</span>
    </div>
  );
}
```

- [ ] **Step 3: Copy machine CSS with landing- prefix**

From mockup: `.machine-wrapper`, `.org-tabs`, `.org-tab`, `.at`, `.machine`, `.machine-panel`, `.machine-op`, `.machine-label`, `.machine-sublabel`, `.machine-options`, `.machine-opt`, `.radio`, `#preview`/`.preview-*`, `.preview-row`, `.preview-denied`, `.mount-list`, `.mount-item`. Prefix all with `landing-`.

- [ ] **Step 4: Mount CompositionMachine in Landing (between CastRoster and InstallBlock)**

- [ ] **Step 5: Verify in browser**

Click org tabs — class and workspace options change. Click radios — preview updates. Try `@chainargos` → `chainargos/frontend-engineer` + `docs-only` — should show red "✕ not loaded" state.

- [ ] **Step 6: Commit**

```bash
git add docs/components/landing/machineData.ts docs/components/landing/CompositionMachine.tsx docs/components/landing/styles.css docs/components/landing/Landing.tsx
git commit -m "landing: add CompositionMachine (Section 06)"
```

---

## Task 10: Build FocusCallout (Kitchen-Sink vs Role-Specific)

**Files:**
- Create: `docs/components/landing/FocusCallout.tsx`
- Modify: `docs/components/landing/styles.css`
- Modify: `docs/components/landing/CompositionMachine.tsx` (include FocusCallout inside Section 06)

Reference: mockup `.focus-callout`, `.focus-card`, `.focus-label`, `.focus-lines`, `.focus-note`.

- [ ] **Step 1: Write the component**

```tsx
// docs/components/landing/FocusCallout.tsx
export function FocusCallout() {
  return (
    <div className="landing-focus-callout">
      <div className="landing-focus-card">
        <div className="landing-focus-label">Kitchen-Sink Agent</div>
        <div className="landing-focus-lines">
          <span>Every toolchain.</span>
          <span>Every plugin.</span>
          <span>Every convention.</span>
        </div>
        <p className="landing-focus-note">Too much context — worse decisions.</p>
      </div>
      <div className="landing-focus-card focus-role">
        <div className="landing-focus-label">Role-Specific Agent</div>
        <div className="landing-focus-lines">
          <span>Only relevant tools.</span>
          <span>Only matching plugins.</span>
          <span>Only applicable conventions.</span>
        </div>
        <p className="landing-focus-note">Focused context — better results, faster.</p>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Copy focus-callout CSS**

Rename to `landing-focus-*` in styles.css.

- [ ] **Step 3: Include FocusCallout at the end of CompositionMachine's section**

```tsx
// docs/components/landing/CompositionMachine.tsx (inside the shell div, after the machine-wrapper div)
import { FocusCallout } from './FocusCallout';

// ... existing JSX
</div> {/* close machine-wrapper */}
<FocusCallout />
```

- [ ] **Step 4: Verify in browser**

Below the composition machine, two side-by-side cards appear: neutral-bordered "Kitchen-Sink Agent" with 3 "Every X" lines and a single-sentence note; green-bordered "Role-Specific Agent" with 3 "Only Y" lines and the counter-note.

- [ ] **Step 5: Commit**

```bash
git add docs/components/landing/FocusCallout.tsx docs/components/landing/styles.css docs/components/landing/CompositionMachine.tsx
git commit -m "landing: add FocusCallout inside Section 06"
```

---

## Task 11: Vocabulary data + scroll-driven dictionary (Section 02)

**Files:**
- Create: `docs/components/landing/vocabularyData.ts`
- Create: `docs/components/landing/VocabularyDictionary.tsx`
- Modify: `docs/components/landing/styles.css`
- Modify: `docs/components/landing/Landing.tsx`

Reference: mockup `.voc-scroll-section`, `.voc-sticky`, `.voc`, `.voc-list`, `.voc-item`, `.voc-detail`, and the JavaScript IIFE that drives it.

- [ ] **Step 1: Write the data module**

```ts
// docs/components/landing/vocabularyData.ts
export interface DefSegment {
  t: string;
  b?: boolean;
}

export interface VocabularyEntry {
  id: string;
  term: string;
  pos: 'noun' | 'verb';
  def: DefSegment[];
  cmd?: string;
  cmdLabel?: string;
}

export const vocabularyEntries: VocabularyEntry[] = [
  {
    id: '01', term: 'Operator', pos: 'noun',
    def: [
      { t: 'You.', b: true },
      { t: ' Running the CLI from your host machine. The one who decides what gets loaded into the Matrix, and when agents are pulled back out.' },
    ],
  },
  {
    id: '02', term: 'The Construct', pos: 'noun',
    def: [
      { t: 'The ' },
      { t: 'shared base Docker image', b: true },
      { t: ' every agent extends. Debian plus the jackin\u2019 runtime \u2014 the empty white space where programs get loaded before a mission.' },
    ],
    cmd: 'projectjackin/construct:trixie', cmdLabel: 'image',
  },
  {
    id: '03', term: 'Agent class', pos: 'noun',
    def: [
      { t: 'A reusable tool profile built on top of the Construct.', b: true },
      { t: ' A git repo with a Dockerfile that extends the base image, plus a small manifest \u2014 adds the toolchains, Claude plugins, shell setup, and conventions layered on top. Answers \u201cwhat kind of agent is this?\u201d' },
    ],
    cmd: 'chainargos/backend-engineer', cmdLabel: 'identifier',
  },
  {
    id: '04', term: 'Workspace', pos: 'noun',
    def: [
      { t: 'A named list of mounts and access rules.', b: true },
      { t: ' Each workspace pairs a name with: the host directories that mount into the container, where they land inside, per-mount permission (read-only or read-write), the agent\u2019s starting directory (workdir), and which agent classes are allowed to load it. Answers \u201cwhat can this agent see, and where?\u201d' },
    ],
    cmd: '{ name, workdir, mounts[], allowed-agents[] }', cmdLabel: 'declares',
  },
  {
    id: '05', term: 'Jacking in', pos: 'verb',
    def: [
      { t: 'Loading an agent into a workspace.', b: true },
      { t: ' Clones the agent-class repo, builds the derived image, applies the workspace\u2019s mounts, drops you into Claude Code running inside.' },
    ],
    cmd: 'jackin load agent-smith [my-project-workspace]', cmdLabel: 'cli',
  },
  {
    id: '06', term: 'The agent inside', pos: 'noun',
    def: [
      { t: 'Claude Code running with full permissions', b: true },
      { t: ' inside the container boundary. It thinks the Matrix is the whole world \u2014 but the world ends at the container wall.' },
    ],
  },
  {
    id: '07', term: 'Hardline', pos: 'verb',
    def: [
      { t: 'Reattach your terminal', b: true },
      { t: ' to a running agent. Closed the window? Agent\u2019s still running \u2014 hardline back in and pick up where you left off.' },
    ],
    cmd: 'jackin hardline agent-smith', cmdLabel: 'cli',
  },
  {
    id: '08', term: 'Pulling out', pos: 'verb',
    def: [
      { t: 'Stop an agent cleanly.', b: true },
      { t: ' State persists on disk for next time \u2014 the operator decides when a construct is torn down.' },
    ],
    cmd: 'jackin eject agent-smith', cmdLabel: 'cli',
  },
  {
    id: '09', term: 'Exile', pos: 'verb',
    def: [
      { t: 'Pull everyone out of the Matrix at once.', b: true },
      { t: ' Every running agent, every network, stopped in a single command.' },
    ],
    cmd: 'jackin exile', cmdLabel: 'cli',
  },
];
```

- [ ] **Step 2: Write the VocabularyDictionary component**

```tsx
// docs/components/landing/VocabularyDictionary.tsx
import { useState, useEffect, useRef } from 'react';
import { vocabularyEntries } from './vocabularyData';

export function VocabularyDictionary() {
  const sectionRef = useRef<HTMLElement>(null);
  const [activeIdx, setActiveIdx] = useState(0);

  useEffect(() => {
    const section = sectionRef.current;
    if (!section) return;

    let ticking = false;
    function onScroll() {
      if (ticking) return;
      ticking = true;
      requestAnimationFrame(() => {
        const rect = section!.getBoundingClientRect();
        const vh = window.innerHeight;
        const h = section!.offsetHeight;
        const scrollDist = h - vh;
        if (scrollDist <= 0) { setActiveIdx(0); ticking = false; return; }
        const scrolled = Math.max(0, -rect.top);
        const p = Math.max(0, Math.min(0.999, scrolled / scrollDist));
        const idx = Math.min(vocabularyEntries.length - 1, Math.floor(p * vocabularyEntries.length));
        setActiveIdx(idx);
        ticking = false;
      });
    }
    window.addEventListener('scroll', onScroll, { passive: true });
    window.addEventListener('resize', onScroll, { passive: true });
    onScroll();
    return () => {
      window.removeEventListener('scroll', onScroll);
      window.removeEventListener('resize', onScroll);
    };
  }, []);

  function jumpTo(i: number) {
    const section = sectionRef.current;
    if (!section) return;
    const vh = window.innerHeight;
    const scrollDist = section.offsetHeight - vh;
    if (scrollDist <= 0) return;
    const target = section.offsetTop + ((i + 0.5) / vocabularyEntries.length) * scrollDist;
    window.scrollTo({ top: target, behavior: 'smooth' });
  }

  const e = vocabularyEntries[activeIdx];

  return (
    <section id="why" ref={sectionRef} className="landing-section landing-voc-scroll-section">
      <div className="landing-voc-sticky">
        <div className="landing-shell">
          <div className="landing-sec-label">02 · Vocabulary</div>
          <h2 className="landing-sec-title">The vocabulary <span className="accent">is</span> the product.</h2>
          <p className="landing-sec-intro">Every command in jackin' maps to a concept from The Matrix — not for fun, but because the Matrix mental model is the shortest path to understanding what the tool does.</p>

          <div className="landing-voc">
            <div className="landing-voc-list">
              {vocabularyEntries.map((entry, i) => (
                <div
                  key={entry.id}
                  className={'landing-voc-item' + (i === activeIdx ? ' active' : '')}
                  onClick={() => jumpTo(i)}
                >
                  <span className="num">{entry.id}</span>
                  <span className="word">{entry.term}</span>
                </div>
              ))}
            </div>
            <div key={activeIdx} className="landing-voc-detail landing-fade">
              <div className="landing-detail-word">{e.term}</div>
              <div className="landing-detail-pos">{e.pos}</div>
              <p className="landing-detail-def">
                <span className="lead">—</span>
                {e.def.map((seg, i) => seg.b ? <strong key={i}>{seg.t}</strong> : <span key={i}>{seg.t}</span>)}
              </p>
              {e.cmd && (
                <div className="landing-detail-cmd">
                  <span className="lbl">{e.cmdLabel}</span>
                  {e.cmd}
                </div>
              )}
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
```

- [ ] **Step 3: Copy voc-scroll CSS with landing- prefix**

From mockup: `.voc-scroll-section`, `.voc-sticky`, `.voc`, `.voc-list`, `.voc-item`, `.voc-detail`, `.detail-*`, `@keyframes detailFade`. Also map mobile fallback. Prefix `landing-`. Note the `key={activeIdx}` on voc-detail triggers React re-mount which restarts the `landing-fade` animation.

- [ ] **Step 4: Mount VocabularyDictionary in Landing (between hero placeholder and PillCards)**

For now, since hero is not yet built, just add VocabularyDictionary first. Task 16 will reorder.

- [ ] **Step 5: Verify in browser**

Scroll through the section. The rail should highlight the active entry, the detail panel should swap content smoothly, and clicking rail items should smooth-scroll to that entry.

- [ ] **Step 6: Commit**

```bash
git add docs/components/landing/vocabularyData.ts docs/components/landing/VocabularyDictionary.tsx docs/components/landing/styles.css docs/components/landing/Landing.tsx
git commit -m "landing: add VocabularyDictionary (Section 02) scroll-driven"
```

---

## Task 12: Daily loop data + DailyLoop (Section 07)

**Files:**
- Create: `docs/components/landing/loopData.ts`
- Create: `docs/components/landing/DailyLoop.tsx`
- Modify: `docs/components/landing/styles.css`
- Modify: `docs/components/landing/Landing.tsx`

Reference: mockup section 07 (5 loop-frames).

- [ ] **Step 1: Write the data module**

```ts
// docs/components/landing/loopData.ts
import { ReactNode } from 'react';

export interface LoopFrame {
  id: string;
  name: string;
  mythos: string;
  desc: string;
  terminal: ReactNode; // pre-rendered terminal body with class names
}

export const loopFrames: LoopFrame[] = [
  {
    id: '01',
    name: 'load',
    mythos: 'Jacking in.',
    desc: 'Enter your project and jack in. Clones the agent repo, builds the derived image, launches the container, drops you into Claude Code.',
    terminal: (
      <>
        <span className="c"># enter your project</span>{'\n'}
        <span className="p">$</span> <span className="cmd">cd ~/Projects/my-app</span>{'\n\n'}
        <span className="c"># jack in</span>{'\n'}
        <span className="p">$</span> <span className="cmd">jackin load agent-smith</span>{'\n'}
        <span className="arrow">  → Pulling construct:trixie     </span><span className="ok">OK</span>{'\n'}
        <span className="arrow">  → Cloning agent-smith           </span><span className="ok">OK</span>{'\n'}
        <span className="arrow">  → Building derived image        </span><span className="ok">OK</span>{'\n'}
        <span className="arrow">  → Launching DinD sidecar        </span><span className="ok">OK</span>{'\n\n'}
        <span className="check">✓</span> <span className="done">Agent loaded. You're inside.</span>
      </>
    ),
  },
  // ... (populate the other 4 from the mockup Section 07: clone, hardline, eject, exile)
];
```

Copy the remaining 4 frames (`clone`, `hardline`, `eject`, `exile`) from the mockup's matching loop-frame blocks with the same `className` approach for syntax spans.

- [ ] **Step 2: Write the DailyLoop component**

```tsx
// docs/components/landing/DailyLoop.tsx
import { loopFrames } from './loopData';

export function DailyLoop() {
  return (
    <section id="commands" className="landing-section">
      <div className="landing-shell">
        <div className="landing-sec-label">07 · How it Works</div>
        <h2 className="landing-sec-title">The <span className="accent">daily loop</span>.</h2>
        <p className="landing-sec-intro">Five moves. Any number of agents. A full day's flow with jackin'.</p>

        <div className="landing-loop">
          {loopFrames.map(f => (
            <div key={f.id} className="landing-loop-frame">
              <div className="landing-loop-info">
                <div className="landing-loop-num">№ {f.id}</div>
                <div className="landing-loop-name">{f.name}</div>
                <div className="landing-loop-mythos">{f.mythos}</div>
                <p className="landing-loop-desc">{f.desc}</p>
              </div>
              <div className="landing-loop-term">
                <div className="landing-loop-term-bar">
                  <span className="landing-dot r" />
                  <span className="landing-dot y" />
                  <span className="landing-dot g" />
                </div>
                <pre className="landing-loop-term-body">{f.terminal}</pre>
              </div>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}
```

- [ ] **Step 3: Copy loop CSS with landing- prefix**

From mockup: `.loop`, `.loop-frame`, `.loop-num`, `.loop-name`, `.loop-mythos`, `.loop-desc`, `.loop-term`, `.loop-term-bar`, `.loop-term-body`, syntax classes `.c`, `.p`, `.cmd`, `.arrow`, `.ok`, `.done`, `.check`. Prefix all with `landing-`.

- [ ] **Step 4: Mount DailyLoop in Landing (before CompositionMachine for now; final order in Task 16)**

- [ ] **Step 5: Verify in browser**

Five vertical frames, each with info left + terminal right. All five commands visible. On mobile the grid collapses to single-column.

- [ ] **Step 6: Commit**

```bash
git add docs/components/landing/loopData.ts docs/components/landing/DailyLoop.tsx docs/components/landing/styles.css docs/components/landing/Landing.tsx
git commit -m "landing: add DailyLoop (Section 07)"
```

---

## Task 13: Rain engine unit tests and implementation

**Files:**
- Create: `docs/components/landing/rainEngine.ts`
- Create: `docs/components/landing/rainEngine.test.ts`
- Modify: `docs/package.json` (add `test` script)

The rain engine is pure logic — perfect for TDD. Port the algorithm from `src/tui.rs`.

- [ ] **Step 1: Add `bun test` script to package.json**

```json
{
  "scripts": {
    "dev": "vocs dev",
    "build": "vocs build",
    "preview": "vocs preview",
    "test": "bun test"
  }
}
```

- [ ] **Step 2: Write the failing test for `ageToColor`**

```ts
// docs/components/landing/rainEngine.test.ts
import { test, expect } from 'bun:test';
import { ageToColor } from './rainEngine';

test('ageToColor returns WHITE for fresh cells (age 0)', () => {
  expect(ageToColor(0)).toBe('rgb(255,255,255)');
});

test('ageToColor returns pale green for age 1-2', () => {
  expect(ageToColor(1)).toBe('rgb(180,255,180)');
  expect(ageToColor(2)).toBe('rgb(180,255,180)');
});

test('ageToColor returns MATRIX_GREEN for age 3-5', () => {
  expect(ageToColor(3)).toBe('rgb(0,255,65)');
  expect(ageToColor(5)).toBe('rgb(0,255,65)');
});

test('ageToColor returns null for dead cells (age > 24)', () => {
  expect(ageToColor(25)).toBeNull();
  expect(ageToColor(100)).toBeNull();
});
```

- [ ] **Step 3: Run the test to verify it fails**

```bash
cd docs && bun test rainEngine
```

Expected: FAIL — `ageToColor` not defined.

- [ ] **Step 4: Implement the rain engine**

```ts
// docs/components/landing/rainEngine.ts

// Exact char pool from src/tui.rs line 76-77
export const RAIN_CHARS = '0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz@#$%&*<>{}[]|/\\~';

// Exact palette from src/tui.rs lines 13-19 + age_to_color (lines 55-64)
export function ageToColor(age: number): string | null {
  if (age === 0)  return 'rgb(255,255,255)';   // WHITE — leader
  if (age <= 2)   return 'rgb(180,255,180)';   // pale green
  if (age <= 5)   return 'rgb(0,255,65)';      // MATRIX_GREEN
  if (age <= 10)  return 'rgb(0,200,50)';      // mid green
  if (age <= 16)  return 'rgb(0,140,30)';      // MATRIX_DIM
  if (age <= 24)  return 'rgb(0,80,18)';       // MATRIX_DARK
  return null;
}

// Mutation probability from src/tui.rs lines 67-74
export function shouldMutate(age: number, rng: () => number = Math.random): boolean {
  const roll = rng() * 100;
  if (age <= 2)  return roll < 30;
  if (age <= 10) return roll < 15;
  return roll < 5;
}

export function randomChar(rng: () => number = Math.random): string {
  return RAIN_CHARS[Math.floor(rng() * RAIN_CHARS.length)];
}

export interface RainCell {
  ch: string;
  age: number;
  fade: number;
}

export interface RainColumn {
  head: number;
  speed: number;
  fade: number;
  active: boolean;
  cooldown: number;
}

export interface RainState {
  cols: number;
  rows: number;
  grid: (RainCell | null)[][];
  columns: RainColumn[];
  frame: number;
}

export function createRainState(cols: number, rows: number, rng: () => number = Math.random): RainState {
  const columns: RainColumn[] = Array.from({ length: cols }, () => ({
    head: -Math.floor(rng() * (rows + 6)),
    speed: 1 + Math.floor(rng() * 4),
    fade: 1 + Math.floor(rng() * 3),
    active: rng() >= 0.33,
    cooldown: 0,
  }));
  const grid: (RainCell | null)[][] = Array.from({ length: rows }, () => new Array(cols).fill(null));
  return { cols, rows, columns, grid, frame: 0 };
}

export function tickRain(state: RainState, rng: () => number = Math.random): void {
  // Age all cells
  for (let r = 0; r < state.rows; r++) {
    const row = state.grid[r];
    for (let c = 0; c < state.cols; c++) {
      const cell = row[c];
      if (!cell) continue;
      cell.age += cell.fade;
      if (ageToColor(cell.age) === null) {
        row[c] = null;
      } else if (shouldMutate(cell.age, rng)) {
        cell.ch = randomChar(rng);
      }
    }
  }

  // Advance columns
  for (let c = 0; c < state.cols; c++) {
    const col = state.columns[c];
    if (!col.active) {
      if (col.cooldown > 0) col.cooldown--;
      else {
        col.active = true;
        col.head = -Math.floor(rng() * 6);
        col.speed = 1 + Math.floor(rng() * 4);
        col.fade = 1 + Math.floor(rng() * 3);
      }
      continue;
    }
    if (state.frame % col.speed === 0) col.head++;
    if (col.head >= 0 && col.head < state.rows) {
      state.grid[col.head][c] = { ch: randomChar(rng), age: 0, fade: col.fade };
    }
    if (col.head > state.rows + 5) {
      col.active = false;
      col.cooldown = 2 + Math.floor(rng() * 18);
    }
  }

  state.frame++;
}
```

- [ ] **Step 5: Run the tests to verify they pass**

```bash
cd docs && bun test rainEngine
```

Expected: 4 tests passing.

- [ ] **Step 6: Add a test for `tickRain` determinism with a seeded rng**

```ts
// append to rainEngine.test.ts
import { createRainState, tickRain, RainState } from './rainEngine';

function makeRng(seed: number) {
  return () => {
    // xorshift for deterministic testing
    seed ^= seed << 13; seed ^= seed >>> 17; seed ^= seed << 5;
    return ((seed >>> 0) % 10000) / 10000;
  };
}

test('tickRain advances frame and mutates grid deterministically', () => {
  const rng1 = makeRng(12345);
  const state1 = createRainState(8, 8, rng1);
  tickRain(state1, rng1);
  tickRain(state1, rng1);

  const rng2 = makeRng(12345);
  const state2 = createRainState(8, 8, rng2);
  tickRain(state2, rng2);
  tickRain(state2, rng2);

  expect(state1.frame).toBe(2);
  expect(state2.frame).toBe(2);
  expect(JSON.stringify(state1.grid)).toBe(JSON.stringify(state2.grid));
});
```

- [ ] **Step 7: Run all tests**

```bash
cd docs && bun test
```

Expected: 5 tests passing.

- [ ] **Step 8: Commit**

```bash
git add docs/components/landing/rainEngine.ts docs/components/landing/rainEngine.test.ts docs/package.json
git commit -m "landing: add rainEngine (ported from src/tui.rs) with unit tests"
```

---

## Task 14: DigitalRain React component

**Files:**
- Create: `docs/components/landing/DigitalRain.tsx`

Wraps the rain engine in a `<canvas>` with resize handling and reduced-motion fallback.

- [ ] **Step 1: Write the component**

```tsx
// docs/components/landing/DigitalRain.tsx
import { useEffect, useRef } from 'react';
import { createRainState, tickRain, ageToColor } from './rainEngine';

export interface DigitalRainProps {
  fontSize?: number;
  cellW?: number;
  cellH?: number;
  frameMs?: number;
  opacity?: number;
}

export function DigitalRain({ fontSize = 14, cellW = 12, cellH = 18, frameMs = 35, opacity = 0.32 }: DigitalRainProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      // Render a single still frame and stop
      const rect = canvas.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;
      canvas.width = rect.width * dpr;
      canvas.height = rect.height * dpr;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      return;
    }

    let state = createRainState(Math.floor(canvas.clientWidth / cellW), Math.floor(canvas.clientHeight / cellH));
    let lastFrame = 0;
    let raf = 0;

    function resize() {
      if (!canvas) return;
      const dpr = window.devicePixelRatio || 1;
      const rect = canvas.getBoundingClientRect();
      canvas.width = rect.width * dpr;
      canvas.height = rect.height * dpr;
      ctx!.setTransform(dpr, 0, 0, dpr, 0, 0);
      state = createRainState(Math.max(1, Math.floor(rect.width / cellW)), Math.max(1, Math.floor(rect.height / cellH)));
    }
    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(canvas);

    function loop(ts: number) {
      raf = requestAnimationFrame(loop);
      if (ts - lastFrame < frameMs) return;
      lastFrame = ts;
      tickRain(state);
      ctx!.clearRect(0, 0, canvas!.clientWidth, canvas!.clientHeight);
      ctx!.font = fontSize + 'px "JetBrains Mono", "SF Mono", monospace';
      ctx!.textBaseline = 'top';
      for (let r = 0; r < state.rows; r++) {
        for (let c = 0; c < state.cols; c++) {
          const cell = state.grid[r][c];
          if (!cell) continue;
          const color = ageToColor(cell.age);
          if (!color) continue;
          ctx!.fillStyle = color;
          ctx!.fillText(cell.ch, c * cellW, r * cellH);
        }
      }
    }
    raf = requestAnimationFrame(loop);

    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
    };
  }, [fontSize, cellW, cellH, frameMs]);

  return <canvas ref={canvasRef} className="landing-rain-canvas" style={{ opacity }} />;
}
```

- [ ] **Step 2: Add rain canvas styles**

```css
/* append to styles.css */
.landing-rain-canvas {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  z-index: 1;
  pointer-events: none;
}
```

- [ ] **Step 3: Smoke test via Landing**

Temporarily mount a wrapper in Landing:

```tsx
// Temporarily in Landing.tsx
<div style={{ position: 'relative', height: '100vh', background: '#050605' }}>
  <DigitalRain />
</div>
```

Run `bun run dev`, open the page, verify rain renders (ASCII chars falling in green with white leaders). Toggle OS reduced-motion setting — canvas should render a single still frame and stop animating.

Remove the smoke test from Landing.tsx.

- [ ] **Step 4: Commit**

```bash
git add docs/components/landing/DigitalRain.tsx docs/components/landing/styles.css
git commit -m "landing: add DigitalRain React component wrapping rainEngine"
```

---

## Task 15: CodePanel (hero interactive terminal)

**Files:**
- Create: `docs/components/landing/CodePanel.tsx`
- Modify: `docs/components/landing/styles.css`

Reference: mockup `.code-panel`, `.code-head`, `.tabs`, `.tab`, `.code-body`, plus the JS IIFE that types out the scripts.

- [ ] **Step 1: Write the component**

```tsx
// docs/components/landing/CodePanel.tsx
import { useEffect, useRef, useState } from 'react';

interface Line {
  cls: string;
  text: string;
}

const scripts: Record<string, Line[]> = {
  load: [
    { cls: 'c',      text: '# Load an isolated agent into your current project\n' },
    { cls: 'prompt', text: '$ ' },
    { cls: 'cmd',    text: 'jackin ' },
    { cls: 'k',      text: 'load' },
    { cls: 'cmd',    text: ' agent-smith\n' },
    { cls: 'dim',    text: '  \u2192 Pulling construct:trixie     ' },
    { cls: 'ok',     text: 'OK\n' },
    { cls: 'dim',    text: '  \u2192 Building derived image       ' },
    { cls: 'ok',     text: 'OK\n' },
    { cls: 'dim',    text: '  \u2192 Per-agent Docker network     ' },
    { cls: 'ok',     text: 'OK\n' },
    { cls: 'dim',    text: '  \u2192 Claude Code ready             ' },
    { cls: 'ok',     text: 'OK\n' },
    { cls: 'cmd',    text: '\n\u2713 Agent loaded. You\u2019re inside.' },
  ],
  // ... populate hardline and eject scripts from the mockup's code-panel JS
};

const typingSpeedCharMs  = 11;
const typingSpeedLineMs  = 110;
const holdDurationMs     = 3500;

export function CodePanel() {
  const [active, setActive] = useState<'load' | 'hardline' | 'eject'>('load');
  const bodyRef = useRef<HTMLDivElement>(null);
  const tokenRef = useRef(0);

  useEffect(() => {
    const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    tokenRef.current += 1;
    const myToken = tokenRef.current;
    const body = bodyRef.current;
    if (!body) return;

    async function run() {
      while (myToken === tokenRef.current) {
        // Clear
        while (body!.firstChild) body!.removeChild(body!.firstChild);
        for (const line of scripts[active]) {
          if (myToken !== tokenRef.current) return;
          const span = document.createElement('span');
          span.className = line.cls;
          body!.appendChild(span);
          if (reducedMotion) {
            span.appendChild(document.createTextNode(line.text));
            continue;
          }
          for (const ch of line.text) {
            if (myToken !== tokenRef.current) return;
            span.appendChild(document.createTextNode(ch));
            await new Promise(r => setTimeout(r, ch === '\n' ? typingSpeedLineMs : typingSpeedCharMs));
          }
        }
        const cursor = document.createElement('span');
        cursor.className = 'cursor';
        body!.appendChild(cursor);
        if (reducedMotion) return;
        await new Promise(r => setTimeout(r, holdDurationMs));
      }
    }
    run();

    return () => { tokenRef.current += 1; };
  }, [active]);

  return (
    <div className="landing-code-panel">
      <div className="landing-code-head">
        <div className="landing-code-tabs">
          {(['load', 'hardline', 'eject'] as const).map(k => (
            <span
              key={k}
              className={'landing-code-tab' + (k === active ? ' active' : '')}
              onClick={() => setActive(k)}
            >
              $ {k}
            </span>
          ))}
        </div>
      </div>
      <div ref={bodyRef} className="landing-code-body" />
    </div>
  );
}
```

Populate `scripts.hardline` and `scripts.eject` from the mockup's matching entries.

- [ ] **Step 2: Copy code-panel CSS with landing- prefix**

From mockup: `.code-panel`, `.code-head`, `.tabs`, `.tab`, `.code-body`, `.cursor`, color classes `.prompt`, `.cmd`, `.k`, `.dim`, `.ok`. Prefix `landing-`.

- [ ] **Step 3: Smoke test**

Temporarily mount in Landing, verify tabs work and each tab types its script.

- [ ] **Step 4: Commit**

```bash
git add docs/components/landing/CodePanel.tsx docs/components/landing/styles.css
git commit -m "landing: add CodePanel with tabbed typing animations"
```

---

## Task 16: HeroContent and HeroStage (Section 01)

**Files:**
- Create: `docs/components/landing/HeroContent.tsx`
- Create: `docs/components/landing/HeroStage.tsx`
- Modify: `docs/components/landing/styles.css`
- Modify: `docs/components/landing/Landing.tsx`

- [ ] **Step 1: Write HeroContent**

```tsx
// docs/components/landing/HeroContent.tsx
import { CodePanel } from './CodePanel';

export function HeroContent() {
  return (
    <div className="landing-hero-grid">
      <div className="landing-hero-left">
        <h1 className="landing-hero-headline">
          You're the <span className="accent">Operator</span>.<br />
          <span className="soft">They're already</span> inside.
        </h1>
        <p className="landing-hero-deck">
          jackin' drops AI coding agents into isolated Docker containers — full autonomy inside, your host untouched outside. One CLI. Same-path mounts. Per-agent state.
        </p>
        <div className="landing-hero-ctas">
          <a className="landing-btn-primary" href="#why">Get Started →</a>
        </div>
      </div>
      <div className="landing-hero-right">
        <CodePanel />
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Write HeroStage**

```tsx
// docs/components/landing/HeroStage.tsx
import { DigitalRain } from './DigitalRain';
import { HeroContent } from './HeroContent';

export function HeroStage() {
  return (
    <section className="landing-hero-stage">
      <DigitalRain opacity={0.32} />
      <nav className="landing-topnav">
        <div className="landing-logo"><span className="mark" />jackin'</div>
        <div className="landing-nav-right">
          <a className="landing-star" href="https://github.com/jackin-project/jackin" target="_blank" rel="noopener">★ Star on GitHub</a>
        </div>
      </nav>
      <div className="landing-shell">
        <section className="landing-hero">
          <HeroContent />
        </section>
      </div>
    </section>
  );
}
```

- [ ] **Step 3: Copy hero CSS with landing- prefix**

From mockup's `/* ========== HERO STAGE ========== */` and related blocks. Rules to copy (all prefixed `landing-`): `.hero-stage`, vignette `::after`, `.topnav` with its gradient + backdrop-filter, `.logo` + `.mark`, `.nav-right`, `.star`, `.hero` padding, `.hero-grid`, `.hero-headline`, `.hero-deck`, `.hero-ctas`, `.hero-right` padding-top.

- [ ] **Step 4: Mount HeroStage at the top of Landing**

```tsx
// docs/components/landing/Landing.tsx — final assembly
import { HeroStage } from './HeroStage';
import { VocabularyDictionary } from './VocabularyDictionary';
import { PillCards } from './PillCards';
import { ApproachCards } from './ApproachCards';
import { CastRoster } from './CastRoster';
import { CompositionMachine } from './CompositionMachine';
import { DailyLoop } from './DailyLoop';
import { InstallBlock } from './InstallBlock';
import { WordmarkFooter } from './WordmarkFooter';

export function Landing() {
  return (
    <div className="landing-root">
      <HeroStage />
      <VocabularyDictionary />
      <PillCards />
      <ApproachCards />
      <CastRoster />
      <CompositionMachine />
      <DailyLoop />
      <InstallBlock />
      <WordmarkFooter />
    </div>
  );
}
```

- [ ] **Step 5: Verify the full page top to bottom**

```bash
cd docs && bun run dev
```

Open the URL. Expected: hero renders with rain, typing terminal demo, and tabs. Scroll — Vocabulary section advances through entries. Pills → Approach (two routes with tabbed builder) → Cast (three cards + invite) → Composition Machine (interactive orgs) → Daily Loop (5 frames) → Install → Footer wordmark. Everything matches the mockup.

- [ ] **Step 6: Commit**

```bash
git add docs/components/landing/HeroContent.tsx docs/components/landing/HeroStage.tsx docs/components/landing/styles.css docs/components/landing/Landing.tsx
git commit -m "landing: add HeroStage + HeroContent, assemble full page"
```

---

## Task 17: Responsive polish and accessibility pass

**Files:**
- Modify: `docs/components/landing/styles.css` (mobile media queries)

- [ ] **Step 1: Verify all breakpoints**

In the browser, use devtools to set viewport to 375px (iPhone SE), 768px (iPad), and 1440px. For each, scroll through the page top to bottom. Look for:

- Hero grid collapsing from 2-column to 1-column
- Pills / Approach / Cast grids collapsing to 1-column
- Composition Machine grid collapsing (3 panels stacked instead of side-by-side)
- Vocabulary section falling back to non-sticky at narrow widths
- Daily Loop frames collapsing to single-column
- Wordmark scaling down without overflow

Reference the mockup's `@media (max-width: 880px)` and `@media (max-width: 820px)` blocks. Make sure every landing- prefixed selector in those media queries is present in styles.css.

- [ ] **Step 2: Verify reduced-motion fallback**

In devtools, set "Rendering → Emulate CSS prefers-reduced-motion: reduce". Reload. Expected:

- Digital rain renders a single still frame and stops
- CodePanel's typing animation falls back to the full script rendered at once
- Vocabulary detail panel no longer animates on switch (still updates instantly)

- [ ] **Step 3: Keyboard navigation check**

Without mouse, Tab through the page. Verify:

- Nav links focusable (GitHub link)
- Hero CTAs focusable
- CodePanel tabs focusable (Tab, then Enter/Space to activate)
- Composition Machine radios and org tabs focusable
- Vocabulary rail items focusable
- Install CTA buttons focusable

If any button-like `<div>` is not focusable, add `tabIndex={0}` and a `role="button"` + keyboard handler. Common ones: `.landing-org-tab`, `.landing-machine-opt`, `.landing-voc-item`, `.landing-code-tab`. Convert the most critical ones to `<button>` elements for native accessibility.

- [ ] **Step 4: Commit**

```bash
git add docs/components/landing/styles.css docs/components/landing/*.tsx
git commit -m "landing: responsive + accessibility polish"
```

---

## Task 18: Build verification and final check

**Files:**
- (Verification only)

- [ ] **Step 1: Run Vocs production build**

```bash
cd docs && bun run build
```

Expected: Build succeeds with no errors. Warnings about font preloading are acceptable.

- [ ] **Step 2: Run production preview**

```bash
cd docs && bun run preview
```

Expected: Static site served. Visit the URL, verify the landing page renders correctly in production mode (no hot-reload artifacts).

- [ ] **Step 3: Run all tests**

```bash
cd docs && bun test
```

Expected: rainEngine tests pass.

- [ ] **Step 4: Visual regression check against mockup**

Open `docs/superpowers/mockups/landing-v2.html` and the built landing side-by-side. Scroll both from top to bottom. Note any visual differences (colors, spacing, typography, interactions). Any significant discrepancies indicate missing CSS rules or incorrect class naming. Fix inline.

- [ ] **Step 5: Update tailrocks docs deploy notes if needed**

Check `docs/.github/workflows/docs.yml` or any Cloudflare Pages / Vercel config. Confirm no changes needed for the new component directory.

- [ ] **Step 6: Final commit**

```bash
# Only if fixes were made during visual regression check
git add docs/components/landing/styles.css
git commit -m "landing: final visual regression fixes"
```

- [ ] **Step 7: Open/update the PR**

If the `landing-design` PR is still open (draft), push the implementation commits:

```bash
git push
gh pr ready $(gh pr view --json number -q .number)  # flip draft to ready
```

Or open a new PR if this plan was executed on a separate branch.

---

## Self-Review Notes

- **Spec coverage:** Every section from the spec has at least one task. Hero (1), Vocabulary (11), Pills (5), Approach (6-7), Cast (8), Mental Model (9-10), Daily Loop (12), Install (3), Footer (4). Design tokens in Task 2. Accessibility in Task 17. Build verification in Task 18.
- **No placeholders:** Every task includes actual code or explicit references to the mockup for CSS blocks. The mockup is the source of truth, so "copy .foo from the mockup and prefix landing-" is a complete instruction — the source exists and is unambiguous.
- **Type consistency:** Data module exports (`orgs`, `vocabularyEntries`, `loopFrames`) are referenced consistently by importing components. `RainState` and `RainCell` are defined once in `rainEngine.ts` and used in `DigitalRain.tsx`.
- **TDD discipline:** Only the rain engine (pure logic) has unit tests. React components are verified via `bun run dev` + visual comparison against the mockup, which is pragmatic for this project's existing test surface (there are no existing frontend tests). Each task ends with a commit.
- **Frequent commits:** 18 tasks, each ending in at least one commit. PR is one branch; visible progress is per-commit.
