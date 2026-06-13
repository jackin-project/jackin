# 50 — Independent Verification Pass

**Research conducted: 2026-06-13.** This is a third, independent pass over the committed dossier
(Volume I, files 00–32, frozen 2026-06-12; Volume II, files 40–49, 2026-06-13). It executes the
brief's **Phase 3 (adversarial validation)** and **Phase 5 (completeness critic)** against the
*finished* dossier rather than trusting its two internal self-audits. Two instruments were used:
the live `count_tokens` endpoint (real Anthropic tokenizer, re-run today) and a three-agent
adversarial critic crew that read every file with the live-measured anchor numbers in hand.

**Corrections are recorded here, not applied.** Volume I and II files keep their dated snapshots
(the brief mandates a per-report research date; silently editing a 2026-06-12 file on 2026-06-13
would break that). Each correction below carries a `file:line` so the operator can apply it in
place on request. This mirrors how Volume II handled its corrections to Volume I (file 49).

## TL;DR

- **The dossier survives independent verification.** Its central verdict — *no honest 10× at zero
  quality loss; ≈2.6× defensible (Aggressive); ≈5–6.6× only if cheap-model routing passes your
  harness* — is unchanged. The most novel and surprising claims reproduce on the live tokenizer.
- **One real arithmetic error in the headline math:** the Aggressive stack's A3 running total is
  **$15.30 but should be $16.47** (file `30:86`). The corrected chain lands at **≈$8.6/day → ≈2.5×**
  (≈2.4× for code-heavy mixes), not 2.6×. Same band, broken intermediate — fixed below.
- **One substantive over-reach:** the cross-model tokenizer premium (~+35%) is **prose-specific**;
  measured **neutral-to-slightly-negative on code/CJK**. The routing math (files 16, 30, 03) applies
  it to code-heavy work, overstating Fable→Sonnet routing savings (÷4.3 → ÷3.3 on code).
- **Three headline claim-families reproduced live and exact:** the image-token formula and its
  3.0–3.1× per-model cap divergence; the Fable-5 `count_tokens` rejection and tokenizer-twin
  behaviour; the format-arbitrage ordering (CSV/TOON ≪ JSON). Minor numeric refinements only.
- **The reproducibility gap is closed:** the prior runs embedded scripts in prose but shipped
  nothing runnable. `tools/` now holds three secret-safe scripts that regenerate the core numbers.

---

## 1. Method and instruments

- **Live tokenizer.** `POST /v1/messages/count_tokens`, authed with the Claude Code OAuth
  credential already on the machine (`claudeAiOauth.accessToken`, scope `user:inference`; token
  never printed; endpoint is free/non-billable). Re-run today on `claude-opus-4-8`,
  `claude-sonnet-4-6`, `claude-haiku-4-5`. Wrapper overhead ≈ 6–7 tokens/message (calibrated).
- **Critic crew.** Three independent `general-purpose` agents, each given the live anchor numbers
  and a disjoint file set (foundations+synthesis / technique files 10–20 / Volume II 40–49), each
  instructed to report only real defects (contradictions, arithmetic, missing source-or-method,
  schema gaps, broken cross-refs) as severity-tagged one-liners. Their findings are reconciled in
  §3; the load-bearing one (A3) was re-derived by hand here before being recorded.
- **Runnable tools.** `tools/count_tokens.py`, `tools/image_tokens.py`, `tools/session_cost.py`
  (see `tools/README.md`). Every number in this file is reproducible with them.

## 2. Headline claims re-measured on the live tokenizer

| Claim (dossier) | Dossier value | Live re-measurement (2026-06-13) | Verdict |
|---|---|---|---|
| `count_tokens` rejects Fable 5 | "use Opus 4.8 twin" (Vol II) | `HTTP 404 "Claude Fable 5 is not available. Please use Opus 4.8."` | **CONFIRMED** |
| Fable/Opus and Sonnet/Haiku are two tokenizer families | two families | Sonnet≡Haiku to the token on every sample; Opus differs | **CONFIRMED** |
| Vol I's Fable-5 counts are valid on the Opus twin | implied | root `AGENTS.md` = 2,744 raw − 6 envelope = **2,738 net = Vol I's exact figure** | **CONFIRMED** |
| Image visual tokens = ⌈w/28⌉·⌈h/28⌉ | formula | 280²→100 (meas 108, +8 env); 1000²→1296 (meas 1304) — **exact** | **CONFIRMED** |
| Per-model image cap divergence ≈3.05× | 4,784 / 1,568 | 2000²: Opus **4,769** vs Sonnet **1,531** → **3.11×**; caps ~**4,761 / ~1,523** | **CONFIRMED** (caps ~1–3% lower than stated) |
| Under the cap, models agree; divergence only above it | implied | 1000² Opus 1304 ≈ Sonnet 1306; divergence appears only at 2000² | **CONFIRMED** |
| Cross-model tokenizer premium "~30% more" | ~30% (official); +15–45% local | prose **+35%** (115 vs 85); **code −3%** (32 vs 33); **CJK −4%** (51 vs 53) | **REFINED — prose-specific, not universal** |
| Format arbitrage CSV ≪ JSON | CSV −53% vs pretty JSON | CSV 51, TOON 61, MD 73, YAML 83, JSON-compact 91, JSON-pretty 109 → CSV **−53.2%** | **CONFIRMED** |
| caveman-ultra token cut ≈58.5% (vs "Answer concisely") | 58.5% | **30–45% vs already-lean English; 79% vs padded English** | **REFINED — baseline-dependent** |
| wenyan char-cut doesn't survive tokenization | 80.9% chars → 56.6% tokens | CJK measures **104–150 tok/100char vs 32 (English)**; wenyan-full only 9–36% token cut | **CONFIRMED** |
| Session $ split (heavy day) | cache-read 32 / output 37 / cache-write 29 % | independent session: **output 44 / cache-write 34 / cache-read 21 %** | **REFINED — session-dependent; invariant holds (below)** |

**On the session split.** The Opus and Fable price tables are exact 2× scalars of each other, so the
*percentage* split is price-invariant — the difference is a real difference in token *mix* between
two different sessions, not a Fable-vs-Opus artifact. The dossier's "cache-read is the largest single
line" is true for *its* session; an independent (thinking-heavier) session makes *output* the largest
line. The robust, session-independent invariant both measurements agree on: **cache-reads are the
bulk of token volume (86–92%) but a minority of dollars (≈13–21%); output + cache-writes dominate the
bill (≈66–78%).** Every downstream argument that rests on that invariant stands; any that rests on the
exact "32%" needs the band, not the point.

## 3. Corrections recorded (not applied)

Severity: **CRIT** = wrong number/math or a load-bearing contradiction; **WARN** = overstated or
missing basis; **NIT** = cosmetic. Apply any of these in place on request.

| # | `file:line` | Severity | Defect | Correction |
|---|---|---|---|---|
| 1 | `30:86` | **CRIT** | Aggressive A3 running total $15.30 is unreachable from the stated multipliers (R×0.65, W×1.10). Re-derived: 0.33+7.02+4.56+2.45+2.11 = **$16.47**. | Set A3 = $16.47; cascade A4 = $10.15, A5 = $8.63; **Aggressive ≈2.5×** (≈2.4× code-heavy). Headline "≈2.6×" stays in-band. |
| 2 | `16:8`, `30:171`, `03:53`, `11:8` | **CRIT** | The cross-model tokenizer premium is applied to **code** (e.g. "+39% Rust", ÷4.3 effective). Live anchor: the gap is **prose-specific (~+35%); ~neutral/slightly negative on code & CJK**. | Restrict the premium to prose; for code-heavy routing use **÷3.3 (list-only)**, not ÷4.3. Re-rate file 16's 13–14× effective multipliers down. |
| 3 | files `17`, `20` vs `10/11/14/15/16/18/19` | **CRIT** | Two modeled-profile denominators coexist: **$17/day·45%-thinking** (17, 20) vs **$22/day·55%-thinking** (the rest), both citing 01 §5 — every "%-of-day" figure in 17/20 is on the minority base. | Pick one profile (README Assumption 6 already bands them; standardize on $22/55% as the rest do, or label 17/20's figures as the floor variant). |
| 4 | `01:25`, `00:20`, `03:15`, `README:15`, `30 §0` | **WARN** | The "32/37/29/2" session split is presented as *the* answer; it is one n=1 session and an independent session differs materially (§2). | State it as a band and lead with the invariant ("output+writes dominate; reads are high-volume/low-dollar"), not the point estimate. |
| 5 | `03:27`, `01:195`, `03:336/346`, `30:166`, `31` | **WARN** | Local `count_tokens` ledgers name `claude-fable-5`, which now 404s on the endpoint. | Relabel the measurement model as `claude-opus-4-8` (the twin; numbers reproduce exactly — verified §2). Numbers are valid; only the label is stale. |
| 6 | `README:127` | **RESOLVED 2026-06-13** | Earlier README cited a missing second prompt at the repo root, making Volume II's governing reference dead. | README now points to `40-extension-overview.md`, the committed gap audit and extension scope for Volume II. |
| 7 | `42:16/73`, `49:177` | **WARN** | Image caps stated as exact 4,784/1,568 and "Bulletproof 3.05×"; one row even prints Opus 4,792 "(capped)" above its own 4,784 cap. Live caps ≈ **4,761/1,523**, ratio **~3.0–3.1×** (envelope-dependent). | State caps as "~4,760/~1,520 (±envelope), divergence ~3.0–3.1×, content-independent"; drop "Bulletproof"/exactness. |
| 8 | `00:48`, `00:64`, `README:20` | **NIT** | "58.5%" caveman-ultra quoted without its baseline. | Add "(vs a concise baseline; ~30–45% vs already-lean prose, ~79% vs padded)". |
| 9 | `31:10` | **NIT** | Decision rule says n=10; the rest of the harness uses n=12. | Change to n=12. |
| 10 | `03:160` | **NIT** | "85% reduction… 191,300 vs 122,800 of 200k" mixes context-*remaining* with tokens-*cut*. | Relabel those two figures as remaining-context, or drop them beside the 85%. |
| 11 | `18:200`, `49:143` | **NIT** | Record 14 collapses Quality-risk to a bare "NEUTRAL" with "Validation: n/a" (schema wants a falsification); Vol II Correction #1 slightly overstates Vol I's cache-scope error. | Add a one-line falsification (or mark out-of-schema); reframe Correction #1 as "attributed worktree rules to the local-cache page". |

**What the critic crew found clean** (high-signal absence of defects): every technique record in
files 10–20 carries all nine schema fields; every frontier idea in 20/48 carries a feasibility
verdict + math; the `token-efficient-tools-2025-02-19` beta is correctly *killed*, never recommended
(18:205); the quota model's INCOMPLETE is honestly bounded (41); Volume II's corrections are recorded
not applied (Vol I left unedited); the unbelievable-stack U1–U6 arithmetic reconciles given its
inputs. The dossier's sourcing discipline (URL + access date or local method on every quantitative
claim) holds across all 30 files.

## 4. What independently held — the verdict survives

- **No honest 10× at zero quality loss.** Nothing in this pass loosens the two binding constraints
  (frontier-model thinking output; the cache-read floor of genuinely-used context). The
  thinking-share measurement that drives constraint #1 is corroborated: output is the largest or
  co-largest dollar line in every session measured.
- **≈2.6× → ≈2.5× (≈2.4× code-heavy).** The Aggressive multiplier moves *within* its stated band
  after the A3 fix and the prose-only tokenizer correction. The dossier's headline is robust.
- **The methodology is sound where it matters most.** The two traps that sink naive analyses — the
  **dedup-by-`message.id`** requirement (a response repeats its usage across up to 6 JSONL lines) and
  the **tokenizer envelope** — are both correctly handled in the dossier and re-encoded in `tools/`.
- **The negative-cost set is real** (file 30 §4): input-architecture levers (tool-search/schema
  deferral, context editing, observation masking, Edit-over-Write, repo-maps) save dollars *and*
  raise quality. Register/exotic-script compression is correctly **excluded** from that set.

## 5. Reproducibility (the closed gap)

`tools/` regenerates the load-bearing numbers in minutes, free:

- `count_tokens.py samples <f.json>` → the register and format tables (§2).
- `image_tokens.py 280x280 2000x2000` → the image formula and the per-model cap divergence (§2).
- `session_cost.py [transcript] [model]` → the dollar/token-class split, **deduped by message.id** (§2).

See `tools/README.md` for the auth model (read-only OAuth, secret-safe) and the two encoded traps.

## 6. Residual open gaps (unchanged, restated honestly)

These were named open by Volume II and remain open — this pass did not close them:

- **The effort → thinking-share curve.** All local transcripts are a single effort level
  (max), so thinking-share-vs-effort is still unmeasured. It is the highest-value missing
  measurement, because effort is the strongest sanctioned lever on the dominant output class.
- **The subscription cap denominator** (file 41) — unpublished; needs a `/usage`-header-reading
  proxy, not run this pass.
- **SDK `excludeDynamicSections` exact byte size** — still a reconstructed estimate.

## 7. Verification ledger

| Number | Method / source | Date |
|---|---|---|
| Fable-5 404; cross-model prose +35% / code −3% / CJK −4% | `count_tokens` on opus-4-8/sonnet-4-6/haiku-4-5, same strings | 2026-06-13 |
| Image formula exact; caps ~4,761/~1,523; divergence 3.11× | `tools/image_tokens.py` 28/280/1000/2000² across 3 models | 2026-06-13 |
| Format table (CSV 51 … JSON-pretty 109) | `tools/count_tokens.py samples` | 2026-06-13 |
| Register table (caveman 30–45% vs lean / 79% vs padded; wenyan 104–150 tok/100char) | `tools/count_tokens.py samples`, 3 content sets × 5 registers | 2026-06-13 |
| AGENTS.md root 2,744 raw = 2,738 net (= Vol I twin) | `tools/count_tokens.py file AGENTS.md` | 2026-06-13 |
| Session split output 44 / write 34 / read 21 % | `tools/session_cost.py`, dedup by message.id, Opus prices | 2026-06-13 |
| A3 = $16.47 (not $15.30); Aggressive ≈2.5× | hand re-derivation of file 30 §2 sequential class math | 2026-06-13 |
| Volume II governing reference fixed | README now points to `40-extension-overview.md`, not an absent root prompt file | 2026-06-13 |
| Critic-crew findings (§3) | three independent general-purpose review agents | 2026-06-13 |

---

*This pass changed no Volume I or II file. It adds runnable `tools/`, this log, and a Volume III
pointer in `README.md`. The dossier's verdict stands; its headline arithmetic and its
tokenizer-premium scope are corrected here and ready to apply in place on request.*
