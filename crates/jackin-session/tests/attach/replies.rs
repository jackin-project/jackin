// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_testkit::{Rig, TestResult, guard};

use super::support::{delivered, events, owner, replied, set_state};

const CPR: &[u8] = b"\x1b[12;40R";
const DA1: &[u8] = b"\x1b[?62;4c";
const DA2: &[u8] = b"\x1b[>0;276;0c";
const OSC10_BEL: &[u8] = b"\x1b]10;rgb:ffff/ffff/ffff\x07";
const OSC11_ST: &[u8] = b"\x1b]11;rgb:0000/0000/0000\x1b\\";
const OSC11_BEL: &[u8] = b"\x1b]11;rgb:0000/0000/0000\x07";
const OSC10_ST: &[u8] = b"\x1b]10;rgb:ffff/ffff/ffff\x1b\\";
const ARROWS: &[u8] = b"\x1b[A\x1b[1;5C\x1b[B";
const DECRQM: &[u8] = b"\x1b[?2004;2$y";
const KITTY: &[u8] = b"\x1b[?0u";

fn every_reply() -> [&'static [u8]; 9] {
    [
        CPR, DA1, DA2, OSC10_BEL, OSC11_ST, OSC11_BEL, OSC10_ST, DECRQM, KITTY,
    ]
}

fn replies() -> [&'static [u8]; 7] {
    [CPR, DA1, DA2, OSC10_BEL, OSC11_ST, OSC11_BEL, OSC10_ST]
}

fn framed(reply: &[u8]) -> Vec<u8> {
    [b"hi".as_slice(), reply, b"yo".as_slice()].concat()
}

#[tokio::test]
async fn terminal_replies_reach_the_agent_only_while_it_runs_and_typed_keys_always_pass()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let mut terminal = owner(&rig, 1).await?;
        events(&mut terminal, 2).await?;

        for reply in replies() {
            assert_eq!(
                delivered(&handle, &mut terminal, &framed(reply)).await?,
                framed(reply)
            );
        }
        assert_eq!(delivered(&handle, &mut terminal, ARROWS).await?, ARROWS);

        handle.exit_codex(3);
        for reply in replies() {
            assert_eq!(
                delivered(&handle, &mut terminal, &framed(reply)).await?,
                b"hiyo"
            );
        }
        assert_eq!(delivered(&handle, &mut terminal, ARROWS).await?, ARROWS);

        for text in ["waiting\n", "garbage words\n"] {
            set_state(&rig, &name, text).await?;
            assert_eq!(
                delivered(&handle, &mut terminal, &framed(CPR)).await?,
                b"hiyo",
                "{text:?}"
            );
            assert_eq!(delivered(&handle, &mut terminal, ARROWS).await?, ARROWS);
        }
        std::fs::remove_file(handle.root().join(".jackin/run/state"))?;
        assert_eq!(
            delivered(&handle, &mut terminal, &framed(DA1)).await?,
            b"hiyo"
        );
        set_state(&rig, &name, "running 1\n").await?;
        assert_eq!(
            delivered(&handle, &mut terminal, &framed(DA1)).await?,
            framed(DA1)
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn reply_passes_only_while_running() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let mut terminal = owner(&rig, 1).await?;
        events(&mut terminal, 2).await?;
        for reply in every_reply() {
            assert_eq!(replied(&handle, &mut terminal, reply).await?, reply);
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn reply_drops_in_every_other_state() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let mut terminal = owner(&rig, 1).await?;
        events(&mut terminal, 2).await?;

        handle.exit_codex(3);
        for reply in every_reply() {
            assert_eq!(replied(&handle, &mut terminal, reply).await?, b"");
        }
        for text in ["waiting\n", "garbage words\n"] {
            set_state(&rig, &name, text).await?;
            for reply in every_reply() {
                assert_eq!(
                    replied(&handle, &mut terminal, reply).await?,
                    b"",
                    "{text:?}"
                );
            }
        }
        std::fs::remove_file(handle.root().join(".jackin/run/state"))?;
        for reply in every_reply() {
            assert_eq!(replied(&handle, &mut terminal, reply).await?, b"");
        }
        set_state(&rig, &name, "running 1\n").await?;
        assert_eq!(replied(&handle, &mut terminal, DECRQM).await?, DECRQM);
        Ok(())
    })
    .await
}
