# T08 Kimi Code with Kimi: blocked on this host

State: unavailable.
Reason: no Kimi Code membership key on this host.
The host Kimi login is OAuth. Its api_key field is empty.
To unblock: put a membership key in a credential source, run `jackin account add kimi --name <name> --agent kimi-code --credential <source>`, then run the A5 live gate in rewrite/GOAL-MULTI-AGENT.md.
The hermetic path is built and tested: provider jackin-<account>-kimi, profile t08, placeholder config with api_key_env.
