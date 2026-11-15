# T07 MiMo Code with MiMo: blocked-vendor

State: unavailable.
Reason: native login has no refresh grant.
Current evidence, 2026-11-10 (Asia/Saigon): the pinned MiMo Code 0.1.16 native browser login returns an `sk` credential and stores it as `type: "api"`. It produces no OAuth access/refresh/expiry bundle, refresh URL, client ID, scope, or refresh grant. Relevant vendor sources are `packages/cli/src/plugin/mimo.ts`, `provider/auth.ts`, and `auth/index.ts` at tag v0.1.16.
The latest operator goal requires the native MiMo Code subscription login only. The browser-issued `sk` must not be relabeled as an OAuth access token, and a Singapore Token Plan API key is not a permitted fallback. Earlier Token Plan evidence for omp, Claude Code, and Codex does not establish native MiMo Code subscription support.
The current static-key provider and placeholder path must be removed. The native record must fail with a typed `NativeRefreshUnsupported` before any gateway write. No further live test can satisfy the class-O contract with this vendor artifact.
To unblock: Xiaomi supplies a refresh-capable native subscription grant and documents its refresh contract, or the operator changes the authentication contract. Then implement that exact path and run the complete A5/D29 live gate on jackin-docker and jackin-local.
