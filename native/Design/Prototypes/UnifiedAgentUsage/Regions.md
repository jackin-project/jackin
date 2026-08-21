# Regions — Unified Agent Usage prototype

Dark-only post-signoff region contract. Geometry is measured at the 1000×680
default Usage window, the 800×520 minimum, the 1200×760 wide reference, and the
fixed 380×520 popover. Structural bounds are informative; accessibility roles,
labels, state, and ownership are the gate.

| Region | Class | Ownership and gate |
|---|---|---|
| Status items and context menu | NATIVE | `NSStatusItem`/`NSMenu`; correct provider identity and display-local action anchor. |
| Popover shell | NATIVE | `NSPopover`; fixed 380×520, transient dismissal, correct clicked-display placement. |
| Popover identity and quota content | NATIVE-COMPOSED | Official centered wordmark; selected account, semantic state, ordered limits. |
| Popover actions | NATIVE-COMPOSED | Refresh and Open Usage remain standard controls with keyboard labels. |
| Window chrome and centered identity | NATIVE | AppKit unified titlebar; wordmark is absolute-centered expanded and collapsed. |
| Sidebar | NATIVE-COMPOSED | Native sidebar plane; Overview, provider taxonomy, account-only multi-account destinations, meters, translucent wells. |
| Overview | CONTENT | Opaque adaptive provider/account modules over authored stage; no fake glass. |
| Provider detail | CONTENT | Identity, state strip, semantically ordered limits, account facts; opaque modules. |
| Digital rain | CONTENT-BACKGROUND | Noninteractive, subordinate, absent under Reduce Transparency, static/disabled under Reduce Motion. |
| Empty/loading/error/stale/unavailable | NATIVE-COMPOSED | Explicit text and symbol; unavailable never claims current quota. |
| Refresh | NATIVE | Standard `NSToolbarItem`; no authored bezel or material. |
| Settings | NATIVE-COMPOSED | Native Settings window and controls. |

Custom content modules use changed-pixel comparison only under the same dark
appearance, scale, backdrop, scenario, and accessibility state. Native chrome is
validated structurally. The capture harness and backdrop are excluded.

Pending operator-only evidence is listed in [SIGNOFF.md](SIGNOFF.md).
