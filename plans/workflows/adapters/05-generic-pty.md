# A05 — Approved generic CLI / PTY adapter

**Status:** Proposed Jackin workflow adapter contract. Upstream documentation is not a passed Jackin runtime certification. Shared purpose/API semantics are defined by [C05](../components/05-agent-adapters.md), [C06](../components/06-capsule-session-control.md) and [C07](../components/07-input-leases-attachment.md).

## Purpose and current boundary

Allow arbitrary **compatible, host-approved CLI applications** without pretending every terminal program supports autonomous turn detection, steering or resume. Capsule remains the only PTY owner. The existing built-in `AgentRuntime` trait is sealed, so generic execution requires an explicit additive descriptor/launch path, not an external implementation of that trait. [J17](#evidence-and-references).

## API and descriptor

```text
RegisteredCli {
  id, approved_binary_digest, target_os_arch,
  argv_template, allowed_role_ids, environment_contract,
  working_directory_policy, context_input_methods,
  input_encoding, submit_sequence, interrupt_sequence?,
  capability_profile, certificate_id
}
ExecutionProgram = BuiltIn(Agent) | RegisteredCli(id, expected_digest)
```

Add this domain choice at the validated runtime/Capsule launch boundary while preserving existing built-in selection. A proposed `programmatic_program` entry point delegates built-ins to the existing programmatic launch path and validates registered descriptors explicitly. Do not launch an unrestricted interactive shell with arbitrary Pkl-supplied commands as a backdoor generic adapter. Roles must declare or be host-authorized for the registered program; generic support does not bypass role trust.

## Required functionality

The minimal transport can start a CLI, maintain an identifiable PTY, send approved literal input, observe output, interrupt/stop the process group and attach to the actual terminal. Every stronger capability—turn readiness, semantic completion, prompt acknowledgement, same-session follow-up, structured output, vendor conversation resume—requires explicit per-application evidence.

A finite terminal-recognition profile may synchronize input using certified markers or a bounded visible-state rule. It is not proof that a task succeeded. If the application cannot safely identify an input boundary, autonomous feedback is unsupported; a plan may instead require operator-controlled input. Generic compatibility is not permission to label blind keystroke injection as robust orchestration.

Use immutable executable and argv templates with typed parameter substitution into individual arguments. Restrict environment, cwd and attachments to the execution grant. No runtime plugin download, arbitrary dynamic library loading or agent-edited registry entry is permitted. Descriptor changes produce new certificate/plan requirements.

## Failure and recovery

EOF, process exit, timeout and uncertain input are distinct. PTY session recovery after process death is unsupported unless a certified application-specific restore command is declared. Never infer a resume command from an agent brand or fabricate a conversation ID. Operator inspection remains available for live processes even when autonomy capabilities are limited.

## Build instructions

Add the approved program descriptor and backward-compatible launch variant; map it into the existing Capsule spawn machinery without creating another terminal manager. Add an operator registration/validation surface through existing configuration conventions. Start with a deterministic fake CLI that models long input, partial writes, busy turns and crashes, then certify real applications individually.

## Acceptance tests

`GENERIC-01`: unregistered/binary-changed executable is denied. `GENERIC-02`: typed argv cannot inject host shell commands. `GENERIC-03`: unsupported semantic capabilities fail planning. `GENERIC-04`: original PTY remains inspectable. `GENERIC-05`: no invented resume and no blind resend after crash. `GENERIC-06`: existing built-in agent/role selection remains unchanged.

## Evidence and references

- **J06 — [Programmatic launch options](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/services/jackin-runtime-launch-load-options/src/load_options.rs).** Lines 1–300. Non-interactive launch, programmatic constructor, identity sink, inputs and validation error vocabulary. Deeper launch call paths were not executed.
- **J09 — [Capsule binary attach protocol](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/core/jackin-protocol/src/attach.rs).** Lines 1–240. Attach framing, raw input/output, resize, host-action tags, limits and spawn request. Full input routing implementation not audited.
- **J17 — [Existing sealed agent runtime trait](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/core/jackin-core/src/agent/runtime.rs).** Lines 1–260, including the complete trait. Installation, auth and state behavior, not workflow-turn control. Built-in adapter count in comments is stale; no count inferred from it.
