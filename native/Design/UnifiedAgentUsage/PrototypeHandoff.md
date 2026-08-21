# Prototype handoff — Unified Agent Usage

Status: dark-only reference implementation complete; human blessing pending.

The selected direction remains alternative A without H. The runnable prototype
is the authoritative visual and interaction reference for later jackin❯ desktop
work, while [PRODUCTION_MAPPING.md](../Prototypes/UnifiedAgentUsage/PRODUCTION_MAPPING.md)
defines what may be adapted. Prototype fixture/store/harness code is never lifted
as production data ownership.

## Fixed contract

- macOS 26; dark appearance only;
- 800×520 minimum, 1000×680 default, 1200×760 wide;
- 380×520 popover;
- flags: `--tr-scenario`, `--tr-appearance dark`, `--tr-window`,
  `--tr-reduce`, `--tr-backdrop`, plus deterministic
  `--tr-increase-contrast`;
- unknown scenarios, malformed sizes, undersized windows, non-dark appearance,
  and unknown reduction values fail closed;
- defaults are cleared and fixture time/geometry remain deterministic before
  `TR-READY`.

## Ownership boundary

The package contains immutable fixture projections and no credentials, provider
networking, CLI invocation, persistence, broker, FFI, or production application
state. Reference UI consumes small presentation records. Harness configuration
does not enter feature views.

Rust/bridge production contracts own stable IDs, identity, visible strings,
state/freshness, semantic quota category, and final order. Swift owns layout and
native-platform composition only.

## Package structure

```text
Sources/UnifiedAgentUsageProto/
├── App/             # executable lifecycle, windows, status items, toolbar
├── Domain/          # immutable reference models and navigation state
├── DesignSystem/    # dark tokens, identity, provider marks, digital rain
├── Features/        # Usage, Popover, Settings reference views
├── Harness/         # fixture scenarios only
└── Resources/
Tests/UnifiedAgentUsageProtoTests/
```

## Gates

1. Build and unit tests pass.
2. [DESIGN_AUDIT.md](../Prototypes/UnifiedAgentUsage/DESIGN_AUDIT.md) matches
   current implementation.
3. [Regions.md](../Prototypes/UnifiedAgentUsage/Regions.md) covers every surface.
4. Human operator completes [SIGNOFF.md](../Prototypes/UnifiedAgentUsage/SIGNOFF.md).
5. Only then may roadmap language say visually blessed or READY.

Post-signoff visual QA captures the dark fixture matrix, real accessibility
settings with restoration proof, focus/hover, active/inactive windows,
collapsed/expanded sidebar, resize, and multi-display popover behavior.
