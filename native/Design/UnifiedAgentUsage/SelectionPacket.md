# Selection Packet — Unified Agent Usage

Status: SELECTION RECORDED — A without H, Alexey Zhokhov, 2026-08-20. The
record lives in [Alternatives.md](Alternatives.md) `## Selection record`;
rejected eligible directions are appended in
[AntiReferences.md](AntiReferences.md); [ExperienceBrief.md](ExperienceBrief.md)
is approved. This packet remains as the ballot and rationale record.

## Eligible ballot

Usage-window hierarchy (pick exactly one):

- **A** — Grouped Overview, Provider Detail
- **B** — Hierarchical Navigation Sidebar
- **G** — Attention Queue plus Complete Inventory

Popover remix (optional, orthogonal):

- **H** — Compact Focused Popover, Deep Window Handoff — valid only paired with
  the chosen window alternative; not a standalone window direction.

Permanently ineligible this round (documented rejections in
[AntiReferences.md](AntiReferences.md)):

- **C** — canonical-account table first (removes the persistent provider
  sidebar)
- **D** — three-column drilldown (adds a third permanent region)
- **E** — native inspector (adds a third region; demotes primary quota content)
- **F** — provider workspace with nested account source list (split inside a
  split)

## Evidence-led recommendation

**A without H.** Rationale: A is the structure the running incumbent prototype
(`native/Sources/JackinDesktop/UsageWindow/`) already proves buildable; every
baseline defect recorded against it is a row/composition failure, not a
structural one, so the fix is targeted, not a re-architecture. A removes the
observed provider-row placeholder defect and protects account/state columns
while preserving the incumbent sidebar, overview, provider detail, and complete
popover. B moves too many long account labels into navigation. G adds an
urgency destination with no baseline evidence that reaching depleted/stale rows
is slow. H removes complete quota-window detail from a popover whose baseline
hierarchy already passed visual review.

This recommendation is evidence-led input only. It does not replace human
selection.

## Exact record the human selector writes

In [Alternatives.md](Alternatives.md), under `## Selection record`:

1. `Human selection:` replace `PENDING` with the selected alternative (one of
   `A`, `B`, `G`, optionally `+ H`), the selector's name, and the date.
2. Remix inputs: replace each `PENDING` with the chosen
   - Primary hierarchy
   - Toolbar/accessory model
   - Minimum-width behavior
   - Popover structure
3. Winner rationale: why the selected alternative won.
4. Loser rationale: why every unselected eligible alternative (and H if not
   remixed) lost.
5. Remaining risks accepted with the selection.

In [AntiReferences.md](AntiReferences.md):

6. Append each newly rejected eligible direction as a
   reason/correction/learned-rule row, citing the human decision rationale —
   not agent-inferred reasoning.

In [ExperienceBrief.md](ExperienceBrief.md):

7. Fill `Approved by:` and `Approved on:` if the brief is approved as written;
   otherwise record requested brief changes first.

Only after 1–7 are the [PrototypeHandoff.md](PrototypeHandoff.md) preconditions
met and `tailrocks-macos-prototype` may be invoked.

## Fixture and gate reminders for the selector

- Every alternative preview binds to exact records in
  [Fixtures.md](Fixtures.md) (F02 normal, F03 multi-account, F05 depleted, F06
  stale, F10 offline, F11 long-identity, F08/F14 error states); a direction
  that works only for the normal fixture is ineligible at prototype review.
- Minimum-width behavior (800 × 520) is declared per alternative in
  [Alternatives.md](Alternatives.md); verify the chosen direction's declared
  behavior, not just its typical-size wireframe.
- Usage surfaces show subscription/quota limits only — no token prices,
  spend-over-time, or trend charts anywhere in the selected direction.
