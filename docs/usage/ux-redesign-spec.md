# Usage view design decisions

This note records design decisions from the November 2026 usage view redesign. The implementation and tests define current behavior.

## Account cards

The view groups accounts by provider and gives each account a card. A card keeps the account label, plan and fetch status together. It shows the selected primary limit first and indents secondary windows beneath it. This hierarchy keeps account identity visible when several accounts share a provider.

The selection rule prefers a live overall limit. If none exists, it prefers a live scoped limit. When neither role has a live limit, it selects the non-add-on limit with the longest window. The implementation is in [pick_main](../../crates/usage/jackin-usage/src/view/build.rs). If a live overall limit and a live daily scoped limit are both available, the overall limit appears first.

## Rollover and freshness

A limit whose reset time passed after its last fetch is shown as rolled over. The view removes the active gauge and shows the previous reading when it fits. Account freshness is displayed separately from limit rollover. The account reading becomes stale ten minutes after its successful fetch.

For example, a limit fetched before midnight that resets at midnight becomes a previous reading until the account refreshes. The stale marker describes the age of the fetched account data. The implementation is in [meter](../../crates/usage/jackin-usage/src/view/build.rs) and [fetched_text](../../crates/usage/jackin-usage/src/view/detail.rs).

## Terminal width

The card renderer measures terminal display width, truncates the plan and account label when needed, and selects shorter meter text when a line does not fit. The detail view also omits optional text when it cannot fit. These rules keep narrow terminals readable without removing the account hierarchy. The implementation is in [card.rs](../../crates/usage/jackin-usage/src/view/card.rs).

## Current examples

Use the checked-in [usage view fixture](../../crates/jackin-testkit/tests/fixtures/usage_view.json) for synthetic account data and the [view goldens](../../crates/usage/jackin-usage/tests/golden/view/) for rendered output. The current command and JSON contract are documented in the [usage CLI reference](cli.md). OpenRouter amounts use exact decimal strings in major units in JSON.
