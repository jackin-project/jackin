# C21 — Visual workflow authoring and Pkl preservation

**Status:** Proposed implementation contract. APIs below are targets, not existing Jackin APIs.

## 1. Purpose and ownership

Provide genuine creation and configuration of reusable workflow blocks, typed connections, attachments, profiles and gates while keeping Pkl authoritative. A graph viewer alone does not complete this component or acceptance criterion 13.

## 2. Existing foundation and gap

Use C18’s host service and C02’s source mappings. React Flow or an equivalent library may render/edit the graph; it is not the scheduler. Evaluated JSON loses source structure, so arbitrary Pkl cannot be safely regenerated from it. The preservation strategy below is an explicit product contract, not an assumed parser feature. [E13](#evidence-and-references).

## 3. Authoring API

```text
get_authoring_model(source_bundle) -> AuthoringModel
preview_edit(source_hash, StructuredPatch) -> SourceDiffAndDiagnostics
apply_edit(source_hash, patch_digest, expected_revision) -> NewSourceBundle
validate_and_plan(new_source_bundle, parameters) -> PlanEnvelope
```

`AuthoringModel` maps graph fields to source spans, import/template origins, editability, effective values and supported patch operations. `StructuredPatch` includes add/remove node, set editable literal field, connect/disconnect typed port, attach immutable resource, choose profile/template, and configure acceptance/repair limits. The host validates patches; the browser never writes arbitrary host paths.

## 4. Source-preservation contract

Use two supported modes. **Managed module mode:** the editor creates and owns a clearly designated Pkl module with stable IDs and literal/template-reference structures. Hand-authored top-level Pkl can import/amend it; the editor never rewrites unrelated files. **Source-aware mode:** a Pkl parser/CST and compiler source map enable surgical edits to a certified subset of literal fields and node declarations in hand-authored modules.

An unsupported expression, custom inheritance pattern, generated collection or unknown construct is read-only in the graph with a source view explaining why. It is not erased or replaced with its evaluated value. Supported imports, comments, formatting and inheritance outside the edited span remain byte-for-byte unchanged. If a user hand-edits an editor-owned file beyond the supported subset, detect that state and require safe source-aware handling rather than treating it as disposable generated output.

Before apply, show the exact source diff and diagnostics. Compare-and-set the original source hash; another editor or manual change causes conflict. Validate the new operational source before atomic save and plan creation. Invalid work may be saved only as an explicitly separate draft that cannot run. Editing source never mutates an active run’s plan.

## 5. Required functionality

Users can create every block kind from the versioned schema catalog, choose an existing role and certified agent, attach read-only task documents, connect compatible output/input ports, configure parallel/join and bounded repair, define profiles and require approval. The UI shows effective permissions versus requested values and explains host denials without offering an unsafe override.

Reusable block templates reference immutable versioned Pkl modules. Creating a template does not automatically trust its role, imports or executable. Connections that would create an ordinary cycle are rejected with a clear path. Missing required outputs and bypassed gates are diagnostics before run. An expanded preview shows how repair scopes lower into finite execution iterations.

Runtime view reuses Console/service semantics: state, candidate revision, findings, artifacts and live original-session Observe/Take control. Authoring state and running state have separate panels/versions. Dragging a runtime node changes only layout metadata unless the operator explicitly edits a future workflow definition.

## 6. Security and error handling

Use the authenticated local bridge described in C18, with origin/CSRF protection and bounded payloads. Treat labels, Markdown findings and terminal content as untrusted rendered content. No arbitrary browser code becomes a condition, policy or host command. Refuse edits that would alter unsupported source structures or denied host resources.

## 7. Build instructions

Build the host authoring-model/patch API and preservation fixtures before a drag-and-drop UI. Choose a maintained Pkl parser/CST or use pklr source spans only after proving they preserve the required structures; do not hand-write a line-based parser. Ship managed-module mode first, then the certified source-aware subset. Add component forms from the schema catalog and typed-edge validation. Complete runtime inspection and safe original-session attachment using the shared service.

## 8. Dependencies and integration

C01 schema/templates, C02 compiler/source map, C03 policy diagnostics, C18 API, C07 attachment, C22 artifacts. Layout metadata is separate non-authoritative data. No requirement to reimplement existing Jackin Console in React.

## 9. Acceptance tests

`EDIT-01`: create/save/plan a workflow using all block categories. `EDIT-02`: valid typed connections succeed; invalid/cyclic ones fail. `EDIT-03`: supported hand-authored imports/comments/inheritance survive edits. `EDIT-04`: unsupported constructs stay intact/read-only. `EDIT-05`: concurrent source changes fail CAS. `EDIT-06`: active run plan stays unchanged. `EDIT-07`: roles/profiles/attachments/gates can be configured visually. `EDIT-08`: viewer-only delivery is not marked final authoring completion.

## Evidence and references

- **E13 — [React Flow computing flows](https://reactflow.dev/learn/advanced-use/computing-flows).** Graph interaction example, not an execution authority. UI library choice is replaceable. Accessed 2026-10-14.
- **J18 — [Engineering rules](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/ENGINEERING.md).** Full document. Rust-first, reuse, turso general-purpose DB, governed jackin-telemetry and privacy rules.
