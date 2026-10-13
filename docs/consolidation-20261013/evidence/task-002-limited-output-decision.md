# TASK-002 limited-output decision

Decision: **REJECT**

This note does not approve branch `fea3ade6830912017f2ad80c0c5a9d1acfbde0b7`, ref `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/cleanup-20261006/jackin-unreferenced-tree-abc5b11ca1b2/tip-fea3ade6830912017f2ad80c0c5a9d1acfbde0b7`, or any source edit.

Policy already bounds process stdout and stderr with a byte cap and fails the capture when a read would pass that cap. `read_limited_output` and `run_limited_output` repeat that failure-on-overflow behavior under different names. They do not add a cap or overflow failure that main and policy lack.

## Commit

| Item | Value |
| --- | --- |
| Commit | `fea3ade6830912017f2ad80c0c5a9d1acfbde0b7` |
| Subject | `WIP: preserve unresolved Jackin snapshot` |
| Parents | one: `0aa821a088e1bacf3d4d85a4c9faaa67faa85132` |
| Type | commit, not a stash commit |
| Ancestor of main `1c3dd1be4a3b1ed518133ec3899c084c792370ff` | no |
| Ancestor of policy `fba69d60c2373156dc0dcecee8e085c83776966f` | no |
| File | `crates/jackin-process/src/lib.rs` |

The two helpers sit on the incoming side of an unresolved conflict. The ours side (`0aa821a088e1bacf3d4d85a4c9faaa67faa85132`) between the markers at lines 650–651 is empty. The incoming marker names `2990df17e25f30afca84804d9c402abc1ce00231`, and that object is absent from this clone. A conflict end marker is inside `read_limited_output`, so the stored function text does not close cleanly.

## Snapshot function bodies

Source: `fea3ade6830912017f2ad80c0c5a9d1acfbde0b7:crates/jackin-process/src/lib.rs`.

`run_limited_output` at lines 711–837:

```rust
async fn run_limited_output(
    mut child: tokio::process::Child,
    request: &ExecRequest,
    started: Instant,
    stdin_task: Option<tokio::task::JoinHandle<Result<()>>>,
) -> Result<ExecResult> {
    let Some(stdout) = child.stdout.take() else {
        let cleanup = kill_and_reap(&mut child).await;
        if let Some(task) = &stdin_task {
            task.abort();
        }
        cleanup.context("cleaning up child without limited stdout pipe")?;
        bail!("limited output capture has no stdout pipe");
    };
    let Some(stderr) = child.stderr.take() else {
        let cleanup = kill_and_reap(&mut child).await;
        if let Some(task) = &stdin_task {
            task.abort();
        }
        cleanup.context("cleaning up child without limited stderr pipe")?;
        bail!("limited output capture has no stderr pipe");
    };
    let (issue_tx, mut issue_rx) = tokio::sync::mpsc::unbounded_channel();
    let stdout_limit = request
        .stdout_limit
        .context("bounded capture has no stdout byte limit")?;
    let stderr_limit = request
        .stderr_limit
        .context("bounded capture has no stderr byte limit")?;
    let stdout_task = tokio::spawn(read_limited_output(stdout, stdout_limit, issue_tx.clone()));
    let stderr_task = tokio::spawn(read_limited_output(stderr, stderr_limit, issue_tx));
    let stdout_abort = stdout_task.abort_handle();
    let stderr_abort = stderr_task.abort_handle();
    let stdin_abort = stdin_task
        .as_ref()
        .map(tokio::task::JoinHandle::abort_handle);

    let selected = {
        let complete = async {
            let status = child
                .wait()
                .await
                .context("waiting on limited-output child")?;
            let stdout = stdout_task.await.context("joining stdout reader")??;
            let stderr = stderr_task.await.context("joining stderr reader")??;
            if let Some(task) = stdin_task {
                task.await.context("joining stdin writer")??;
            }
            Ok::<_, anyhow::Error>((status, stdout, stderr))
        };
        tokio::pin!(complete);
        if let Some(timeout) = request.timeout {
            let remaining = timeout.saturating_sub(started.elapsed());
            tokio::select! {
                result = &mut complete => LimitedOutputResult::Complete(result),
                Some(issue) = issue_rx.recv() => LimitedOutputResult::OutputIssue(issue),
                () = tokio::time::sleep(remaining) => LimitedOutputResult::TimedOut,
            }
        } else {
            tokio::select! {
                result = &mut complete => LimitedOutputResult::Complete(result),
                Some(issue) = issue_rx.recv() => LimitedOutputResult::OutputIssue(issue),
            }
        }
    };

    match selected {
        LimitedOutputResult::Complete(result) => {
            let (status, stdout, stderr) = match result {
                Ok(result) => result,
                Err(error) => {
                    stdout_abort.abort();
                    stderr_abort.abort();
                    if let Some(abort) = stdin_abort {
                        abort.abort();
                    }
                    return match kill_and_reap(&mut child).await {
                        Ok(()) => Err(error),
                        Err(cleanup) => Err(anyhow::anyhow!(
                            "{error:#}; cleaning up child after output failure also failed: {cleanup:#}"
                        )),
                    };
                }
            };
            Ok(ExecResult {
                code: status.code(),
                success: status.success(),
                stdout,
                stderr,
                duration: started.elapsed(),
                timed_out: false,
            })
        }
        LimitedOutputResult::OutputIssue(issue) => {
            let cleanup = kill_and_reap(&mut child).await;
            stdout_abort.abort();
            stderr_abort.abort();
            if let Some(abort) = stdin_abort {
                abort.abort();
            }
            cleanup.context("cleaning up child after limited-output failure")?;
            match issue {
                LimitedOutputIssue::ExceededLimit => {
                    bail!("subprocess output exceeded its configured byte limit")
                }
                LimitedOutputIssue::ReadError(message) => bail!("{message}"),
            }
        }
        LimitedOutputResult::TimedOut => {
            let cleanup = kill_and_reap(&mut child).await;
            stdout_abort.abort();
            stderr_abort.abort();
            if let Some(abort) = stdin_abort {
                abort.abort();
            }
            cleanup.context("cleaning up timed-out child")?;
            Ok(ExecResult {
                code: None,
                success: false,
                stdout: Vec::new(),
                stderr: Vec::new(),
                duration: started.elapsed(),
                timed_out: true,
            })
        }
    }
}
```

`read_limited_output` at lines 854–885, including the conflict marker stored in the blob:

```rust
async fn read_limited_output<R>(
    mut reader: R,
    limit: usize,
    issue_tx: tokio::sync::mpsc::UnboundedSender<LimitedOutputIssue>,
) -> Result<Vec<u8>>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut output = Vec::with_capacity(limit.min(64 * 1024));
    let mut buffer = [0_u8; 8192];
    loop {
        let remaining = limit.saturating_sub(output.len());
        let read_limit = remaining.saturating_add(1).min(buffer.len());
        let count = match reader.read(&mut buffer[..read_limit]).await {
            Ok(count) => count,
            Err(error) => {
                let message = format!("reading subprocess output: {error}");
                let _send_result = issue_tx.send(LimitedOutputIssue::ReadError(message.clone()));
                bail!("{message}");
            }
        };
        if count == 0 {
            return Ok(output);
        }
        if count > remaining {
            let _send_result = issue_tx.send(LimitedOutputIssue::ExceededLimit);
            bail!("subprocess output exceeded {limit} bytes");
        }
        output.extend_from_slice(&buffer[..count]);
>>>>>>> 2990df17e25f30afca84804d9c402abc1ce00231
    }
}
```

Overflow behavior in that text: read at most `remaining + 1` bytes, and when `count > remaining` send `LimitedOutputIssue::ExceededLimit` and bail. `run_limited_output` then kills and reaps the child and bails with `subprocess output exceeded its configured byte limit`. The captured prefix is not returned.

The call site, still inside the same incoming conflict side (`fea3ade` lines 451–459 and 557–559), enters this path only when `stdout_limit` or `stderr_limit` is `Some`, and only when both limits are set and both streams are `StdioMode::Capture`. The other incoming path uses `read_output`, which calls `read_to_end` with no byte cap (`fea3ade` lines 676–686).

## Search on main and policy

`git grep` for `fn read_limited_output` and `fn run_limited_output` on `1c3dd1be4a3b1ed518133ec3899c084c792370ff` and `fba69d60c2373156dc0dcecee8e085c83776966f` returned no matches.

The equivalent process helper is `read_captured`. Main and policy copies differ only by path. `git diff --stat` between those commits for this file is a rename with 0 line changes:

- main: `1c3dd1be4a3b1ed518133ec3899c084c792370ff:crates/jackin-process/src/lib.rs`
- policy: `fba69d60c2373156dc0dcecee8e085c83776966f:crates/core/jackin-process/src/lib.rs`

Policy `read_captured` at lines 381–405:

```rust
async fn read_captured(
    stream: Option<impl tokio::io::AsyncRead + Unpin>,
    limit: usize,
    name: &str,
) -> Result<Vec<u8>> {
    use tokio::io::AsyncReadExt;
    let Some(mut stream) = stream else {
        return Ok(Vec::new());
    };
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        let count = stream
            .read(&mut buffer)
            .await
            .with_context(|| format!("reading child {name}"))?;
        if count == 0 {
            return Ok(bytes);
        }
        if count > limit.saturating_sub(bytes.len()) {
            bail!("captured {name} exceeded byte limit {limit}");
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
}
```

Policy applies that cap on every `exec_async` / `exec_sync` capture. `exec_sync` (`crates/core/jackin-process/src/lib.rs` lines 274–295) blocks on `exec_async`. `run_once_async` (lines 453–462) calls:

```rust
read_captured(
    stdout,
    request.stdout_limit.unwrap_or(DEFAULT_CAPTURE_LIMIT),
    "stdout",
),
read_captured(
    stderr,
    request.stderr_limit.unwrap_or(DEFAULT_CAPTURE_LIMIT),
    "stderr",
),
```

`DEFAULT_CAPTURE_LIMIT` is `16 * 1024 * 1024` (line 17). `None` therefore still fails at 16 MiB. An explicit `output_limits` value replaces that default (lines 177–179). On reader error, `run_once_async` kills and reaps the child (lines 484–486) and returns the error. Partial overflow bytes are not returned.

Policy tests in `crates/core/jackin-process/src/tests/case_01.rs`:

- lines 261–270: overflow with `output_limits(4, 4)` must contain `{name} exceeded byte limit 4` and the child must be reaped before return
- lines 276–294: `default_capture_budget_is_finite` expects `stdout exceeded byte limit 16777216`

Main has the same assertions in `crates/jackin-process/src/tests.rs` at lines 274 and 296.

A second name match, `cap_output`, is a later string truncation, not the process reader:

- main: `1c3dd1be4a3b1ed518133ec3899c084c792370ff:crates/jackin-capsule/src/exec.rs` lines 276–278, `MAX_OUTPUT = 1024 * 1024`
- policy: `fba69d60c2373156dc0dcecee8e085c83776966f:crates/apps/jackin-capsule/src/exec/command.rs` lines 173–198

`cap_output` truncates an already captured `String` to 1 MiB and appends `[output truncated at 1 MiB]`. The process-capture bound that matches the snapshot helpers is `read_captured`: a byte cap plus `bail` when the next read would pass it.

## Why REJECT

`read_limited_output` fails when the next read would pass `limit`. `run_limited_output` turns that into a capture error and kills the child. Policy `read_captured` already does that for stdout and stderr, using `output_limits` when set and `DEFAULT_CAPTURE_LIMIT` (16 MiB) when unset, and `run_once_async` already reaps the child on that error. Main has the same helper at the pre-move path. The snapshot names are absent; the bound is present.
