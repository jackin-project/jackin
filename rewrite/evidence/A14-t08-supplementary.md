# A14 T08 supplementary live proof attempt

Date: 2026-11-11. Base: integrated tree at fd247eed8 plus the uncommitted T08 promotion. Binary: `/tmp/jackin-a14-integrated/target/debug/jackin` built from that tree. Gateway: `jackin-docker`. Isolated config: `XDG_CONFIG_HOME=/tmp/a14t08s-xdg` with the existing OpenShell gateway registrations symlinked in. Role: `/tmp/a14t08s-sentinel` (sentinel copy with the Kimi instruction mapping added, role name `a14t08s-sentinel`, `jackin role check` passed). Project: `/tmp/a14t08s-project`. Account: `t08native`, added with `account add kimi --name t08native --from-host-login`.

## What passed

- `jackin doctor` on jackin-docker: all checks pass, OpenShell 0.1.7, gateway serving.
- Account creation from the native host login succeeded. `jackin usage --json` lists the account with freshness fresh and outcome unavailable with reason `no_credentials`. This confirms the known native-account usage-reporter limitation; it is not a sandbox failure.
- `jackin load a14t08s-sentinel /tmp/a14t08s-project --agent kimi --provider kimi --account t08native --launch-id a14t08skimd01` completed every step: credentials, upload, supervisor, hooks, start. Sandbox `jackin-a14t08s-c7fu` reached phase ready.
- `jackin inspect jackin-a14t08s-c7fu` reported supervisor `running 1`, provider `jackin-t08native-kimi`, policy loaded true, and `kimi version 2.5.2`.
- `jackin attach jackin-a14t08s-c7fu` opened the supervised interactive Kimi Code terminal. The first-run folder-trust screen for `/sandbox/project` appeared and `Trust and continue` was selected. The Kimi welcome screen showed directory `/sandbox/project`, model `kimi-for-coding`, version 2.5.2. A session `session_534319df-7425-4c76-87b0-89cf831fd083` was created on the first message.
- Detach with Ctrl+D printed `detached from sandbox jackin-a14t08s-c7fu`. The sandbox was then purged cleanly with `jackin purge jackin-a14t08s-c7fu`.

## What blocked inference

The interactive task (a Bash-tool file write with an exact reply marker) was submitted. Kimi answered with:

```text
Error: [provider.api_error] You've reached your weekly (7-day) usage limit. Your quota will reset when the current 7-day window ends.
```

A host-side check immediately after, `kimi -p "Reply with exactly: HOST-KIMI-QUOTA-CHECK"`, failed with the same weekly-limit error. The limit is therefore at the subscription account level, shared by the host login and the gateway-held credential. It is not a sandbox, gateway, or Jackin defect.

## Disposition

The supervised interactive launch path is proven up to the provider call: supervision, trust flow, session creation, exact model config, attach, detach, inspect, and purge all pass on jackin-docker. The interactive task completion, the secondary-home execution, and the vendor-side usage attribution remain unproven only because the Kimi subscription weekly window is exhausted.

T08 stays Unavailable in the catalog until the window resets. On reset, rerun: (1) this interactive task to its exact reply, (2) a multi-instance launch with Kimi as the secondary instance and an attach to that instance, (3) a usage attribution check. Then the promotion unit can land.
