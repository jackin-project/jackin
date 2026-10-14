# C07 — Input leases, observation and takeover

**Status:** Proposed implementation contract. APIs below are targets, not existing Jackin APIs.

## 1. Purpose and ownership

Provide Observe, Take control and safe relinquishment against the original Capsule session. Capsule is the terminal and input authority; the host workflow service persists operator decisions and orchestrates pause/resume.

## 2. Existing foundation and gap

The binary attach protocol already carries terminal input/output, resize, focus and host actions. Those capabilities must be separated by observer/controller authority for workflow sessions. Inspected protocol declarations do not establish a workflow-scoped exclusive lease, so implementation must audit and cover every input route rather than adding a UI-only lock. [J09](#evidence-and-references).

## 3. API and data contracts

```text
observe(session, principal, output_cursor?) -> ObserveHandle
request_control(session, principal, expected_generation, reason) -> LeaseOffer
acquire_control(offer, ttl) -> InputLease
renew_control(lease_id, generation) -> InputLease
release_control(lease_id, generation, resume_request) -> ReleaseReceipt
revoke_control(session, authorized_principal, reason) -> Receipt
```

`InputLease` includes session address, owner kind (`scheduler` or `operator`), authenticated principal, lease ID, monotonically increasing generation, expiry and permitted input actions. A secret lease token is distinct from its audit-safe ID. The generation fences stale owners even if messages arrive late.

## 4. Required functionality

Observe subscribes to the actual running session without interrupting the worker, acquiring keyboard ownership or altering the terminal’s authoritative dimensions. Multiple observers are allowed. Observer rendering adapts locally; only the controller can change the session viewport/focus that affects agent behavior. Observers cannot trigger clipboard writes, URL opening, file exports or replay-triggered host actions without separate explicit authorization.

Takeover first persists operator intent and fences further scheduled input. Capsule acknowledges the new generation before issuing the operator lease. Any in-flight input transaction reaches a defined completed/partial/unknown boundary before handover; two writers never interleave. The UI reports an active turn honestly and offers a certified interrupt rather than claiming takeover instantly stopped the model.

Enforce lease checks on raw attach bytes, control RPC, slash-command submission, structured vendor API calls and any workflow automation bridge. Agent/session capability tokens cannot mint an operator lease. If a vendor transport cannot be mediated or independently restricted, that topology cannot be certified for exclusive control.

Release flushes/records operator input boundaries, increments the generation and moves the run into a reconciliation boundary. Capture current vendor turn and working-tree/candidate changes. Reverification is required for unverified manual edits. The scheduler regains input only after host policy, lease, session and budget checks pass. `resume_request` is an explicit operator request, not unconditional permission.

## 5. Expiry, disconnect and emergency control

Disconnect does not silently resume automation. On lease expiry, Capsule rejects further input under that token and records the event; host enters `awaiting_operator` or a configured safe reconciliation policy. Pending keystrokes from the old connection are discarded or explicitly reported as uncertain, not replayed under the next owner.

An authorized emergency revoke must be auditable. It fences input but does not erase operator changes. User-accessible stop/interrupt remains available through a separate authorized cancellation path, including while the scheduler is unavailable. A read-only observer cannot use that path.

## 6. Build instructions

Trace every Capsule input ingress and enumerate it in a test fixture. Add one shared lease validator close to the actual writer/transport action. Implement acquisition as a serialized state transition, then wire host approval/pause events. Extend attach negotiation with observe/control modes and explicit rejection for incompatible old clients. Add UI state showing owner, expiry, active turn, pending input and resume requirements.

## 7. Dependencies and integration

Requires C06 epoch/command records, C10 decisions, C17 reconciliation and C22 audit. C19/C20/C21 call this API, never implement their own lock. Keep native Capsule multiplexer ownership unchanged.

## 8. Acceptance tests

`LEASE-01`: two concurrent operators cannot both input. `LEASE-02`: stale generation and expired token fail. `LEASE-03`: observing does not resize, steal focus or emit host actions. `LEASE-04`: automated input is fenced before takeover acknowledgement. `LEASE-05`: legacy raw input and vendor HTTP cannot bypass the lease. `LEASE-06`: disconnect/release with manual changes requires reconciliation and revalidation.

## Evidence and references

- **J07 — [Shared Capsule contracts](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/core/jackin-protocol/src/lib.rs).** Lines 1–280. Session capability and identity environment, scoped credential contracts, CapsuleConfig and exit preservation choices.
- **J08 — [Capsule control protocol](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/core/jackin-protocol/src/control.rs).** Lines 1–280. SessionSend writes literal bytes; SessionSent acknowledges PTY writer delivery; Events is a subscription. Not an exactly-once prompt protocol.
- **J09 — [Capsule binary attach protocol](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/core/jackin-protocol/src/attach.rs).** Lines 1–240. Attach framing, raw input/output, resize, host-action tags, limits and spawn request. Full input routing implementation not audited.

## Taking control during finalization

When a scope is finalized for acceptance/publication, the UI shows that its writable phase is fenced. Acquiring input that can reopen work first records host-authorized invalidation of finalization and any pending acceptance eligibility. Do not release the writer barrier and only later invalidate an approval. A completed run is immutable; additional work starts a new linked run rather than silently modifying the accepted result.
