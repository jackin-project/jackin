# Technical terms

Use one term for one meaning in the MVP documents.
Keep code identifiers, command names, and protocol field names exact.

| Term | Meaning in this pack |
|---|---|
| MVP | Minimum viable product. The first useful release defined in `SPEC.md`. |
| Host | The operator's local machine and its trusted Jackin control services. |
| Host daemon | The existing long-lived Jackin host process. |
| Workflow | The fixed sequence that implements, checks, and repairs one task. |
| Plan | Frozen input and configuration data for one workflow definition. |
| Run | One execution of a plan. |
| Stage | The current part of the fixed workflow sequence. |
| Round | One source checkpoint and its test and review results. Round 0 follows the initial turn. |
| Repair turn | A later implementer turn that receives recorded failure findings. |
| Role | An existing reusable Jackin execution environment. |
| Instance | One isolated runtime environment created from a role. |
| Worker | An instance that performs implementation, testing, or review. |
| Agent | A real coding-agent CLI process. |
| Subagent | A worker created by the coding-agent runtime or by the implementation coordinator. |
| Reviewer | A separate agent instance that checks an exact checkpoint. |
| Capsule | The existing Jackin session and terminal owner. |
| PTY | Pseudoterminal. The operating-system terminal channel used by the CLI process. |
| TUI | Terminal user interface. The visible interface of the original CLI. |
| Session | One Capsule-managed CLI process and its PTY. |
| Physical session key | The instance ID, Capsule epoch, and Capsule session ID. |
| Epoch | A generation value that distinguishes a new controller or Capsule process. |
| Turn | Work that the agent performs after one submitted instruction. |
| Input lease | Permission for exactly one owner to send input to a session. |
| Observer | A client with output access and no input permission. |
| Takeover | The operator action that acquires exclusive input permission. |
| Checkpoint | A host-owned immutable source revision and its manifest. |
| Candidate | The current checkpoint that requires verification. |
| Revision | An exact Git commit ID, not a branch name. |
| Tree | The Git source-content object for a checkpoint. |
| Digest | A cryptographic hash of exact bytes. |
| Evidence | A recorded test, review, event, or operator action with a known source. |
| Finding | A structured defect report against a named checkpoint. |
| Backup | A retained source copy. Backup does not imply acceptance. |
| Preservation hold | A durable rule that prevents cleanup of required work. |
| Grant | Access that the host permits for a specific operation. |
| Capability | A function that a backend or adapter can actually provide. |
| Profile | A tested combination of runtime, limits, authentication mode, and network rules. |
| Authentication | The process that proves an account's identity to a provider. |
| Credential | Secret data used for authentication. |
| API | Application programming interface. The typed control boundary between components. |
| DTO | Data transfer object. A typed request, response, or saved record. |
| RPC | Remote procedure call. Here it uses the existing local host transport. |
| CI | Continuous integration. The repository's automated verification system. |
| DAG | Directed acyclic graph. General DAG execution is outside this MVP. |
| Pkl | The typed configuration language selected for workflows. |
| `pklr` | The selected Rust Pkl evaluator. |
| `turso` | The existing Rust database dependency selected for local workflow state. |
| `delivery_unknown` | The host cannot prove whether a prompt reached the agent. |
| `session_lost` | The original physical session is no longer available. |
| `MVP PASS` | All required checks in `CHECKS.md` have valid evidence. |
| `BLOCKED` | A named prerequisite prevents a required operation or check. |

## Writing rules for this pack

Write short sentences. Use active voice. Give one instruction per sentence.
Use complete sentences in procedures and explanations.
Use fixed technical terms when ordinary words would change the meaning.
Do not use a technical term to hide an unclear requirement.
Do not change literal code or vendor command syntax to fit prose rules.

This pack uses ASD-STE100 as its writing target.
The document check includes sentence length and term consistency.
It is not a formal dictionary or compliance certification.
