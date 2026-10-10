# Usage monitor protocol v8 and durable schema 4

Contract freeze: 2026-10-10. This document describes the grouped usage
integration contract and supersedes the v2 wire/schema numbers recorded in
older implementation notes. It is contract documentation, not installed-binary
or native-platform verification.

The broker wire protocol is v8, durable monitor state is schema 4, and
normalized statusline input remains schema 2. Statusline input and persisted
monitor state are separate contracts: a schema 4 state must not cause a schema
2 callback to be rejected or relabeled.

Wire v8 adds `UsageIdentityKindV1::LocalSourceHandle` so a locally derived
credential-source capability is never mislabeled as provider-issued identity.
The R37 amendment also adds `UsageIdentityKindV1::UnverifiedHandle` for legacy
identity values whose provenance cannot be proved. It carries no provider or
local-source authority. Provider account IDs and provider stable handles
remain reserved for evidence issued by the provider. A v7 peer is rejected by
the protocol/build handshake; there is no dual-version or compatibility path.

The durable projection envelope has its own schema number: this integration
writes schema 3, accepts schema 2 only through the broker startup migration,
and preserves valid future schema bytes while failing closed. The projection
payload remains `UsageProjectionSchemaV1`; per-account state remains schema 2,
monitor state remains schema 4, and statusline input remains schema 2.

Schema 4 adds source binding and consent fields. `MonitorAccountBindingInput`
and persisted `MonitorAccountBinding` carry `provider_account_id` and
`experimental_collector_approved`. Existing snapshots deserialize those fields
as absent and false. `MonitorConfig.experimental_collector` also defaults to
false. `MonitorServiceStatus.experimental_collector_source` is either absent
for a passive broker or the opaque source capability selected by an active
foreground service.

`account_id` remains the operator's local account partition and label.
`provider_account_id` is a 64-character lowercase hexadecimal capability ID:
the SHA-256 `account_key_hash` for the exact selected Claude Keychain service,
with the `sha256:` prefix stripped. It identifies the local source capability;
it is not authenticated provider account evidence. The CLI names this input
`--source-capability-id`; the wire field keeps its frozen `provider_account_id`
name. Mapping the ID to a local account requires separate operator
confirmation. The experimental collector
requires an additional explicit approval on that same binding, a Claude
account-bound observe-only monitor opt-in, an exact match with the foreground
lease, and a fresh consent fence for each collection request. A dispatch guard
cannot enable the collector.

The service status may expose the opaque capability ID so an operator can map
the foreground source. It must never expose the raw Keychain service or
credential. The ID is persisted only in the local account binding needed for
consent; it is excluded from provider state, telemetry, and diagnostics. The
raw Keychain service and credential are never persisted or emitted. The
foreground `usage auth prepare` command
requires an attached terminal and holds the acquired lease while its broker
process runs. Its `--prepare-auth` binary entry is a command-line bootstrap,
not a monitor RPC. The `PrepareAuth` monitor operation and `AuthPrepared` reply
are removed; there is no alias or compatibility path.

V2 and V3 durable snapshots migrate to schema 4 only after source validation.
New consent remains disabled. V3 strict goals whose retained anchors show a
rollover are marked incomplete while preserving their known cumulative
estimate. The account correction horizon records uncertainty rather than a
received correction and cannot be cleared by newer receipts. A rejected or
future schema remains untouched and fails closed; successful migration is
published through the existing fsync-and-rename path.
