# T04 Muse Code with Meta: blocked

State: unavailable.
Reason: Muse 1.4.14 fails in the sandbox before any provider work: `Agent Definition filesystem source failed: IoError`.

Four live proof rounds are in rewrite/evidence/A6-muse.md. The gateway path works: `jackin account add muse --from-host-login` creates provider jackin-muse-live-muse, the sandbox holds only a placeholder, and the host CLI keeps working at the same time. The sandbox run never starts. The error occurs with every provider mode (meta, echo, local), every cwd, a clean HOME, and all documented flags. The failing filesystem path cannot be seen from inside the sandbox: ptrace, seccomp tracing, and user namespaces are all denied there, and failed opens give no inotify event.

One jackin gap from the same proof is fixed separately: jackin now writes the Muse trust store at launch (see the A6 trust change). That change is necessary but not sufficient.

To unblock: run the Muse 1.4.14 Linux binary under strace in an environment that permits ptrace and matches the role image, record the failing openat path, create that path in the role image or launch setup, and re-run the A6 live gate. Then flip T04 to supported.
