# Connect and inspect a usage account in the terminal

Launch `jackin` and press **u** for Usage. Press **a** for **All Usage** to compare accounts. Each row shows an account and a representative reading. Use **↑** and **↓** to see the selected account’s other limits in the preview. Press **Enter** to explore its metrics, then open a metric for its breakdown and actions. **Esc** goes back. Press **p** for **Priority** to see individual readings that need attention.

Choose **Providers** on the usage page to browse all products, including those without a saved account. For the Gemini and Claude quotas included with Google AI Ultra, select **Antigravity (Google AI)**. To manage a connection, press **c** from the main screen and select its name under **Usage accounts**. Execution accounts on that screen control sandbox launches.

## Connect another account

Open **Accounts** in the jackin console. With a provider selected in the tree, press Tab twice to focus the action buttons, use the arrow keys to highlight **Add usage account…**, and press Enter. From the account tree, **n** also opens the provider picker. To inspect a saved usage connection, choose the neighboring **Usage accounts…** button or press **Shift+N** from the tree. Select a provider, choose an account region, MiMo product, or Kimi/MiniMax sign-in method when prompted, and activate **Continue**. Tab moves between controls, Enter opens a selector, and the arrow keys change its selection.

| Sign-in | What to do |
| --- | --- |
| Claude or ChatGPT in the browser | Complete the provider’s sign-in. Return to the terminal after the browser redirects. When the terminal offers a paste field, you can submit the authorization response there. |
| Device sign-in | Open the displayed URL and enter the displayed code. The dialog offers **Copy code**, **Copy URL**, and **Open browser** while it waits for authorization. |
| API key | Use **Open key page** or **Copy URL**, enter the key in the masked field, and activate **Validate and save**. |
| Antigravity (Google AI) | Enter the Google account email you use in Antigravity. Run the displayed `agy --settings …` command in another terminal and complete sign-in there if prompted. Keep the Jackin dialog open until it shows your account. |
| Gemini Code Assist | For an eligible Cloud Code Assist project, set `JACKIN_GOOGLE_OAUTH_CLIENT_FILE` to the registered Desktop app JSON for direct browser consent and managed project discovery. With the variable unset, supply the client ID, optional secret and optional project when asked. Complete the sign-in in the browser and allow its loopback redirect. |

Esc dismisses an active sign-in. After a successful connection, the dialog displays the account and its usage. When you return to **Accounts**, select its saved name under **Usage accounts** to open its readings again. A saved account awaiting its first quota response displays a waiting message. Open **Usage accounts…** in Accounts, or press **N**, to inspect it again. The account opens on its plan and key readings. Choose **Full details** for its complete readings and history. The same dialog offers refresh, sign in again, rename, and remove. When the provider reports reset inventory, select an available reset and review its confirmation before redeeming it.

To check your Ultra subscription, open the saved Antigravity account. Compare **Gemini 5h**, **Gemini weekly**, **Claude 5h** and **Claude weekly** with the quotas in Antigravity. Each percentage is the amount remaining. The account can receive new readings while Antigravity runs with the displayed settings.

A fresh installation opens **Priority**. Under **All Usage**, scroll with the arrow keys or **PgUp/PgDn**. An account with several metrics opens a chooser. A single reading opens directly. For a combined metric, choose a member to inspect its account.

Use **Refresh** for providers that support manual refresh. The console checks polling providers every five minutes while it runs. Antigravity publishes updates during its signed-in CLI sessions.

Open **Actions…** for settings and sharing. Settings control used or remaining amounts, warning thresholds, reset and pace displays, alerts, grouping and name concealment. **Copy report** exports a redacted dashboard. Hidden metrics can be restored through **Undo hide** or the hidden-metric settings.

## Screen gallery

The All Usage page shows each account with a representative reading. The screenshots below use isolated synthetic accounts. Each sign-in screen uses a test provider response. The usage screens use the native binary with snapshots from the corresponding provider reader.

[Accounts](screens/accounts.png) · [Multiple accounts](screens/overview.png) · [13 accounts at 80 × 24](screens/compact.png) · [End of the account list](screens/extended.png) · [Google AI Ultra metric details](screens/antigravity-detail.png)

| Provider | Sign-in prompt at 80 × 24 | Saved readings |
| --- | --- | --- |
| Claude | [Sign-in prompt](screens/claude-prompt.png) | [Usage](screens/claude.png) |
| ChatGPT | [Sign-in prompt](screens/codex-prompt.png) | [Usage](screens/codex.png) |
| Muse | [Sign-in prompt](screens/muse-prompt.png) | [Usage](screens/muse.png) |
| Grok | [Sign-in prompt](screens/grok-prompt.png) | [Usage](screens/grok.png) |
| Kimi | [Sign-in prompt](screens/kimi-prompt.png) | [Usage](screens/kimi.png) |
| Z.ai | [Sign-in prompt](screens/zai-prompt.png) | [Usage](screens/zai.png) |
| MiniMax | [Sign-in prompt](screens/minimax-prompt.png) | [Usage](screens/minimax.png) |
| MiMo | [Sign-in prompt](screens/mimo-prompt.png) | [Usage](screens/mimo.png) |
| Gemini Code Assist | [Sign-in prompt](screens/gemini-prompt.png) | [Usage](screens/gemini.png) |
| OpenCode | [Sign-in prompt](screens/opencode-prompt.png) | [Usage](screens/opencode.png) |
| OpenRouter | [Sign-in prompt](screens/openrouter-prompt.png) | [Usage](screens/openrouter.png) |

## Keyboard fixture transcript

The following excerpts come from the 140 × 40 terminal test backend. The fixtures also check the key field and dashboard value at 80 × 24. The fixtures supply a synthetic sign-in and quota reply. The test enters the API key by keyboard, verifies masking, saves, and checks that a late reply after cancellation cannot reopen the detail view.

```text
Connect usage account
https://example.test/keys
API key
Open key page   Copy URL   Back   Cancel   Validate and save
```

```text
Work · zai
Session · 12% used
Refresh   Sign in again…   Rename…   Remove…   Back   Cancel
```

A separate dashboard fixture begins with a pinned metric at zero remaining. The test hides it, restores its pin through Undo, changes a setting, dismisses the dialog, and opens All Usage.
