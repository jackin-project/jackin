# T08 Kimi Code with Kimi: blocked on the vendor weekly quota

State: unavailable.
Reason: the Kimi weekly (7-day) usage window is exhausted, so the remaining inference proof cannot run.
Update 2026-11-11 (Asia/Saigon): `rewrite/evidence/A14-t08-live.md` and `rewrite/evidence/A14-t08-supplementary.md` record the current state. Native `--from-host-login` import, generic `OAuth2RefreshToken` refresh, supervised launch, trust flow, session creation, attach, detach, inspect, and purge pass on `jackin-docker`, and the host login remains usable. The supervised interactive task and a direct host `kimi -p` call both returned the weekly 7-day usage limit, so supervised interactive inference to the exact marker, secondary-home execution, and vendor-side usage attribution are still unproven. A host probe on 2026-11-11 returned the same weekly-limit error.
The mainland China lane was not exercised because `/Users/donbeave/.kimi-code/credentials/kimi-code.json` does not exist. No membership API key is a permitted fallback.
To unblock: wait for the Kimi weekly window to reset, then rerun the supplementary live gate (interactive inference to the exact marker, secondary-home Kimi execution, vendor usage attribution with no overage) and land the T08 promotion only on a full pass.
