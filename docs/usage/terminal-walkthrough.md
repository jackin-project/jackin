# Connect and inspect a usage account in the terminal

Open **Accounts** in the jackin console. Select **Add account…** and press Enter, or press **n**. Select a provider, choose an account region, MiMo product, or Kimi/MiniMax sign-in method when prompted, and activate **Continue**. Tab moves between controls, Enter opens a selector, and the arrow keys change its selection.

| Sign-in | What to do |
| --- | --- |
| Claude or ChatGPT in the browser | Complete the provider’s sign-in. Return to the terminal after the browser redirects. When the terminal offers a paste field, you can submit the authorization response there. |
| Device sign-in | Open the displayed URL and enter the displayed code. The dialog offers **Copy code**, **Copy URL**, and **Open browser** while it waits for authorization. |
| API key | Use **Open key page** or **Copy URL**, enter the key in the masked field, and activate **Validate and save**. |
| Gemini | Supply the desktop OAuth client ID, optional client secret, and optional Cloud project when asked. Complete the sign-in in the browser and allow its loopback redirect. |

Esc dismisses an active sign-in. After a successful connection, the dialog displays the account and its usage. A saved account awaiting its first quota response displays a waiting message. Open **Usage accounts…** in Accounts, or press **N**, to inspect it again. The account details offer refresh, sign in again, rename, and remove. When the provider reports reset inventory, select an available reset and review its confirmation before redeeming it.

Opening **Usage** shows Priority by default on a fresh installation or once you have connected a usage account. An installation with only execution accounts opens their usage view. Use **Settings…** to make All Usage your default. After closing a board, press **p** for Priority or **a** for All Usage. From an empty board, choose **Add account…**. For a failed account, use the **Needs attention** selector and choose **Fix…** to open that account. Select a metric and activate **Details** for its value, reset time, and other fields supplied by the provider. A combined metric offers a member selector and **Inspect account**. Use **Refresh all** on the board or **Refresh provider** in a metric’s details. Automatic refresh checks run every five minutes while the console is running.

In **Settings…**, choose used or remaining amounts, warning and critical thresholds, reset and pace displays, alerts, account grouping, and name concealment. Use **Network proxy…** to configure the provider connection. Hide a metric in its details, use **Undo hide** to restore it immediately, or select it later under **Hidden metric**. **Copy report** exports the redacted dashboard.

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

```text
Priority
Two hours to reset
Pinned
Work quota · 0% remaining
All Usage   Settings…   Refresh all   Copy report   Cancel   Details
```
