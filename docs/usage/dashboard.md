# Using Priority and All Usage

Priority lists pinned metrics, limits running low, long windows about to reset with unused quota, accounts needing attention and spent limits. A pinned metric moves to **Used Up** when exhausted and returns to **Pinned** after recovery. A cash or credit balance of zero qualifies as spent. A reset inventory of zero simply means there are no resets available. All Usage keeps the per-account metrics in saved order. Hiding a metric also unpins it and excludes it from alerts.

**Waiting for usage** means at least one account has no first reading. **No reported limits** means the observed accounts had no visible bounded resources. **All clear** means bounded resources were observed and none require attention. Read failures retain the last successful values, together with the account's failure notice.

## Thresholds and pace

The default warnings apply at 30% remaining and the critical level at 10%. Thresholds use the nearest whole percentage left. Settings offer five-point adjustments, with a warning range of 20–70 and a critical range of 5–25. Critical stays at least five points below warning.

**Burning Fast** projects usage to the provider's reset using its window length. A projection needs at least a minute and 1% of the window to have elapsed. A limit already burning fast remains listed as its pace approaches the cap, until its projection falls to 90% or less of the cap. Optional pace marks show elapsed window share on a used meter and the inverse on a remaining meter. **Use It or Lose It** flags a non-currency window longer than a day when over half remains and the reset is within the next 24 hours. Both Priority sections are enabled by default. Pace marks and the four notification switches start disabled.

Alerts appear inside the terminal. The first reading establishes a baseline. A drop across warning and critical delivers the critical alert when enabled. Level alerts rearm on recovery. A rolling reset moving forward before its previous deadline keeps its alert history. A new window can alert again after the former reset has passed. Closing the console ends its notification session.

## Combining accounts and sharing

Settings can combine Claude accounts or ChatGPT accounts independently. The combined view sums each contributing account's usage up to its own limit. A missing metric contributes nothing. Different currencies retain separate totals. The earliest reported reset appears on the combined limit. The focused metric lists the participating accounts' readings. Each member still has its own refresh, notices and alert transitions.

A redacted export includes aggregated usage values, timestamps, providers and metric types. Account names, IDs, workspace IDs, detailed diagnoses and unrecognized dynamic labels are omitted. An ordinary cached snapshot export can include account metadata and should be handled separately when sharing.

## Verification

The dashboard fixtures run through the public account registry. They cover threshold crossings and recovery, a pinned exhausted limit, empty balances and reset inventory, hiding, failure with retained data, redacted output, pooled percent limits, incompatible currencies and the UTC reset boundary. Fixture tests use controlled timestamps and an external vault boundary. [Dashboard tests](../../crates/usage/jackin-usage/tests/dashboard/tests.rs) · [Priority rules in OpenUsage](https://github.com/robinebers/openusage/blob/53f70b396698656cb5e1e3f59c40d1b86d6b5296/Sources/OpenUsage/Models/PriorityBoard.swift) · [Unification rules](https://github.com/robinebers/openusage/blob/53f70b396698656cb5e1e3f59c40d1b86d6b5296/Sources/OpenUsage/Models/UnifiedAccounts.swift)
