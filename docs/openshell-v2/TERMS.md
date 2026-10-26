# Technical terms and writing rules

## Writing rules for task text

Use one instruction in each procedural sentence.
Use no more than 20 words in a procedural sentence.
Use active voice and direct commands.
Use one technical term for one meaning.
Put the condition before the action when the order matters.
Do not hide a missing condition in words such as appropriate, normally, or suitable.
Use a numbered procedure when order matters.
Use explicit expected results in tests.

Keep code identifiers, paths, commands, and API names exact.
Do not change an upstream symbol to make its English simpler.
Do not claim full ASD-STE100 conformity from a sentence-length check alone.

## Product nouns

| Term | Meaning |
| --- | --- |
| Account | A stable identity and a reference to one credential source. |
| AccountGrant | Approved account authority for one domain or run. |
| activation barrier | Required checks before the first agent session starts. |
| activation receipt | The stored matching inputs that permit session activation. |
| adapter | Code that connects an application port to one external system. |
| agent | A coding or tool program that performs work in an admitted sandbox. |
| application port | A domain-owned interface implemented by an adapter. |
| approval | A host decision for one fixed, scoped operation. |
| artifact | A versioned build or recovery output with a recorded identity. |
| Attachment | One temporary client connection to a session. |
| AuthorityScopeBinding | The selected OpenShell workspace and its authorized use. |
| baseline | The recorded source state before later changes. |
| binding | A record that connects an application entity to its exact runtime resource. |
| boot ID | A fresh identifier for one Capsule process incarnation. |
| builder | Trusted infrastructure that builds role images. |
| capability | An explicitly admitted workload ability. |
| Capsule | The unprivileged service that owns sessions inside one domain. |
| checkpoint | An independently retained project and approved agent-state recovery artifact. |
| companion | An OpenShell-owned engine VM attached to one admitted domain. |
| compatibility tuple | The complete pinned runtime and infrastructure combination under test. |
| control plane | The service that owns runtime admission and lifecycle. |
| credential revision | The identified version of a credential source. |
| decoder | Code that reads protocol bytes into bounded messages. |
| digest | A cryptographic content identity. |
| domain | One account, policy, writable-data, and trust set in one OpenShell sandbox. |
| effect | An operation that changes external or durable state. |
| engine | The optional Docker daemon and its disposable machine. |
| epoch | A new identity for a staged publication or activation. |
| ExecutionDomain | The typed application record for a domain. |
| fence | A server-side precondition that prevents an effect on a different resource incarnation. |
| fixture | Disposable test input or infrastructure with known ownership. |
| frame | One bounded protocol message with a length prefix. |
| gateway | One registered OpenShell endpoint and its compute driver. |
| generation | The revision that identifies a relevant runtime or authority incarnation. |
| grant | Explicit authorized access with a scope and lifetime. |
| handoff | Creation of a new authorized domain from a verified checkpoint. |
| host control | The trusted per-user Jackin application service. |
| host effect | A desktop, filesystem, network, or secret-consuming action outside the coding sandbox. |
| input lease | Temporary permission for one attachment to send terminal input. |
| Instance | One logical developer session with one or more domains. |
| journal | Durable operation intent, progress, request IDs, and receipts. |
| materialization | One concrete project checkout or snapshot with an identified storage owner. |
| mutation | An effectful runtime API operation. |
| observation | A reported resource or session state with its source and freshness. |
| operation ID | The durable identity of one application intent. |
| outcome unknown | A state where available evidence cannot prove the external result. |
| platform manifest | The OCI manifest for one selected architecture and operating system. |
| projection | The confined project view available to the companion engine. |
| protocol | The defined message format and its state rules. |
| PTY | A pseudo-terminal used by an interactive process. |
| qualification | Tests that establish a specific capability on a specific compatibility tuple. |
| receipt | A recorded completed effect or verified application publication. |
| relay | A short-lived process that connects OpenShell exec to the Capsule socket. |
| replay | Reuse of a recorded result or retained output under its defined identity. |
| ResolvedRole | The immutable role image and all resolved inputs and compatibility data. |
| RoleRecipe | The versioned source definition of an agent environment. |
| sandbox | The OpenShell resource that supplies one execution boundary. |
| SandboxBinding | Gateway, workspace, name, immutable ID, and observed runtime generation. |
| scope | The resource and authority boundary selected for an operation. |
| Session | One agent or shell PTY inside one domain and Capsule boot. |
| snapshot | A point-in-time data or terminal-state representation. |
| source isolation | The relation between the project checkout and its original source. |
| staging | Temporary content that is not yet active. |
| subagent | An actual separately invoked agent with a bounded task and report. |
| TargetProfile | A gateway reference plus qualified product, storage, placement, and resource choices. |
| tombstone | A retained record that prevents intentional reuse of a former canonical name. |
| trust context | The complete admitted account, policy, data, tenant, and capability set. |
| verdict | The host-owned result for a frozen independent check. |
| watermark | The output sequence represented by a terminal snapshot. |
| workspace | A Jackin project in user-facing vocabulary; qualify OpenShell workspace when authority is meant. |
| worktree | A Git working tree with repository metadata and a recorded base. |
| write lease | The coordinator's exclusive assignment of files to one implementation task. |

## Technical verbs

Use these verbs only with their defined software meanings in this pack.

| Verb | Meaning |
| --- | --- |
| admit | Permit a domain or capability after its authority checks pass. |
| attach | Connect a client to an existing session. |
| compile | Convert source code into an executable artifact. |
| decode | Convert bounded protocol bytes into messages. |
| deduplicate | Prevent a repeated identified request from creating another effect. |
| deploy | Install an approved infrastructure artifact into its authorized environment. |
| encode | Convert a message into the defined protocol bytes. |
| execute | Run the specified command or task. |
| export | Produce selected data for an explicit destination. |
| freeze | Record a contract or input identity that later tasks must not change silently. |
| fuzz | Test a parser or state machine with generated bounded input. |
| hash | Calculate a cryptographic content digest. |
| import | Materialize explicitly selected external data. |
| inspect | Read state without changing the resource. |
| materialize | Create a concrete verified project state from its manifest. |
| merge | Integrate a reviewed source change into the designated branch. |
| pin | Record an immutable source or artifact identity. |
| prewarm | Prepare immutable artifacts without account or engine activation. |
| publish | Make a verified staged artifact active or deliver it to an authorized registry. |
| qualify | Complete the required tests for one stated compatibility tuple. |
| reconcile | Restore declared upstream resource ownership after a failure. |
| redact | Remove identified secrets from ordinary output. |
| replay | Repeat only the result or operation permitted by its recorded replay contract. |
| reprovision | Create a new runtime resource rather than modifying an incompatible existing one. |
| revoke | Remove previously granted access. |
| serialize | Convert structured records into their defined durable or wire format. |
| stage | Write inactive content before verification and publication. |
| validate | Check data or a result against explicit rules. |

These terms support consistent software instructions.
They do not replace the ASD-STE100 dictionary or an independent language review.
