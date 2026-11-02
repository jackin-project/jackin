// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{ProcessRunner, ProcessStatus, ProcessStdin, SecretBytes};
use jackin_session::TokioProcessRunner;
use tokio_util::sync::CancellationToken;

use crate::support::{Scratch, TestResult, guarded, same_bytes};

#[tokio::test]
async fn stdin_is_dev_null_or_the_given_bytes_followed_by_end_of_input() -> TestResult {
    guarded(async {
        let scratch = Scratch::new()?;
        let cancel = CancellationToken::new();
        let null = scratch.spec(
            "[ /dev/stdin -ef /dev/null ] && echo devnull || echo other\ncat\n",
            &[],
        )?;
        let output = TokioProcessRunner.run(&null, None, &cancel).await?;
        assert_eq!(output.stdout, b"devnull\n", "Null stdin");

        let input: Vec<u8> = (0..1_usize << 20)
            .map(|index| (index % 256) as u8)
            .collect();
        let mut echo = scratch.spec("cat\n", &[])?;
        echo.stdin = ProcessStdin::Bytes(SecretBytes::new(input.clone()));
        let output = TokioProcessRunner.run(&echo, None, &cancel).await?;
        assert_eq!(output.status, ProcessStatus::Exited(0));
        same_bytes("echoed 1 MiB", &output.stdout, &input)?;

        let mut empty = scratch.spec("cat\n", &[])?;
        empty.stdin = ProcessStdin::Bytes(SecretBytes::new(Vec::new()));
        let output = TokioProcessRunner.run(&empty, None, &cancel).await?;
        assert_eq!(output.status, ProcessStatus::Exited(0));
        assert!(output.stdout.is_empty(), "empty input gave output");

        let mut unread = scratch.spec("exit 3\n", &[])?;
        unread.stdin = ProcessStdin::Bytes(SecretBytes::new(input));
        let output = TokioProcessRunner.run(&unread, None, &cancel).await?;
        assert_eq!(output.status, ProcessStatus::Exited(3), "unread stdin");
        Ok(())
    })
    .await
}
