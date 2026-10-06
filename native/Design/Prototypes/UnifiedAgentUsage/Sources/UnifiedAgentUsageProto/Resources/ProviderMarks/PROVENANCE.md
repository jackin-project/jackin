# ProviderMarks provenance (deep audit 2026-08-10)

Template monochrome marks for `NSStatusItem` (`isTemplate = true`) and UI plates.
**Critical rule:** only ship marks verified against official brand / press assets.

**Desktop domain (7):** `codex` · `claude` · `amp` · `grok` · `zai` · `kimi` · `minimax`  
(`opencode` is a host surface but **out of Desktop icon contract** — no mark.)

## Live re-verify (2026-08-10, byte / path identity)

Fetched vendor or industry-canonical assets and compared to `*.master.*` **path `d=` identity** (SVG) or **pixel identity** (PNG).

| Surface | Product | Master | Source (fetched) | Result | Conf. |
|---------|---------|--------|------------------|--------|-------|
| `codex` | OpenAI / Codex | Blossom | Commons `OpenAI_logo_2025_(symbol).svg` · [openai.com/brand](https://openai.com/brand/) | **Byte-identical** to `codex.master.svg` | **High** (vendor mark via Commons) |
| `claude` | Anthropic Claude | Starburst | [lobe-icons `claude.svg`](https://github.com/lobehub/lobe-icons) | **Path-identical** to `claude.master.svg` (multi-ray starburst, not generic star) | **High** (product mark geometry) |
| `amp` | Amp | Triple chevron | **Vendor** [ampcode.com/amp-mark-color.svg](https://ampcode.com/amp-mark-color.svg) (+ [press-kit](https://ampcode.com/press-kit) favicon) | **Byte-identical** to `amp.master.svg` (sha256 head `215a0ef86a91`) | **High** (vendor press) |
| `grok` | xAI Grok | Slash-circle | [lobe-icons `grok.svg`](https://github.com/lobehub/lobe-icons) · xAI [brand guidelines](https://x.ai/legal/brand-guidelines) (download gated 403) | **Path-identical** to `grok.master.svg` | **High** geometry; **Med** vs sealed xAI kit |
| `kimi` | Moonshot Kimi | K + orbit | **Vendor** [KIMI Brand Guidelines](https://moonshotai.github.io/Branding-Guide/) `kimi-icon-round.png` | **Byte-identical** to `kimi.master.png` (1024²) | **High** (vendor brand guide) |
| `zai` | Z.ai | Z plate | **Vendor CDN** [z-cdn…/logo.svg](https://z-cdn.chatglm.cn/z-ai/static/logo.svg) | **Path-identical** (5 paths) to `zai.master.svg` | **High** (vendor CDN) |
| `minimax` | MiniMax | Vertical-bar mark | lobe-icons `minimax.svg` + live **minimax.io** OG/favicon (dual-bar glyph) | Master **path-identical** to lobe; **visual match** to official OG/favicon bars (not wordmark) | **High** (icon form of official mark) |
| fallback | jackin❯ | `Brand/JackinMonogram{Dark,Light}.svg` | canonical docs generator | Done | High |

## Ship PNG health (template mono, black + alpha)

| File | maxA | Notes |
|------|------|-------|
| `codex.png` | ~245 | OK |
| `claude.png` | ~253 | OK |
| `amp.png` | **255** | Rebuilt 2026-08-10: prior maxA≈59 (near-invisible template) → alpha-scaled full opacity, geometry preserved from official mark |
| `grok.png` | 255 | OK |
| `kimi.png` | ~242 | Mono K extracted from official round icon |
| `zai.png` | ~239 | OK |
| `minimax.png` | ~246 | OK |

Load order: **PNG preferred**, PDF fallback (`ProviderMarks.swift`).

## Rejected / previous stand-ins (do not reintroduce)

| Was | Why wrong |
|-----|-----------|
| SF Symbols (`sparkles`, `hexagongrid`, `waveform`) | Not brand marks |
| Generic 8-point star (claude) | Not Claude starburst proportions |
| Arch/dome glyph (amp) | Not Amp press mark |
| Generic “K” / “Z” / “G” monograms | Not official icon assets |
| Corrupted OpenAI soccer-ball silhouette | Bad conversion of classic mark |
| Amp ship PNG maxA≈59 | Official geometry, unusable template ink |

## Render law

- Status bar: template mono only (LG-P2 / FB1-6).
- Popover/Usage: same mark on brand-tinted plate with template glyph.
- Prefer icon-only marks on menu bar (no wordmarks).

## Masters

`*.master.svg` / `kimi.master.png` are the pre-template source files used for the PNG/PDF build.

## Prototype rebuild (2026-08-20)

Re-downloaded every mark from the sources above; all verified byte- or
path-identical to the audited masters. Rebuilt template assets:

- **Normalization:** all marks placed on a canonical 24pt grid with a 20pt
  live area (2pt uniform inset) via measured alpha-bbox transforms, so visual
  weight and padding are consistent across providers at every rendered size.
- **kimi:** rebuilt from the official `k-only-color.svg` (clean vector K +
  orbit dot) instead of extracting from the textured round icon.
- **zai:** the vendor CDN `logo.svg` is the full gradient app-icon plate; the
  earlier luma extraction kept the rounded plate border and read wrong as a
  mono template. Replaced with the pure italic Z glyph (lobe-icons `zai.svg`,
  `fill="currentColor"`), the same Z geometry the plate encloses — the icon
  form of the official mark.
- **PNG:** rendered at 1024², LANCZOS-downscaled to 512², black + alpha
  template ink, maxA 255.
- **PDF:** rsvg-convert vector-direct from the normalized SVGs; transparent
  paper (corner α=0 verified). Prototype loads **PDF-first** (vector,
  resolution-independent); PNG is fallback. This inverts the incumbent's
  PNG-first order, whose PDFs carried opaque paper.
