# Usage connections from the CLI

Run `jackin account usage connect` with the provider ID. Interactive authentication needs a terminal. API keys, authorization responses, management keys and proxy credentials go into the prompts. Jackin stores credentials in the operating system credential store. On Linux, unlock a Secret Service collection in the host login session. On macOS, unlock the login Keychain.

On macOS, choosing **Always Allow** for Jackin in the Keychain dialog remembers access to the requested credential item. Each account has its own credential, and macOS can ask again after a local rebuild changes the executable. [Apple Keychain access controls](https://support.apple.com/guide/mac-help/kychn002/mac)

```sh
jackin account usage connect claude
jackin account usage connect codex
jackin account usage connect muse
jackin account usage connect grok
jackin account usage connect kimi --region global
jackin account usage connect kimi --auth device --region global
jackin account usage connect zai
jackin account usage connect minimax --region cn
jackin account usage connect minimax --auth device --region cn
jackin account usage connect mimo --region sgp
jackin account usage connect antigravity
jackin account usage connect opencode
jackin account usage connect openrouter
```

Claude and ChatGPT open a browser on the host. Finish sign-in there or paste the complete response that includes the state from this request. ChatGPT offers device authorization when its callback port is unavailable. Muse, Grok and OpenCode display a code. Press Enter to open the verification page. For OpenCode, choose the workspace in the provider's authorization flow. When using SSH, the browser must be able to reach the callback listener for a loopback flow.

For Grok, sign in with the xAI account linked to your eligible X subscription to read the Grok CLI billing pool. For Z.ai Global, use the Personal Coding Plan key. [Grok Build eligibility](https://x.ai/news/grok-build-cli) · [Z.ai key types](https://docs.z.ai/devpack/notice/api-key-management)

For Kimi and MiniMax, select `global` or `cn` to match the coding plan. For MiMo Token Plan, select `cn`, `sgp` or `ams` and enter a Token Plan key. Omit the MiMo region to connect the separate regular-key balance product. For an international MiMo personal Token Plan, match the endpoint in your console to Singapore (`sgp`) or Europe (`ams`). [MiMo cluster guide](https://mimo.mi.com/docs/en-US/tokenplan/Token%20Plan/quick-access)

For the Gemini and Claude quotas on your **Google AI Ultra** subscription, run `jackin account usage connect antigravity`. The connector follows the OpenUsage Antigravity browser flow: S256 PKCE, a loopback callback, identity lookup, then the plan and quota. Live sign-in uses only the OAuth client registration OpenUsage uses. Google has not authorized Jackin to use that registration, so the command stops before opening a browser and does not ask for a different client. A registration supplied to the process is a test seam. `jackin usage refresh` repeats a stored account's quota read directly from Google, including after Jackin restarts and while Antigravity is closed. A failed refresh keeps the previous reading and shows its observation time and the error. The status-line command can still record a report from a running Antigravity CLI for an account enrolled that way; that saved report is not a network refresh. [Antigravity CLI status line](https://antigravity.google/docs/cli/statusline)

## Manage a connection

The listing gives the stable ID used by subsequent commands. Rename changes the display name. Reconnect or replace a key to keep the ID, ordering and metric preferences. Connect a second account when the identity or workspace differs.

```sh
jackin account usage ls --json
jackin account usage rename "$account_id" "Work account"
jackin account usage reconnect "$account_id"
jackin account usage replace-key "$account_id"
jackin account usage rm "$account_id"
```

Local removal deletes the usage connection. Workspace execution bindings and gateway credentials have their own lifecycle under `jackin account`.

## Read and refresh usage

```sh
jackin usage
jackin usage --json
jackin usage providers
jackin usage providers --json
jackin usage providers antigravity
jackin usage details --provider antigravity --json
jackin usage details
jackin usage details --provider claude
jackin usage details --account "$account_id" --json
jackin usage snapshot --provider codex
jackin usage snapshot --account "$account_id"
jackin usage refresh
jackin usage refresh --account "$account_id"
jackin usage share
```

Use `jackin usage providers` to list products even before connecting your first account. Select **Antigravity (Google AI)** to track the Gemini and Claude quotas on a personal Google AI subscription.

In the catalog JSON, each product has an `id`, `name`, `account_count` and `status`. A status of `connected` means at least one account is saved. A product with no saved accounts is `not_connected` when Jackin supports a connection, or `unavailable` with an `unavailable_reason` when it cannot read that product. Listing the catalog succeeds on an empty installation.

Use `jackin usage details` to inspect all saved accounts side by side in text. Filter by provider or stable account ID. Add `--json` for numeric readings, quota reset times, and available model, day, and billing histories. The details command can run offline.

The plain `jackin usage` overview groups readings under each provider and account. A bounded reading says whether it shows the amount used or remaining. A stale reading is labeled on its own row. Snapshot and share commands read persisted data. Active refresh contacts providers that support polling, including a direct Antigravity account. A rejected refresh preserves the last successful reading and reports that reading's age with the error. The snapshot becomes stale ten minutes after that reading. The console updates connected accounts while it runs. For existing execution-account sources, request an explicit host read with `jackin usage host`.

Use `jackin usage redeem` to redeem an eligible account reset. Select the displayed reset and confirm the prompt. Jackin checks eligibility again before submitting the request. Reading, refreshing and exporting usage never redeems a reset.

## Connect a hub

```sh
jackin account usage hub connect hub.example.org:8317
jackin account usage hub ls
jackin account usage hub sync "$hub_id"
jackin account usage hub replace-key "$hub_id"
jackin account usage hub rm "$hub_id"
```

The connect prompt asks for the CLIProxyAPI management key. Jackin mirrors enabled Claude, ChatGPT and Grok logins that have an auth index. Sync updates membership and preserves local names. A management-key refusal stops further requests using that key. Replace it to reconnect. Removing the local hub also removes its mirrored usage accounts. Provider tokens stay on the server.

## Network proxy

Run `jackin usage proxy` and enter an HTTP, HTTPS or SOCKS5 proxy URL. Include proxy credentials in the hidden prompt when needed. The OS vault holds the URL. Run `jackin usage proxy --off` to remove the setting. Browser sign-in pages use the browser's own network settings.

See the [terminal walkthrough](terminal-walkthrough.md) for the keyboard controls and the [dashboard guide](dashboard.md) for display settings.

## Snapshot JSON schema 1

`jackin usage --json` and `jackin usage snapshot` read the local database without opening the credential store. Both write one JSON document to stdout. Times are Unix seconds in UTC.

| Field | Meaning |
| --- | --- |
| `schema` | Integer schema version, currently 1. |
| `generated_at` | Time jackin read the database. |
| `accounts` | Array of matching accounts in saved order. |
| `providers` | In `jackin usage --json`, the same product inventory as `jackin usage providers --json`. |
| `account.id`, `account.provider`, `account.label` | Stable UUID, provider ID and user-visible name within each array entry. The snapshot export includes account labels. Use `usage share` for a redacted report. |
| `fetched_at`, `expires_at`, `stale` | Last successful fetch, its ten-minute freshness deadline, and whether that deadline has passed. An account awaiting its first snapshot has `stale: true` and omits both timestamps. |
| `usage` | Null before the first successful fetch. Otherwise an object with optional `plan` and a `resources` map keyed by metric ID. |
| `resources.<id>` | A reading with `kind` (consumption, balance, amount or quota) and `unit`. Numeric fields depend on the metric. These may include `used`, `limit`, `available`, `remaining`, `amount`, `window_seconds` and `resets_at`. Monetary readings can include `currency`. |

A reported zero is a number. An omitted field or resource has no reported value. Utilization and remaining fractions use a scale of 0–1. Remaining percentages use 0–100. Currency readings in minor units include an exponent (2 means hundredths). OpenRouter USD readings are the exception: they are exact decimal strings in major units, described in the next section. A quota can report a provider status such as `unlimited`, `not_in_plan` or `exhausted`. Reset inventory can include an ordered `resources.rateLimitResets.expires_at` array. Grant identifiers and private history ledgers stay in the local account store.

### Exact amounts

OpenRouter amounts are exact decimals. Jackin reads each amount from the response text and never converts it to a floating-point number, so `0.004` and `0.10000000000000001` keep every digit through the stored snapshot, `jackin usage --json`, `jackin usage details`, `jackin usage host` and the host cache. USD amounts in a connected OpenRouter snapshot are decimal strings in major units, such as `"used":"0.0042"`, with no `exponent`. Counts and token totals stay numbers. Readers accept numbers and decimal strings.

| Case | Behavior |
| --- | --- |
| Precision | At most 38 digits after dropping leading and trailing zeros, and at most 38 digits after the decimal point. |
| Exponent | JSON exponent notation is expanded exactly: `1.5e-7` is `0.00000015`. Zero with any exponent is `0`. |
| Overflow | A value or a sum beyond the precision, such as `1e-39` or `1e40`, fails the read with "a number in the response is out of range". Jackin never rounds, saturates or truncates an amount. |
| Not a number | `null`, booleans, empty text and text that is not a decimal (`n/a`, `NaN`, `inf`) count as not reported. Text such as `" 7.5 "` or `"2.5e-3"` is a number. |
| Negative amounts | A key remainder, balance, budget spend or connected period spend below zero is read as zero. `jackin usage host` shows a negative key, daily, weekly or monthly spend as reported. A negative or unreadable activity amount rejects that activity read, and the days read earlier stay. |
| Rounding | Only presentation rounds: percentages, pace marks and the `K` and `M` captions for amounts of 10,000 or more. `jackin usage share` shows an integer amount as an integer and any other amount as a floating-point number. |

Exit 0 indicates at least one matching account has a stored snapshot. Exit 4 means the selected accounts have no stored snapshot, including an empty installation. Both can produce valid JSON. Invalid filters and filters matching zero accounts exit 2. Active refresh can return exit 1 with the last stored snapshot on stdout and the refresh error on stderr. Cancellation exits 130.
