# Signoff — Unified Agent Usage prototype

Status: BLESSED — 2026-08-20 by Alexey Zhokhov.

The prototype is dark-only. Canonical geometry is 800×520 minimum, 1000×680
default, 1200×760 wide, and 380×520 popover. Launch contract:
`--tr-scenario`, `--tr-appearance dark`, `--tr-window`, `--tr-reduce`, and
`--tr-backdrop`; `--tr-increase-contrast` is the deterministic contrast lane.

Build and test:

```sh
rtk mise run desktop-prototype-build
rtk swift test --package-path native/Design/Prototypes/UnifiedAgentUsage
```

## Automated contract

- dark-only parsing and exact window bounds;
- default/F02 equivalence and unknown-fixture rejection seam;
- account-only multi-account navigation and direct single-account navigation;
- keyboard destination order and valid selection persistence;
- explicit unavailable/stale quota truth;
- semantic quota category order with stable order inside a category;
- reduction flag parsing.

## Human signoff matrix

No row below is inferred from builds, screenshots, or agent review.

| Matrix | Status |
|---|---|
| F00–F29 at 800×520, 1000×680, and 1200×760 | passed — 2026-08-20 — Alexey Zhokhov |
| Popover-bearing fixtures at 380×520 | passed — 2026-08-20 — Alexey Zhokhov |
| Sidebar expanded/collapsed, provider/account selection, meters | passed — 2026-08-20 — Alexey Zhokhov |
| Hover, keyboard focus, VoiceOver, Voice Control, Full Keyboard Access | passed — 2026-08-20 — Alexey Zhokhov |
| Reduce Motion and Reduce Transparency using real system settings | passed — 2026-08-20 — Alexey Zhokhov |
| Increase Contrast and Differentiate Without Color using real system settings | passed — 2026-08-20 — Alexey Zhokhov |
| Active/inactive window, resize/full screen, scale and color profiles | passed — 2026-08-20 — Alexey Zhokhov |
| Secondary-display and rightmost-menu-bar popover anchoring | passed — 2026-08-20 — Alexey Zhokhov |
| Digital-rain worst-frame motion review | passed — 2026-08-20 — Alexey Zhokhov |

Blessed: 2026-08-20 by Alexey Zhokhov

Production adaptation follows [PRODUCTION_MAPPING.md](PRODUCTION_MAPPING.md).
Prototype harness/store/fixtures are never production source.
