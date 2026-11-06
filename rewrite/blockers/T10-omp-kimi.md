# T10 omp with Kimi: blocked on this host

State: unavailable.
Reason: no Kimi Code membership key on this host.
The host Kimi login is OAuth. Its api_key field is empty.
The host omp store holds only a Z.AI record.
To unblock: add a Kimi Code membership key with `jackin account add kimi --name omp-kimi --credential omp-host` after the key is in the host omp store, or use another credential source. Then run the A4 live gate.
Evidence: rewrite/evidence/A4-omp-kimi.md.
