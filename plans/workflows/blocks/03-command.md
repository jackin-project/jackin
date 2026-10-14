# B03 — Command block

**Status:** Proposed block contract. Every field in the [common node contract](../architecture/02-domain-contracts.md) is required after template/default resolution: identity, typed inputs/outputs, dependencies, permissions, limits, retry and completion.

## Purpose

Run a declared deterministic command inside an authorized sandbox. Do not expose arbitrary host shell execution through the workflow graph.

## API: configuration and ports

`kind: command`. Configuration: executable path/identity, `argv` array, approved `cwd`, environment references, stdin artifact, expected exit codes, timeout, stdout/stderr caps, declared output paths and optional pinned structured-output parser. Shell syntax requires an explicit shell executable/script artifact whose bytes are hashed.

Inputs: declared repository/checkpoint/context/artifact references. Outputs: `result: command-result/v1`, stdout/stderr artifact references and optional typed output ports. The result records command digest, environment-reference digest, sandbox identity, start/end, exit/signal/timeout and collection status.

## Functionality

Provision/reuse only the explicitly scoped sandbox and launch through the existing runtime process abstraction. Capture output with byte/time bounds and collect only declared paths. Exit criteria and output-contract validation determine node success. A Command block does not automatically satisfy a Test gate simply because it runs a command named “test.”

Commands with side effects declare retry/reconciliation behavior. Read-only deterministic transformations may retry in a new sandbox; database writes or remote calls are not automatically repeatable. Independent commands never share mutable instance state by default.

## Permissions, errors and limits

No implicit network, credentials, DinD or host sockets. Validate cwd and output paths after canonicalization. Terminate descendants on timeout/cancel. Output limit violations remain visible; declared acceptance artifacts cannot be silently truncated. Missing executable, nonzero exit, signal, timeout and collection failure remain distinct.

## Build instructions and acceptance

Reuse C04 command execution and C11 collection. Use argv vectors instead of concatenated shell strings. Add test fixtures for argument quoting, hostile filenames, child processes, output overflow and restart during a command. Test IDs: `SBX-03`, `ART-02`, `ART-05`, `DB-01`.

## Implementation ownership

See [C04 — Runtime provisioning and sandbox enforcement](../components/04-runtime-sandbox.md). Shared behavior is implemented once in that component and exposed through this block; do not create a separate executor stack.
