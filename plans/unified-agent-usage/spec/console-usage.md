# Console usage

Covers: F13; S1-S6; W1; B1, B5.

## Screen contract: Overview

The top-level route SHALL use shipped Console grammar: top-left
` jackin❯  · usage`, blank spacer, body-only master/detail split, one bordered account
list, individually titled right panels, and centered contextual footer hints. The left
list SHALL contain Overview, nonselectable multi-account provider headings plus account
rows, and direct rows for single-account providers.

Overview SHALL use full-width Capsule-style meters for each account summary. It SHALL
render loading, refreshing with last-good, successful empty, current, stale,
partial-provider failure, and global failure/Retry without duplicating accounts.

#### Scenario: focused account list

- WHEN the list owns focus
- THEN its single-line border is green, the selected row has full-width selection fill,
  and `▸` appears
- AND inactive panes retain selection without the cursor.

## Screen contract: Account Detail

The right pane SHALL render `Account` metadata and `Limits` panels. Limits SHALL match
Capsule quota composition: provider/source order, full inner-width semantic meter,
remaining left and reset right, optional pace/reserve/run-out on rich current data,
blank rows between ordinary buckets, and special provider-defined separators. Plan
metadata appears once before limits.

#### Scenario: narrow terminal

- GIVEN the detail body is below the settled compact breakpoint
- THEN meters and secondary pace details collapse to one semantic line per window
- AND provider/account identity, first semantic detail, reset, focus, and scrolling remain usable.

## Interaction contract

Up/Down SHALL traverse canonical destinations; Enter opens; `R` refreshes/joins broker
work; Back/Escape reverses focus/navigation; `Ctrl-Q` quits. Long lists and detail
content SHALL use repository focus/scroll conventions. Removal returns to Overview
with the persistent notice.

#### Scenario: empty inventory

- THEN the detail pane explains no providers are configured
- AND footer offers refresh and existing Settings navigation without inventing rows.

