# Diagnosing `auth_malformed`

`auth prepare` reads one selected macOS Keychain item and checks its bounded
Claude credential shape. It does not contact Anthropic or verify that the
credential is current. A malformed result means the returned UTF-8 payload was
over 65,536 bytes or did not parse as a document with a nonblank access token.
Invalid UTF-8 is reported as `auth_missing`, not `auth_malformed`.

## Run it from an operator terminal

Run the command directly with stdin, stdout, and stderr attached to a terminal.
Do not pipe, redirect, background, or record the terminal session. The
malformed diagnostic is returned in the error JSON only after this TTY gate.

```text
jackin usage --data-dir PATH auth prepare --provider claude
```

With no `--keychain-service`, Jackin searches for at most one generic-password
item whose service is exactly `Claude Code-credentials`, and loads its data. If
you pass `--keychain-service SERVICE`, Jackin searches that exact service. It
does not try another service, match by account name, or fall back to a file or
environment variable. For a custom Claude config directory, supply the exact
service associated with that directory; `auth prepare` does not derive it from
`CLAUDE_CONFIG_DIR`.

The installed v3 binary from source commit `8abfa235` predates this diagnostic
field. It returns only `error.code` and `error.message`; the fields below are
available in a build containing the diagnostic change.

## Read the diagnostic

For `auth_malformed`, `error.diagnostic` reports only fixed structural facts:

| Field | Meaning |
| --- | --- |
| `payload_bytes`, `limit_bytes` | Total payload bytes and the 65,536-byte bound. |
| `json` | `skipped_oversize`, `invalid`, or `valid`. |
| `root` | JSON kind of the root value. |
| `oauth_container`, `account_container`, `email_address`, `organization_type` | `camel_case`, `snake_case`, and `duplicate_alias` facts for the named field spellings. |
| `access_token` | Camel and snake field kinds, their optional `camel_case_nonempty` and `snake_case_nonempty` booleans, and `duplicate_alias`. |
| `subscription_type` | Kinds for `subscription_type`, `subscription_type_snake_case`, `rate_limit_tier`, and `rate_limit_tier_snake_case`, plus `duplicate_alias`. |

Field kinds are `unavailable`, `missing`, `null`, `object`, `string`, `number`,
`boolean`, or `array`. `unavailable` means the bounded diagnostic could not
classify that field; it is not the same as `missing`. A nonempty boolean is
`null` when the field is absent, null, not a string, or contains JSON escapes;
escaped string content is left undecoded by the diagnostic. `duplicate_alias: true`
means more than one supported spelling was present for that field.

The credential parser accepts an object containing `claudeAiOauth` or
`claude_ai_oauth`, with a nonblank string `accessToken` or `access_token`.
`oauthAccount`/`oauth_account` and the recognized account and tier metadata are
optional. Recognized optional string fields may be absent or null; another JSON
type can reject the whole document even when the access token is a string.
Unknown fields are ignored. The diagnostic never includes field values, token
length, email, service or account IDs, snippets, or unknown keys. It uses a
fixed set of field names and JSON kinds.

If the diagnostic identifies a missing, blank, or wrongly typed access token,
use Claude Code's own login flow for the intended config directory to rewrite
its credentials; do not edit or copy credential JSON by hand. If the metadata
fields have unexpected types, reauthentication through Claude Code is also the
bounded way to regenerate its own document. If the service is wrong, rerun
with the exact intended service rather than probing other Keychain items.

## What this does not establish

`auth_malformed` is a structural or size result, not an expiry result. The
parser does not inspect `expiresAt`, and a nonblank expired token passes this
shape check. `auth prepare` makes no provider request; the malformed path does
not start a ready service or generate usage. Provider collection requires a
separate explicit source binding and experimental observer opt-in, described
in the [bootstrap contract](bootstrap-contract.md).

Fixtures demonstrate parser behavior for synthetic payloads only. They do not
show the operator's Keychain contents, identify the account behind an item, or
prove that an account is authenticated or expired. Do not inspect or share
Keychain contents. If asking for help, share only `error.code`,
`error.message`, and `error.diagnostic` from the error object.
