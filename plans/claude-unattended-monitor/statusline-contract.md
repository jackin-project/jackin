# Claude Code statusline contract

## Source and supported data

Claude Code runs the configured `statusLine.command` as a local shell command,
passes its statusline JSON on stdin, and displays its stdout. The command runs
on session start/resume and again after assistant messages, `/compact`,
permission-mode changes, Vim-mode toggles, command changes, and `refreshInterval`
timers. Claude Code also reruns it when a rate-limit reset or prompt-cache
expiration in the last payload arrives. Updates debounce by 300 ms; a newly
triggered update cancels the in-flight command. The optional `refreshInterval`
repeats the callback but does not fetch new provider usage data.

The official [statusline guide](https://code.claude.com/docs/en/statusline)
documents these usage fields:

| Field | Meaning |
| --- | --- |
| `session_id` | Unique session identifier |
| `rate_limits.five_hour.used_percentage` | 5-hour window utilization, 0–100 |
| `rate_limits.five_hour.resets_at` | 5-hour reset time, Unix epoch seconds |
| `rate_limits.seven_day.used_percentage` | 7-day window utilization, 0–100 |
| `rate_limits.seven_day.resets_at` | 7-day reset time, Unix epoch seconds |

Claude Code added `rate_limits` in [v2.1.80](https://raw.githubusercontent.com/anthropics/claude-code/v2.1.80/CHANGELOG.md#L2-L2).
The current guide says `rate_limits` is available for Claude.ai Pro/Max
subscribers or through a Claude apps gateway with spend limits, after the
session's first API response. Each window can be absent independently, and
Claude Code removes a window after its reset time. Treat absent or malformed
data as unknown; do not convert it to 0%. `used_percentage` is utilization,
not remaining quota, token count, or spend.

`session_id` identifies a session, not an account. The documented statusline
payload has no account ID or email, so the selected Jackin account must be
provided explicitly to the ingestion command. A callback is not an exactly-once
event: Claude Code can invoke it repeatedly with unchanged input, and can cancel
an in-flight run when another update triggers. Persist these as idempotent
observations, not as per-turn usage deltas. The callback is event driven and can
go quiet while a session is idle when no `refreshInterval` is set. A refresh
timer reruns the command with the last statusline payload, but does not refresh
provider data. This is a best-effort source for local observation, not an
unattended provider refresh scheduler.

## Composition behavior

`cli::usage::statusline::compose` reads a settings file up to 1 MiB and returns
proposed settings JSON. A missing file is treated as an empty object for initial
setup. It does not write or create the settings file, modify account state, or
start Claude Code. If `statusLine` is present, it must be a command object with a
string `command`; composition changes only that command and preserves other
settings and statusline options. If `statusLine` is absent, it proposes a
command that consumes input and renders no output before ingesting. Malformed
settings, settings over 1 MiB, and non-command status lines are rejected. An
existing command over 16 KiB of UTF-8 bytes or a composed shell command over
64 KiB is also rejected, so large or quote-heavy settings are never returned as
an unlaunchable proposal.

The wrapper uses Python 3's standard library in isolated mode to read Claude's
stdin once, stream it to the existing command, and retain at most 16 KiB plus
one byte in memory.
The existing command is run once and owns the displayed stdout and exit status.
If it exits unsuccessfully, ingestion is skipped. Payloads over 16 KiB still
flow through the existing command but skip ingestion. If Python 3 is
unavailable, the wrapper runs the existing command directly once and skips
ingestion. Accepted payloads go to the explicit local command:

```text
jackin usage statusline ingest --account <id> --format json --data-dir <path>
```

The account ID, binary path, data directory, and original command are passed as
separately shell-quoted arguments; they are not interpolated into the wrapper
script. Ingestion stdout and stderr are suppressed so CLI output cannot replace
or contaminate the existing statusline. Ingestion has a two-second timeout; a
failed or timed-out ingest does not change the existing command's output or
successful exit status. The data path is local and must stay fast because Claude
Code waits for the command to exit before displaying its output. This
composition performs no OAuth lookup, Keychain access, or provider HTTP
request.

The wrapper requires a POSIX-compatible shell and Python 3 for ingestion; if
Python 3 is unavailable, the existing command still runs without ingestion.
Ingestion requires top-level `session_id`; `rate_limits`, `five_hour`, and
`seven_day` are independently optional. A present window must contain a finite
`used_percentage` from 0 through 100 and a `resets_at` Unix epoch timestamp.

## Official references

- [Customize your status line — Claude Code Docs](https://code.claude.com/docs/en/statusline)
- [Claude Code changelog, v2.1.80](https://raw.githubusercontent.com/anthropics/claude-code/v2.1.80/CHANGELOG.md#L2-L2)
