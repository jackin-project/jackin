# Light-mode redesign plan

Dark mode is the reference — it looks right. Light mode looks "ugly / weird colors / weird borders." This plan makes light mode a clean, coherent mirror of the dark design instead of a pile of bespoke per-component overrides.

**Keep, do not touch:** the first-screen hero with the digital rain — its saturated green canvas (`--landing-hero-stage` light scope: `--landing-bg: #16a34a`, `--landing-bg-deep: #0f7d37`, white accent) is the one light surface the operator likes. Everything below the hero is in scope.

Scope: `docs/src/styles/docs-theme.css` (the `--jk-*` light scope + per-component light overrides) and `docs/src/components/landing/styles.css` (the `--landing-*` light scope + light overrides).

---

## 1. Findings — why light mode reads wrong

### 1.1 Green-tinted "neutrals"
Light surfaces aren't neutral — they carry a green cast:

- docs: `--jk-panel: #f2f6f3`, `--jk-bg-deep: #e2e9e5` (green-grey off-whites)
- landing: `--landing-panel: #f6f8f7`, `--landing-bg-deep: #e9edeb`

On a white page these read as muddy/greenish cards rather than clean surfaces. This is the dominant "weird color" source.

### 1.2 Two different border families
- docs borders: `--jk-ui: rgba(5, 20, 12, 0.1)` — **green-black** alpha.
- landing borders: `--landing-ui: rgba(0, 0, 0, 0.08)` — neutral black alpha.
- bespoke overrides invent yet more: tables use `rgba(31, 36, 33, 0.13)`, asides use `color-mix(... 28%)`, etc.

Three+ uncoordinated border tints → borders look inconsistent and "weird," especially the green-black ones on white.

### 1.3 Phosphor green (`--jk-brand: #5cf07a`) used as a light accent
`#5cf07a` is the bright dark-mode phosphor. On light surfaces it's washed out and low-contrast. It still drives, on light: the tabs underline, right-rail TOC line, the brand-mark chevron, and the inline prose chevron (`.jk-name`). Those read weak/garish on white. Light should use the darker `--jk-accent`, never `--jk-brand`.

### 1.4 Accent contrast + button ink
- `--jk-accent: #1d9e75` (mid green). As **text/links on white** its contrast is ~2.6:1 — **fails WCAG AA** (needs ≥4.5:1). Links/inline accents are hard to read.
- Primary buttons in light use `background: var(--jk-accent)` + `color: #0a0a0a` (dark ink). Black on `#1d9e75` is ~3.5:1 — borderline for button text. Ink choice was picked for dark mode and never re-decided for light.

### 1.5 Green-tinted shadows
Landing shadows are green: `--landing-card-shadow-soft: 0 14px 32px rgba(10, 80, 30, 0.12)`, `…strong: rgba(10, 80, 30, 0.18)`. Colored shadows on a white page look off; shadows should be neutral.

### 1.6 Accumulated one-off overrides instead of a token system
Light mode is ~20 bespoke `:root[data-theme='light'] <selector>` blocks (tables, asides, early-dev notice, tabs, sidebar active, TOC, pagination, popover, dev-warning…), each with hand-picked rgba values. Dark mode is mostly token-driven; light mode is patches. That's the structural root cause — there is no single coherent light palette the components inherit, so each one drifts.

### Root cause
Light mode was bolted on as per-component overrides over a **green-tinted neutral palette + reused phosphor accent**, with no unified light token set mirroring dark. Fix the tokens once; delete the one-offs.

---

## 2. Target light palette (token-first)

Define a clean, **neutral cool-grey** light palette with an **AA-compliant green accent**, identical border/shadow families across docs + landing. Mirror the dark token structure so components inherit instead of overriding.

### 2.1 Surfaces (neutral, no green cast)
| Token | Now | Target | Note |
|---|---|---|---|
| `--jk-bg` | `#ffffff` | `#ffffff` | keep |
| `--jk-panel` | `#f2f6f3` | `#f4f5f7` | neutral grey, no green |
| `--jk-bg-deep` | `#e2e9e5` | `#e8eaee` | neutral recessed |
| `--jk-popover` | `#f7f8f6` | `#fbfbfc` | neutral |
| landing `--landing-panel` | `#f6f8f7` | `#f4f5f7` | match docs |
| landing `--landing-bg-deep` | `#e9edeb` | `#e8eaee` | match docs |

### 2.2 Text (neutral near-black ramp)
| Token | Target |
|---|---|
| `--jk-text` | `#15181c` |
| `--jk-prose` | `#262b30` |
| `--jk-text-dim` | `#535a61` |
| `--jk-text-ghost` | `#828b93` |

Landing text ramp set to the same values.

### 2.3 Accent — two roles, both AA
The single mid-green can't be both a readable text color and a fill. Split:

- `--jk-accent` (text / links / borders / icons on white): **`#0b774e`** (~5:1 on white, AA).
- Accent **fill** for solid buttons: `#15935f` (or keep `#1d9e75`) **with white ink** (`#ffffff`), not dark ink — guarantees button-label contrast.
- `--jk-brand` stays `#5cf07a` but is **never used directly on light surfaces**; light components reference `--jk-accent`.

Landing mirrors: `--landing-accent: #0b774e`; primary button fill green + white ink.

### 2.4 Borders — one family
| Token | Target |
|---|---|
| `--jk-ui` | `rgba(17, 24, 39, 0.10)` (neutral slate) |
| `--jk-ui-strong` | `rgba(17, 24, 39, 0.20)` |
| `--jk-ui-subtle` | `rgba(17, 24, 39, 0.05)` |

Landing `--landing-ui*` set to the same three values. Kill the green-black `rgba(5,20,12,*)` family and the per-component one-offs (`rgba(31,36,33,*)` etc.) — they all become `var(--jk-ui*)`.

### 2.5 Shadows — neutral
- Replace landing green shadows with neutral: `--landing-card-shadow-soft: 0 14px 32px rgba(17,24,39,0.10)`, `…strong: 0 20px 48px rgba(17,24,39,0.16)`.
- Button hover glow keeps the green tint (`color-mix(--jk-accent …)`) — that's intentional brand feedback, fine in both modes.

---

## 3. Component fixes (after tokens land, most resolve for free)

1. **`--jk-brand` → `--jk-accent` on light surfaces**: tabs underline, right-rail TOC line/dot, brand-mark chevron, `.jk-name` chevron. Add light-scope overrides (or make those rules use `--jk-accent` directly) so none render `#5cf07a` on white.
2. **Buttons (light)**: primary fill = accent-fill green + **white** ink (drop the `#0a0a0a` ink in light); secondary outline uses `--jk-accent` (now AA). Re-verify the green tiers from `AGENTS.md` still hold. The hero-light dark-pill exception stays (green-on-green canvas).
3. **Asides** (`.jk-aside` light): keep the tinted-by-callout-color approach but base the surface/border on `--jk-bg` + `var(--jk-ui)`; drop the heavy triple inset shadows → one subtle neutral shadow.
4. **Tables** (light): borders → `var(--jk-ui)` / `var(--jk-ui-strong)`; `thead` bg → `var(--jk-panel)`. Remove the `rgba(31,36,33,*)` one-offs.
5. **Tabs** (`[role=tablist]` light), **sidebar active**, **TOC**, **pagination / page-footer cards**, **popover**, **early-dev / dev-warning notices**: re-point every bespoke light rgba to the new tokens; delete values that duplicate a token.
6. **Code blocks**: stay dark (`--jk-code-bg`) in light mode — keep (intentional, reads fine).
7. **Switcher trigger / theme toggles / search / social**: already token-driven; confirm they pick up the new accent + neutral surfaces, no extra work expected.

---

## 4. Keep list (do not change)
- Hero (first screen) light scope: green canvas + digital rain + white accent.
- Dark mode: untouched — only the `:root[data-theme='light']` scopes and light overrides change.
- Code-block dark surface in both themes.
- The 3-tier green button system + shadows (only the light ink/accent values are corrected).

---

## 5. Acceptance criteria
- No green-tinted neutral surfaces or shadows in light mode (greys are neutral).
- One border family site-wide; no `rgba(5,20,12,*)` / `rgba(31,36,33,*)` survivors.
- `#5cf07a` never paints on a white surface; light accent is `--jk-accent`.
- Contrast: body/prose text ≥ 4.5:1; accent text/links ≥ 4.5:1; button labels ≥ 4.5:1; UI borders/icons ≥ 3:1. (Spot-check with a contrast tool; the proposed hexes target these.)
- Light mode visually mirrors dark mode's structure (same hierarchy, just light surfaces) and looks intentional, not patched.
- `bun run types:check` clean; `bun run build` succeeds.

---

## 6. Execution phases
1. **Tokens** — rewrite the `:root[data-theme='light']` blocks in both stylesheets to §2 (surfaces, text, accent split, borders, shadows). Biggest visual win; do first and review.
2. **De-phosphor** — §3.1: stop `--jk-brand` painting on light.
3. **Buttons** — §3.2: light fill/ink + accent.
4. **Consolidate overrides** — §3.3–3.5: re-point/delete the bespoke light rgba blocks to tokens.
5. **Verify** — contrast spot-checks, `types:check`, `build`, eyeball every light surface (docs page, sidebar, TOC, tables, asides, tabs, pagination, landing sections, footer) against dark for parity. Confirm hero rain untouched.
6. **Docs** — note the light palette + "never use `--jk-brand` on light" rule in `docs/AGENTS.md` Theme section.

Each phase is a separate commit; review after phase 1 before continuing, since the token rewrite alone may resolve most of the "weird" look.
