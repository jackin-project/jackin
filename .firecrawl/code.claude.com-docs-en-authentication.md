> ## Documentation Index
>
> Fetch the complete documentation index at: [/docs/llms.txt](https://code.claude.com/docs/llms.txt)
>
> Use this file to discover all available pages before exploring further.

[Skip to main content](https://code.claude.com/docs/en/authentication#content-area)

[Claude Code Docs home page![light logo](https://mintcdn.com/claude-code/c5r9_6tjPMzFdDDT/logo/light.svg?fit=max&auto=format&n=c5r9_6tjPMzFdDDT&q=85&s=78fd01ff4f4340295a4f66e2ea54903c)![dark logo](https://mintcdn.com/claude-code/c5r9_6tjPMzFdDDT/logo/dark.svg?fit=max&auto=format&n=c5r9_6tjPMzFdDDT&q=85&s=1298a0c3b3a1da603b190d0de0e31712)](https://code.claude.com/docs/en/overview)

English

Search...

Ctrl KAsk AssistantCTRLI

- [Claude Developer Platform](https://platform.claude.com/)
- [Open Claude Code](https://claude.ai/code)
- [Open Claude Code](https://claude.ai/code)

Search...

Navigation

Setup and access

Authentication

[Getting started](https://code.claude.com/docs/en/overview) [Build with Claude Code](https://code.claude.com/docs/en/agents) [Plugins](https://code.claude.com/docs/en/plugins/overview) [Administration](https://code.claude.com/docs/en/admin-setup) [Configuration](https://code.claude.com/docs/en/settings) [Reference](https://code.claude.com/docs/en/cli-reference) [Agent SDK](https://code.claude.com/docs/en/agent-sdk/overview) [What's New](https://code.claude.com/docs/en/whats-new) [Resources](https://code.claude.com/docs/en/legal-and-compliance)

### Setup and access

- [Administration overview](https://code.claude.com/docs/en/admin-setup)
- [Advanced setup](https://code.claude.com/docs/en/setup)
- [Authentication](https://code.claude.com/docs/en/authentication)
- [Managed settings](https://code.claude.com/docs/en/managed-settings)
- [Server-managed settings](https://code.claude.com/docs/en/server-managed-settings)
- [Managed MCP configuration](https://code.claude.com/docs/en/managed-mcp)
- [Configure auto mode](https://code.claude.com/docs/en/auto-mode-config)

### Deployment

- [Overview](https://code.claude.com/docs/en/third-party-integrations)
- [Feature availability](https://code.claude.com/docs/en/feature-availability)
- [Amazon Bedrock](https://code.claude.com/docs/en/amazon-bedrock)
- [Claude Platform on AWS](https://code.claude.com/docs/en/claude-platform-on-aws)
- [Google Cloud's Agent Platform](https://code.claude.com/docs/en/google-vertex-ai)
- [Microsoft Foundry](https://code.claude.com/docs/en/microsoft-foundry)
- [Network configuration](https://code.claude.com/docs/en/network-config)
- [Corporate launcher](https://code.claude.com/docs/en/corporate-launcher)
- [Development containers](https://code.claude.com/docs/en/devcontainer)

### Gateways

- [Overview](https://code.claude.com/docs/en/gateways)
- Claude apps gateway

- Other gateways


### Usage and costs

- [Monitoring](https://code.claude.com/docs/en/monitoring-usage)
- [Costs](https://code.claude.com/docs/en/costs)
- [Track team usage with analytics](https://code.claude.com/docs/en/analytics)

### Security and data

- [Security](https://code.claude.com/docs/en/security)
- [Data usage](https://code.claude.com/docs/en/data-usage)
- [Zero data retention](https://code.claude.com/docs/en/zero-data-retention)

### Adoption

- [Communications kit](https://code.claude.com/docs/en/communications-kit)
- [Champion kit](https://code.claude.com/docs/en/champion-kit)

## On this page

- [Log in to Claude Code](https://code.claude.com/docs/en/authentication#log-in-to-claude-code)
  - [Log in with multiple accounts](https://code.claude.com/docs/en/authentication#log-in-with-multiple-accounts)
- [Set up team authentication](https://code.claude.com/docs/en/authentication#set-up-team-authentication)
  - [Claude for Teams or Enterprise](https://code.claude.com/docs/en/authentication#claude-for-teams-or-enterprise)
  - [Claude Console authentication](https://code.claude.com/docs/en/authentication#claude-console-authentication)
  - [Sign in without an API key](https://code.claude.com/docs/en/authentication#sign-in-without-an-api-key)
  - [Cloud provider authentication](https://code.claude.com/docs/en/authentication#cloud-provider-authentication)
  - [Restrict login to your organization](https://code.claude.com/docs/en/authentication#restrict-login-to-your-organization)
  - [Restrict which API providers a machine may use](https://code.claude.com/docs/en/authentication#restrict-which-api-providers-a-machine-may-use)
- [Credential management](https://code.claude.com/docs/en/authentication#credential-management)
  - [Renew an expiring login](https://code.claude.com/docs/en/authentication#renew-an-expiring-login)
  - [Authentication precedence](https://code.claude.com/docs/en/authentication#authentication-precedence)
  - [Anthropic profiles and federation credentials](https://code.claude.com/docs/en/authentication#anthropic-profiles-and-federation-credentials)
  - [Generate a long-lived token](https://code.claude.com/docs/en/authentication#generate-a-long-lived-token)

Setup and access

# Authentication

Copy pageCopy page

Log in to Claude Code and configure authentication for individuals, teams, and organizations.

Copy pageCopy page

Claude Code supports multiple authentication methods depending on your setup. Individual users can log in with a claude.ai account, while teams can use Claude for Teams or Enterprise, the Claude Console, or a cloud provider like Amazon Bedrock, Google Cloud’s Agent Platform, or Microsoft Foundry.

## [​](https://code.claude.com/docs/en/authentication\#log-in-to-claude-code)  Log in to Claude Code

After [installing Claude Code](https://code.claude.com/docs/en/setup#install-claude-code), run `claude` in your terminal. On first launch, Claude Code opens a browser window for you to log in. If you’ve set the `ANTHROPIC_API_KEY` environment variable, Claude Code skips the login prompt and asks you to approve the key instead.If the browser doesn’t open automatically, press `c` to copy the login URL to your clipboard, then paste it into your browser.If your browser shows a login code instead of redirecting back after you sign in, paste it into the terminal at the `Paste code here if prompted` prompt. This happens when the browser can’t reach Claude Code’s local callback server, which is common in WSL2, SSH sessions, and containers.When login completes, the terminal shows `Login successful` and prompts you to press `Enter` to continue.You can authenticate with any of these account types:

- **Claude Pro or Max subscription**: log in with your claude.ai account. Subscribe at [claude.com/pricing](https://claude.com/pricing?utm_source=claude_code&utm_medium=docs&utm_content=authentication_pro_max).
- **Claude for Teams or Enterprise**: log in with the claude.ai account your team admin invited you to.
- **Claude Console**: log in with your Console credentials. Your admin must have [invited you](https://code.claude.com/docs/en/authentication#claude-console-authentication) first. You can sign in with or without [creating an API key](https://code.claude.com/docs/en/authentication#sign-in-without-an-api-key).
- **Cloud providers**: if your organization uses [Amazon Bedrock](https://code.claude.com/docs/en/amazon-bedrock), [Google Cloud’s Agent Platform](https://code.claude.com/docs/en/google-vertex-ai), or [Microsoft Foundry](https://code.claude.com/docs/en/microsoft-foundry), set the required environment variables before running `claude`, or select **3rd-party platform** at the login prompt, which launches an interactive setup wizard for Bedrock and Vertex AI. No browser login is needed.
- **Cloud gateway**: if your organization runs a self-hosted [Claude apps gateway](https://code.claude.com/docs/en/claude-apps-gateway), sign in with corporate SSO through `/login`. The gateway-issued token is the session’s only credential.

Admins can direct which login method developers use and require claude.ai logins to belong to a specific organization; see [Restrict login to your organization](https://code.claude.com/docs/en/authentication#restrict-login-to-your-organization).To log out and re-authenticate, type `/logout` at the Claude Code prompt. Logging out also resets your first-launch setup state, so the next time you run `claude` it walks you through login and setup again.If you’re having trouble logging in, see [authentication troubleshooting](https://code.claude.com/docs/en/troubleshoot-install#login-and-authentication).

### [​](https://code.claude.com/docs/en/authentication\#log-in-with-multiple-accounts)  Log in with multiple accounts

To stay signed in to multiple accounts at once, such as work and personal accounts, give each account its own configuration directory. When you start `claude`, set the [`CLAUDE_CONFIG_DIR`](https://code.claude.com/docs/en/env-vars#variables) environment variable to the directory for the account you want to use. Each directory has its own settings, session history, and claude.ai login or API key. For example, in Bash or Zsh, add this alias to `~/.bashrc` or `~/.zshrc` so that `claude-work` uses your work account while `claude` keeps your personal one:

```
alias claude-work='CLAUDE_CONFIG_DIR=~/.claude-work claude'
```

After you open a new terminal and run `claude-work` for the first time, Claude Code walks you through login and setup for the new directory. Separate directories don’t keep two Claude Console sign-ins [without an API key](https://code.claude.com/docs/en/authentication#sign-in-without-an-api-key) apart, because Claude Code stores that kind of sign-in outside the configuration directory.

## [​](https://code.claude.com/docs/en/authentication\#set-up-team-authentication)  Set up team authentication

For teams and organizations, you can configure Claude Code access in one of these ways:

- [Claude for Teams or Enterprise](https://code.claude.com/docs/en/authentication#claude-for-teams-or-enterprise), recommended for most teams
- [Claude Console](https://code.claude.com/docs/en/authentication#claude-console-authentication)
- [Claude apps gateway](https://code.claude.com/docs/en/claude-apps-gateway), a self-hosted gateway that signs developers in with your IdP and routes inference to the cloud provider you configure
- [Amazon Bedrock](https://code.claude.com/docs/en/amazon-bedrock)
- [Google Cloud’s Agent Platform](https://code.claude.com/docs/en/google-vertex-ai)
- [Microsoft Foundry](https://code.claude.com/docs/en/microsoft-foundry)

### [​](https://code.claude.com/docs/en/authentication\#claude-for-teams-or-enterprise)  Claude for Teams or Enterprise

[Claude for Teams](https://claude.com/pricing?utm_source=claude_code&utm_medium=docs&utm_content=authentication_teams#team-&-enterprise) and [Claude for Enterprise](https://anthropic.com/contact-sales?utm_source=claude_code&utm_medium=docs&utm_content=authentication_enterprise) provide the best experience for organizations using Claude Code. Team members get access to both Claude Code and Claude on the web with centralized billing and team management.

- **Claude for Teams**: self-service plan with collaboration features, admin tools, SSO, billing management, and [server-managed settings](https://code.claude.com/docs/en/server-managed-settings) for organization-wide Claude Code configuration. Best for smaller teams.
- **Claude for Enterprise**: adds domain capture, role-based permissions, and the compliance API. Best for larger organizations with security and compliance requirements.

1

Subscribe

Subscribe to [Claude for Teams](https://claude.com/pricing?utm_source=claude_code&utm_medium=docs&utm_content=authentication_teams_step#team-&-enterprise) or contact sales for [Claude for Enterprise](https://anthropic.com/contact-sales?utm_source=claude_code&utm_medium=docs&utm_content=authentication_enterprise_step).

2

Invite team members

Invite team members from the admin dashboard.

3

Install and log in

Team members install Claude Code and log in with their claude.ai accounts.

### [​](https://code.claude.com/docs/en/authentication\#claude-console-authentication)  Claude Console authentication

For organizations that prefer API-based billing, you can set up access through the Claude Console.

1

Create or use a Console account

Use your existing Claude Console account or create a new one.

2

Add users

You can add users through either method:

- Bulk invite users from within the Console: Settings -> Members -> Invite
- [Set up SSO](https://support.claude.com/en/articles/13132885-setting-up-single-sign-on-sso)

3

Assign roles

When inviting users, assign one of:

- **Claude Code** role: users can only create Claude Code API keys
- **Developer** role: users can create any kind of API key

4

Users complete setup

Each invited user needs to:

- Accept the Console invite
- [Check system requirements](https://code.claude.com/docs/en/setup#system-requirements)
- [Install Claude Code](https://code.claude.com/docs/en/setup#install-claude-code)
- Log in with Console account credentials

#### [​](https://code.claude.com/docs/en/authentication\#sign-in-without-an-api-key)  Sign in without an API key

You can sign in to your Console account without creating an API key, even when your organization doesn’t let developers create them. Choose the Anthropic Console account at the `/login` prompt and Claude Code asks how you want to sign in. Requires Claude Code v2.1.242 or later. Both routes sign you in to Console in the browser and differ in what Claude Code stores afterwards:

- **Sign in with your Console account**, labeled `(recommended)`: Claude Code keeps the OAuth token from that sign-in and stores it as an [Anthropic profile](https://code.claude.com/docs/en/authentication#anthropic-profiles-and-federation-credentials). It creates no API key
- **Create an API key**, labeled `(legacy)`: Claude Code creates a Console API key for you and stores it with your other credentials

In practice, the profile stores an OAuth login while an API key is a static credential: Claude Code refreshes the profile’s login automatically, and when refresh fails, requests fail with [Anthropic profile login expired](https://code.claude.com/docs/en/errors#anthropic-profile-login-expired) until you sign in again.You don’t get the choice on every machine. Claude Code creates an API key without asking in these cases:

- You run against a cloud provider, such as [Amazon Bedrock, Google Cloud’s Agent Platform, or Microsoft Foundry](https://code.claude.com/docs/en/third-party-integrations) or [Claude Platform on AWS](https://code.claude.com/docs/en/claude-platform-on-aws)
- Any settings file sets [`forceLoginOrgUUID`](https://code.claude.com/docs/en/authentication#restrict-login-to-your-organization), or sets `forceLoginMethod` to `"claudeai"` or `"console"`
- A managed settings source on your machine, such as the managed settings file, an MDM profile, or the cached server-managed settings, exists but Claude Code [can’t read it](https://code.claude.com/docs/en/managed-settings#invalid-entries-in-managed-settings) and no other managed source supplies a policy

Unset `ANTHROPIC_API_KEY` before you sign in without a key.After you sign in without a key, you have a profile instead of a stored API key:

- **Which profile it writes**: Claude Code writes the profile named by `ANTHROPIC_PROFILE`, or your active profile, or `default`. If that profile is a federation profile, Claude Code refuses the sign-in instead of overwriting it
- **What it signs you out of**: Claude Code signs you out of any claude.ai login stored on the machine
- **How to undo it**: run `/logout`, which removes and revokes the credential this sign-in wrote

If your organization uses [server-managed settings](https://code.claude.com/docs/en/server-managed-settings), they apply to this sign-in on Claude Code v2.1.257 or later.Everything else about profiles applies to this sign-in, including where it ranks against your other credentials, the `Profile` row you get in `/status`, and the features that need a claude.ai login. See [Anthropic profiles and federation credentials](https://code.claude.com/docs/en/authentication#anthropic-profiles-and-federation-credentials).

### [​](https://code.claude.com/docs/en/authentication\#cloud-provider-authentication)  Cloud provider authentication

For teams using Amazon Bedrock, Google Cloud’s Agent Platform, or Microsoft Foundry:

1

Follow provider setup

Follow the [Amazon Bedrock docs](https://code.claude.com/docs/en/amazon-bedrock), [Google Cloud’s Agent Platform docs](https://code.claude.com/docs/en/google-vertex-ai), or [Microsoft Foundry docs](https://code.claude.com/docs/en/microsoft-foundry).

2

Distribute configuration

Distribute the environment variables and instructions for generating cloud credentials to your users. Read more about how to [manage configuration here](https://code.claude.com/docs/en/settings).

3

Install Claude Code

Users can [install Claude Code](https://code.claude.com/docs/en/setup#install-claude-code).

### [​](https://code.claude.com/docs/en/authentication\#restrict-login-to-your-organization)  Restrict login to your organization

To require that developers’ claude.ai logins belong to a specific Anthropic organization, set [`forceLoginMethod`](https://code.claude.com/docs/en/settings-reference#forceloginmethod) and [`forceLoginOrgUUID`](https://code.claude.com/docs/en/settings-reference#forceloginorguuid) in [managed settings](https://code.claude.com/docs/en/managed-settings). Set `forceLoginOrgUUID` to your organization ID, shown in [claude.ai admin settings](https://claude.ai/admin-settings/organization) for Claude for Teams or Enterprise organizations. Claude Code reports an error for a claude.ai login to any other organization and exits at startup if the claude.ai credential in use belongs to an organization that isn’t listed.For Claude Console logins, Claude Code uses `forceLoginOrgUUID` to pre-select the organization on the Console sign-in page when you set it to a single Console organization ID, shown at [platform.claude.com/settings/organization](https://platform.claude.com/settings/organization). It doesn’t check which organization the resulting Console credential belongs to, at login or at startup. A developer who logged in with a Console account before you deployed the keys stays logged in, and that saved key is blocked on a machine that also requires the [gateway](https://code.claude.com/docs/en/claude-apps-gateway) sign-in or in a session that selects a cloud provider.If you set `forceLoginOrgUUID` in any settings file, Claude Code stops offering the [keyless Console sign-in](https://code.claude.com/docs/en/authentication#sign-in-without-an-api-key) in the sessions that file applies to and creates an API key instead. To direct developers to claude.ai sign-in instead, set `forceLoginMethod` to `"claudeai"`.On Claude Code v2.1.212 or later, every login path listed here applies `forceLoginMethod`. On the terminal’s interactive login screen, reached by `/login` or first-run onboarding, Claude Code pre-selects a `claudeai` or `console` method without enforcing it, so even with `forceLoginMethod` set to `"claudeai"`, a developer can still complete a Console login there.The paths differ on `forceLoginOrgUUID`:

- **Terminal, [VS Code extension](https://code.claude.com/docs/en/vs-code), and Agent SDK logins**: verify `forceLoginOrgUUID` for claude.ai account logins
- **`claude setup-token` and `/install-github-app`**: enforce only `forceLoginMethod`, so they can mint a token in a different organization
- **[Gateway](https://code.claude.com/docs/en/claude-apps-gateway) sign-in**: selected by `forceLoginMethod: "gateway"` rather than restricted by it, and doesn’t authenticate against an Anthropic organization, so `forceLoginOrgUUID` doesn’t apply; use your gateway identity provider to restrict access

Deploy the keys through your device management tooling. [Server-managed settings](https://code.claude.com/docs/en/server-managed-settings) reach only accounts that are already authenticated into your organization, so they can’t redirect a developer’s first login. If your organization distributes server-managed settings as well, set the keys in both places: managed-settings sources [don’t merge](https://code.claude.com/docs/en/server-managed-settings#settings-precedence), and cached server-managed settings replace the device-managed file apart from a few [per-key exceptions](https://code.claude.com/docs/en/server-managed-settings#per-key-exceptions-across-managed-sources). `forceLoginOrgUUID` and the `"claudeai"` and `"console"` values of `forceLoginMethod` aren’t among those exceptions, so keep them in both places.In a [gateway](https://code.claude.com/docs/en/claude-apps-gateway) deployment, also keep `forceLoginMethod` and `forceLoginOrgUUID` out of the [settings the gateway serves](https://code.claude.com/docs/en/claude-apps-gateway-config#managed).The keys also decide whether a session that doesn’t use a login credential can start. See [`forceLoginOrgUUID`](https://code.claude.com/docs/en/settings-reference#forceloginorguuid) in the settings reference for the full behavior.

- **`ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, or `apiKeyHelper`**: blocked at startup. Under `forceLoginOrgUUID`, organization membership can’t be verified for an environment credential, and under `forceLoginMethod` the credential would stand in for the required sign-in. When the managed settings also require the [gateway](https://code.claude.com/docs/en/claude-apps-gateway) sign-in, Claude Code blocks an API key saved by an earlier Claude Console login the same way. See [Administrator policy requires a Cloud gateway sign-in](https://code.claude.com/docs/en/errors#administrator-policy-requires-a-cloud-gateway-sign-in)
- **Cloud provider sessions such as Amazon Bedrock**: blocked only while an `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, or `apiKeyHelper` credential, or an API key saved by an earlier Claude Console login, is still present on the machine. Remove it and the session starts. These sessions authenticate against your cloud provider, whose access policies govern them
- **[Anthropic profile or federation credentials](https://code.claude.com/docs/en/authentication#anthropic-profiles-and-federation-credentials)**: not blocked unless an `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, or `apiKeyHelper` credential, or an API key saved by an earlier Claude Console login, is also present on the machine. The keys don’t check which organization the profile belongs to

### [​](https://code.claude.com/docs/en/authentication\#restrict-which-api-providers-a-machine-may-use)  Restrict which API providers a machine may use

[`allowedProviders`](https://code.claude.com/docs/en/settings-reference#allowedproviders) in [managed settings](https://code.claude.com/docs/en/managed-settings) lists which services a managed machine may reach Claude through, such as the Anthropic API, Amazon Bedrock, or an LLM gateway. It complements `forceLoginMethod` and `forceLoginOrgUUID`, which govern which account a session uses when it talks to Anthropic. Requires Claude Code v2.1.285 or later.

managed-settings.json

```
{
  "forceLoginMethod": "claudeai",
  "forceLoginOrgUUID": ["xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"],
  "allowedProviders": ["anthropic", "bedrock"]
}
```

With this file, a developer signed in to your claude.ai organization or configured for Amazon Bedrock starts normally. A session set up for any other provider is refused at startup, and a running session that switches to one is refused on its next request. [Managed settings don’t allow this API provider](https://code.claude.com/docs/en/errors#managed-settings-dont-allow-this-api-provider) shows each message.

- **Allow an LLM gateway or proxy**: list `"customEndpoint"` and set the gateway’s URL in the managed `env` block of the same source. The [settings reference](https://code.claude.com/docs/en/settings-reference#allowedproviders) lists every value and says which endpoint variables need a managed `env` pin.
- **Deploy on managed machines**: put the list in the managed source that carries the rest of your policy. The entry’s [Scope note](https://code.claude.com/docs/en/settings-reference#allowedproviders) says how a server-managed list combines with it.
- **Server-managed settings only**: a list you set only in [server-managed settings](https://code.claude.com/docs/en/server-managed-settings) reaches only sessions that fetch your organization’s settings, so treat it as a convenience for machines you can’t reach with device management, not as enforcement. [Platform availability](https://code.claude.com/docs/en/server-managed-settings#platform-availability) lists which sessions fetch them.

## [​](https://code.claude.com/docs/en/authentication\#credential-management)  Credential management

Claude Code securely manages your authentication credentials:

- **Storage location**:
  - On macOS, credentials are stored in the encrypted macOS Keychain. When the Keychain rejects the write, such as when it’s locked in an SSH session, Claude Code stores your login in `~/.claude/.credentials.json` with file mode `0600` instead, the same storage it uses on Linux. A Console login that creates an API key fails until the Keychain is writable. To move your login back into the Keychain, follow [the recovery steps](https://code.claude.com/docs/en/troubleshoot-install#not-logged-in-or-token-expired).
  - On Linux, credentials are stored in `~/.claude/.credentials.json` with file mode `0600`.
  - On Windows, credentials are stored in `%USERPROFILE%\.claude\.credentials.json` and inherit the access controls of your user profile directory, which restricts the file to your user account by default.
  - If you’ve set the `CLAUDE_CONFIG_DIR` environment variable, Claude Code keeps the `.credentials.json` file under that directory instead, including the file the macOS fallback writes, and keys the macOS Keychain entry to that directory too, so a session with a different `CLAUDE_CONFIG_DIR` reads a different entry.
  - Claude Code manages `.credentials.json` through `/login` and `/logout`. To route requests through a custom API endpoint, set the [`ANTHROPIC_BASE_URL`](https://code.claude.com/docs/en/env-vars) environment variable instead.
- **Supported authentication types**: claude.ai credentials, Claude API credentials, Microsoft Foundry Auth, Bedrock Auth, Vertex Auth, Anthropic profile and [Workload Identity Federation](https://platform.claude.com/docs/en/manage-claude/workload-identity-federation) credentials, and [Claude apps gateway](https://code.claude.com/docs/en/claude-apps-gateway) session tokens.
- **Custom credential scripts**: configure the [`apiKeyHelper`](https://code.claude.com/docs/en/settings-reference#apikeyhelper) setting to run a shell script that returns an API key.
- **Refresh intervals**: see [`apiKeyHelper`](https://code.claude.com/docs/en/settings-reference#apikeyhelper) for the cases in which Claude Code re-runs the helper.
- **Slow helper notice**: if `apiKeyHelper` takes longer than 10 seconds to return a key, Claude Code displays a warning notice in the prompt bar showing the elapsed time. If you see this notice regularly, check whether your credential script can be optimized.
- **Helper failures**: when the script exits with an error, times out, or prints nothing, requests fail with [`Your apiKeyHelper script is failing`](https://code.claude.com/docs/en/errors#your-apikeyhelper-script-is-failing) within three attempts.

`apiKeyHelper`, `ANTHROPIC_API_KEY`, and `ANTHROPIC_AUTH_TOKEN` apply to the CLI and the surfaces that wrap it, including the VS Code extension, the Agent SDK, and GitHub Actions. Claude Desktop and cloud sessions do not call `apiKeyHelper` or read these environment variables: they use OAuth, except desktop sessions running a [third-party inference configuration](https://code.claude.com/docs/en/llm-gateway-connect#desktop-app), which authenticate with that configuration’s credential.

### [​](https://code.claude.com/docs/en/authentication\#renew-an-expiring-login)  Renew an expiring login

When the login you created with `/login` is within three days of expiring, Claude Code shows a warning at startup: `Your login expires in 3 days · run /login to renew`.Run `/login` to renew. The warning is informational and never blocks a request: authentication keeps working until the login actually expires.Once the stored login expires and can’t be refreshed, each model request fails with [`Login expired · Please run /login`](https://code.claude.com/docs/en/errors#login-expired) until you sign in again.You can check for this state before a request fails: [`/status`](https://code.claude.com/docs/en/commands) shows a `Login` row reading `Expired — log in again`, plus the organization and email it has saved for the expired login. The row appears only when the saved claude.ai login is the active credential. The row requires Claude Code v2.1.210 or later.The warning appears only when a claude.ai login is the active credential, and not when a cloud provider, `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, or `apiKeyHelper` supplies the credential.Renewing early matters most for sessions that run unattended. A [background session in agent view](https://code.claude.com/docs/en/agent-view) or a [Remote Control](https://code.claude.com/docs/en/remote-control) session that outlives the login stops making progress once the credential expires and can’t recover until you sign in again.

### [​](https://code.claude.com/docs/en/authentication\#authentication-precedence)  Authentication precedence

When multiple credentials are present, Claude Code chooses one in this order:

1. Cloud provider credentials, when `CLAUDE_CODE_USE_BEDROCK`, `CLAUDE_CODE_USE_VERTEX`, or `CLAUDE_CODE_USE_FOUNDRY` is set. See [third-party integrations](https://code.claude.com/docs/en/third-party-integrations) for setup.
2. `ANTHROPIC_AUTH_TOKEN` environment variable. Sent as the `Authorization: Bearer` header. Use this when routing through an [LLM gateway or proxy](https://code.claude.com/docs/en/llm-gateway) that authenticates with bearer tokens rather than Anthropic API keys.
3. `ANTHROPIC_API_KEY` environment variable. Sent as the `X-Api-Key` header. Use this for direct Anthropic API access with a key from the [Claude Console](https://platform.claude.com/). In interactive mode, you are prompted once to approve or decline the key, and your choice is remembered. To change it later, use the “Use custom API key” toggle in `/config`. The toggle only appears while `ANTHROPIC_API_KEY` is set in your environment. In non-interactive mode (`-p`), the key is always used when present.
4. [`apiKeyHelper`](https://code.claude.com/docs/en/settings-reference#apikeyhelper) script output. Use this for dynamic or rotating credentials, such as short-lived tokens fetched from a vault.
5. `CLAUDE_CODE_OAUTH_TOKEN` environment variable. A long-lived OAuth token generated by [`claude setup-token`](https://code.claude.com/docs/en/authentication#generate-a-long-lived-token). Use this for CI pipelines and scripts where browser login isn’t available. If you run `/login` while the variable is set, Claude Code switches the current session to the new login, but reads the variable again in every new session until you remove it from your shell profile or the `env` block of a [settings file](https://code.claude.com/docs/en/settings).
6. Anthropic profile and federation credentials, the credentials that the `ant` CLI and Workload Identity Federation use. A profile that `ant auth login` wrote ranks here only when you name it in `ANTHROPIC_PROFILE`; otherwise it ranks below `/login`. See [Anthropic profiles and federation credentials](https://code.claude.com/docs/en/authentication#anthropic-profiles-and-federation-credentials).
7. Subscription OAuth credentials from `/login`. This is the default for Claude Pro, Max, Team, and Enterprise users.

A signed-in [Claude apps gateway](https://code.claude.com/docs/en/claude-apps-gateway) session sits outside this list: it is a provider selection like Amazon Bedrock or Google Cloud’s Agent Platform, and it outranks them. When a gateway session exists, the CLI authenticates with the gateway token even if `CLAUDE_CODE_USE_BEDROCK`, `CLAUDE_CODE_USE_VERTEX`, or `CLAUDE_CODE_USE_FOUNDRY` is set, and credential sources above such as the bearer token, API key, `apiKeyHelper`, and profiles are not used.If your machine’s [managed settings](https://code.claude.com/docs/en/managed-settings) set [`forceLoginMethod`](https://code.claude.com/docs/en/settings-reference#forceloginmethod) to `"gateway"` or set [`forceLoginGatewayUrl`](https://code.claude.com/docs/en/settings-reference#forcelogingatewayurl), and you don’t select a cloud provider through a variable such as `CLAUDE_CODE_USE_BEDROCK` or `CLAUDE_CODE_USE_VERTEX`, your session uses only the gateway sign-in. Claude Code skips the other credential sources and asks you to sign in with `/login`. See [Administrator policy requires a Cloud gateway sign-in](https://code.claude.com/docs/en/errors#administrator-policy-requires-a-cloud-gateway-sign-in) for what you see with each leftover credential. Requires Claude Code v2.1.261 or later, or v2.1.265 or later on a machine that sets only `forceLoginGatewayUrl`.If you have an active Claude subscription but also have `ANTHROPIC_API_KEY` set in your environment, Claude Code uses the API key once you approve it. This can cause authentication failures if the key belongs to a disabled or expired organization.Run `unset ANTHROPIC_API_KEY` to fall back to your subscription, and check `/status` to confirm which method is active. When a login and an API key are both configured, `/status` marks the credential that isn’t in use.[Cloud sessions](https://code.claude.com/docs/en/claude-code-on-the-web) always use your subscription credentials. If you set `ANTHROPIC_API_KEY` or `ANTHROPIC_AUTH_TOKEN` in the cloud environment, it doesn’t override your subscription credentials.

#### [​](https://code.claude.com/docs/en/authentication\#anthropic-profiles-and-federation-credentials)  Anthropic profiles and federation credentials

A profile is a named credential configuration file in your [Anthropic configuration directory](https://platform.claude.com/docs/en/manage-claude/wif-reference#configuration-directory), by default `~/.config/anthropic` on macOS and Linux or `%APPDATA%\Anthropic` on Windows. A profile’s auth mode is `oidc_federation` when you set it up for [Workload Identity Federation (WIF)](https://platform.claude.com/docs/en/manage-claude/workload-identity-federation) or `user_oauth` when [`ant auth login`](https://platform.claude.com/docs/en/cli-sdks-libraries/cli/authentication) wrote it or you [signed in to a Console account without an API key](https://code.claude.com/docs/en/authentication#sign-in-without-an-api-key).Claude Code doesn’t read profiles or federation variables in [bare mode](https://code.claude.com/docs/en/headless#start-faster-with-bare-mode), in Claude Desktop, or in cloud sessions. In those sessions, `/status` shows no `Profile` row.Claude Code checks three sources in this order and stops at the first one that is set. The table shows what sets each source and where it ranks against your `/login` credential.

| Source | Set by | Rank against `/login` |
| --- | --- | --- |
| Named profile | `ANTHROPIC_PROFILE` | Above, whichever auth mode the profile has |
| Federation variables | `ANTHROPIC_FEDERATION_RULE_ID` and `ANTHROPIC_ORGANIZATION_ID`, both set | Above |
| Active profile | The [`active_config` file](https://platform.claude.com/docs/en/manage-claude/wif-reference#active-profile) in your configuration directory, or a profile named `default` | Above when its auth mode is `oidc_federation`; below a working `/login` credential when its auth mode is `user_oauth` |

For the federation variables, Claude Code also reads the other variables in the [WIF reference](https://platform.claude.com/docs/en/manage-claude/wif-reference#environment-variables), such as `ANTHROPIC_IDENTITY_TOKEN_FILE`, when it exchanges your identity token. For the profile file format, see the [WIF reference](https://platform.claude.com/docs/en/manage-claude/wif-reference#profile-configuration-file).To confirm which source Claude Code chose, run `/status`. A `Profile` row names the source in place of the `Login method` row. When the profile is the credential in use, `Organization` and `Email` rows show its account.When a `user_oauth` profile’s login has expired and Claude Code can’t renew it, requests fail with [Anthropic profile login expired](https://code.claude.com/docs/en/errors#anthropic-profile-login-expired).Features that need your claude.ai login, such as [claude.ai connectors](https://code.claude.com/docs/en/mcp#use-mcp-servers-from-claude-ai) and [`/schedule`](https://code.claude.com/docs/en/routines), aren’t available while one of these sources is selected. To stop Claude Code from selecting a source:

- **Named profile or federation variables**: unset `ANTHROPIC_PROFILE`, or unset either federation variable
- **Active profile**: run `/logout` for a `user_oauth` profile whose current credential you wrote by [signing in to a Console account without an API key](https://code.claude.com/docs/en/authentication#sign-in-without-an-api-key), run `ant auth logout` for one whose current credential `ant auth login` wrote, or delete the profile’s file from `configs/` in your configuration directory for either auth mode

### [​](https://code.claude.com/docs/en/authentication\#generate-a-long-lived-token)  Generate a long-lived token

For CI pipelines, scripts, or other environments where interactive browser login isn’t available, generate a one-year OAuth token with `claude setup-token`:

```
claude setup-token
```

The command opens the same browser authorization flow as `/login`, and the token prints to the terminal after you approve access in the browser. It does not save the token anywhere; copy it and set it as the `CLAUDE_CODE_OAUTH_TOKEN` environment variable wherever you want to authenticate:

- macOS, Linux, WSL

- Windows PowerShell

- Windows CMD


```
export CLAUDE_CODE_OAUTH_TOKEN=your-token
```

```
$env:CLAUDE_CODE_OAUTH_TOKEN = "your-token"
```

```
set CLAUDE_CODE_OAUTH_TOKEN=your-token
```

This token authenticates with your Claude subscription and requires a Pro, Max, Team, or Enterprise plan. It can only make model requests, so it can’t establish [Remote Control](https://code.claude.com/docs/en/remote-control) sessions or fetch [claude.ai connectors](https://code.claude.com/docs/en/mcp#use-mcp-servers-from-claude-ai). MCP servers you configure locally still work.[Bare mode](https://code.claude.com/docs/en/headless#start-faster-with-bare-mode) does not read `CLAUDE_CODE_OAUTH_TOKEN`. If your script passes `--bare`, authenticate with `ANTHROPIC_API_KEY` or an `apiKeyHelper` instead.

Was this page helpful?

YesNo

[Advanced setup](https://code.claude.com/docs/en/setup) [Managed settings](https://code.claude.com/docs/en/managed-settings)

[Claude Code Docs home page![light logo](https://mintcdn.com/claude-code/c5r9_6tjPMzFdDDT/logo/light.svg?fit=max&auto=format&n=c5r9_6tjPMzFdDDT&q=85&s=78fd01ff4f4340295a4f66e2ea54903c)![dark logo](https://mintcdn.com/claude-code/c5r9_6tjPMzFdDDT/logo/dark.svg?fit=max&auto=format&n=c5r9_6tjPMzFdDDT&q=85&s=1298a0c3b3a1da603b190d0de0e31712)](https://code.claude.com/docs/en/overview)

[x](https://x.com/AnthropicAI) [linkedin](https://www.linkedin.com/company/anthropicresearch)

Company

[Anthropic](https://www.anthropic.com/company) [Careers](https://www.anthropic.com/careers) [Economic Futures](https://www.anthropic.com/economic-futures) [Research](https://www.anthropic.com/research) [News](https://www.anthropic.com/news) [Trust center](https://trust.anthropic.com/) [Transparency](https://www.anthropic.com/transparency)

Help and security

[Availability](https://www.anthropic.com/supported-countries) [Status](https://status.anthropic.com/) [Support center](https://support.claude.com/)

Learn

[Courses](https://academy.claude.com/) [MCP connectors](https://claude.com/partners/mcp) [Customer stories](https://www.claude.com/customers) [Engineering blog](https://www.anthropic.com/engineering) [Events](https://www.anthropic.com/events) [Powered by Claude](https://claude.com/partners/powered-by-claude) [Service partners](https://claude.com/partners/services) [Startups program](https://claude.com/programs/startups)

Terms and policies

[Privacy choices](https://www.anthropic.com/legal/privacy) [Privacy policy](https://www.anthropic.com/legal/privacy) [Disclosure policy](https://www.anthropic.com/responsible-disclosure-policy) [Usage policy](https://www.anthropic.com/legal/aup) [Commercial terms](https://www.anthropic.com/legal/commercial-terms) [Consumer terms](https://www.anthropic.com/legal/consumer-terms)

Assistant

Responses are generated using AI and may contain mistakes.