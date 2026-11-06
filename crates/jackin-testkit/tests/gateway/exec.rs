// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{EnvName, FileMode, SecretBytes};
use jackin_openshell::{Gateway, ProgramStdin, SandboxFile};
use jackin_testkit::ProgramScript;

use super::support::{TestResult, fake, guard, path, program_run, read, ready, run, sh, write};

#[tokio::test]
async fn run_program_maps_sandbox_paths_records_env_and_returns_127_for_unknown_programs()
-> TestResult {
    guard(async {
        let fake = fake()?;
        let handle = ready(&fake, sh("sleep 3600")?).await?;
        let name = handle.name();
        let input = SandboxFile {
            path: path("/sandbox/in.txt")?,
            mode: FileMode(0o600),
            content: SecretBytes::new(b"hi".to_vec()),
        };
        fake.write_file(&name, &input).await?;
        assert_eq!(
            handle.file_mode(&path("/sandbox/in.txt")?),
            Some(FileMode(0o600))
        );

        let script = "cat \"$1\"; printf %s \"${HOME-unset}\" >&2; echo \"$FOO\" >&2; exit 4";
        let mut shell = program_run(&name, "/bin/sh", &["-c", script, "sh", "/sandbox/in.txt"])?;
        shell.env.insert(EnvName::parse("FOO")?, "bar".to_owned());
        let outcome = run(&fake, &shell).await?;
        assert_eq!(outcome.stdout, b"hi");
        assert_eq!(outcome.stderr, b"unsetbar\n");
        assert_eq!(outcome.exit_code, 4);
        let records = handle.runs(&path("/bin/sh")?);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].exit_code, 4);
        assert_eq!(
            records[0].env,
            [(EnvName::parse("FOO")?, "bar".to_owned())].into()
        );

        let unknown = run(&fake, &program_run(&name, "/usr/bin/nothing", &[])?).await?;
        assert_eq!(unknown.exit_code, 127);

        let hook = path("/usr/local/lib/jackin/hooks/setup")?;
        let failing = path("/usr/local/lib/jackin/hooks/preflight")?;
        for (program, exit_code) in [(&hook, 0), (&failing, 1)] {
            fake.script_program(
                program,
                ProgramScript {
                    exit_code,
                    stdout: b"scripted".to_vec(),
                    stderr: Vec::new(),
                },
            );
        }
        write(&fake, &name, "/sandbox/.jackin/seed", "s").await?;
        let mut ok = program_run(&name, hook.as_str(), &[])?;
        ok.env
            .insert(EnvName::parse("HOOK_VALUE")?, "from-env-file".to_owned());
        ok.touch_on_success = Some(path("/sandbox/.jackin/ok")?);
        ok.stdin = ProgramStdin::Closed;
        let outcome = run(&fake, &ok).await?;
        assert_eq!(
            (outcome.exit_code, outcome.stdout),
            (0, b"scripted".to_vec())
        );
        assert_eq!(handle.runs(&hook)[0].env.len(), 1);
        assert!(read(&fake, &name, "/sandbox/.jackin/ok").await?.is_some());
        let mut bad = program_run(&name, failing.as_str(), &[])?;
        bad.touch_on_success = Some(path("/sandbox/.jackin/bad")?);
        assert_eq!(run(&fake, &bad).await?.exit_code, 1);
        assert_eq!(read(&fake, &name, "/sandbox/.jackin/bad").await?, None);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn sha256sum_is_computed_in_process_over_the_sandbox_file() -> TestResult {
    guard(async {
        let fake = fake()?;
        let handle = ready(&fake, sh("sleep 3600")?).await?;
        let name = handle.name();
        write(&fake, &name, "/sandbox/abc", "abc").await?;
        let found = run(
            &fake,
            &program_run(&name, "/usr/bin/sha256sum", &["/sandbox/abc"])?,
        )
        .await?;
        assert_eq!(found.exit_code, 0);
        assert!(
            String::from_utf8_lossy(&found.stdout)
                .starts_with("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        );
        let missing = run(
            &fake,
            &program_run(&name, "/usr/bin/sha256sum", &["/sandbox/none"])?,
        )
        .await?;
        assert_ne!(missing.exit_code, 0);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn shell_scripts_map_every_sandbox_path_at_a_path_boundary_and_leave_look_alikes_alone()
-> TestResult {
    guard(async {
        let fake = fake()?;
        let handle = ready(&fake, sh("sleep 3600")?).await?;
        let name = handle.name();
        let scripts = [
            ("printf a >/sandbox/o.txt", "/sandbox/o.txt", "a"),
            ("printf b > \"/sandbox/q 1.txt\"", "/sandbox/q 1.txt", "b"),
            ("printf c > '/sandbox/q2.txt'", "/sandbox/q2.txt", "c"),
            ("V=/sandbox/v; printf d >$V", "/sandbox/v", "d"),
            ("cd /sandbox; printf e > e.txt", "/sandbox/e.txt", "e"),
        ];
        for (script, target, content) in scripts {
            let outcome = run(&fake, &program_run(&name, "/bin/sh", &["-c", script])?).await?;
            assert_eq!(outcome.exit_code, 0, "{script}: {outcome:?}");
            assert_eq!(
                handle.file(&path(target)?),
                Some(content.as_bytes().to_vec()),
                "{script}"
            );
        }
        let alike = run(
            &fake,
            &program_run(
                &name,
                "/bin/sh",
                &["-c", "echo /sandbox2/x /opt/sandbox/y /sandboxed"],
            )?,
        )
        .await?;
        assert_eq!(
            String::from_utf8_lossy(&alike.stdout),
            "/sandbox2/x /opt/sandbox/y /sandboxed\n"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_program_workdir_outside_sandbox_is_accepted_and_stays_inside_the_sandbox() -> TestResult
{
    guard(async {
        let fake = fake()?;
        let handle = ready(&fake, sh("sleep 3600")?).await?;
        let name = handle.name();
        let marker = format!("p-{}", jackin_core::LaunchId::generate()?);
        let mut request = program_run(&name, "/bin/sh", &["-c", &format!("printf t > {marker}")])?;
        request.workdir = Some(path("/tmp")?);
        let outcome = run(&fake, &request).await?;
        assert_eq!(outcome.exit_code, 0, "{outcome:?}");
        assert!(!tokio::fs::try_exists(std::path::Path::new("/tmp").join(&marker)).await?);
        assert_eq!(handle.file(&path(&format!("/sandbox/{marker}"))?), None);
        Ok(())
    })
    .await
}
