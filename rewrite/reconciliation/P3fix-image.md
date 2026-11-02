# P3 fix round, writer B: jackin-image reconciliation

Source of acceptance: `P3-fix-decisions.md` (section Writer B) and `P3-fix-test-plan.md` (section Writer B and the orchestrator additions), decisions D01, D05, D06, D08, D09, D17, D18. The orchestrator folds this file into `rewrite/API.md` section "Implementation reconciliation" (U4 and later).

Lines of API.md that this file rewrites: U4 S2, U4 S3, U4 S6 (fields 8 and 9), the U4 parser note "A directive line is `#key=value` at column 0", and the section 2 types `PlannedBuild`, `ContextSource`, `ImageRequest`, `EngineError` messages.

## B-R1. Directive and leading-whitespace rule

Source: BuildKit `dockerfile/1.28.0` (GitHub release of `moby/buildkit`, published 2026-10-07), files `frontend/dockerfile/parser/parser.go` and `frontend/dockerfile/parser/directives.go`, fetched from `raw.githubusercontent.com/moby/buildkit/dockerfile/1.28.0/...` on 2026-11-02.

- `Parse` calls `discardBOM` on the first physical line only. A U+FEFF anywhere else is data.
- `processLine(stripLeftWhitespace=true)` calls `trimLeadingWhitespace`, which is `bytes.TrimLeftFunc(src, unicode.IsSpace)`. It runs on the first physical line of every instruction and before the directive test. `unicode.IsSpace` is the Unicode White_Space property, so Rust `char::is_whitespace` is the same set (it includes U+00A0 and U+0085). Continuation lines keep their leading whitespace.
- `isComment` and `isEmptyContinuationLine` also use `trimLeadingWhitespace`. A line of Unicode whitespace is blank, and an indented `#` line is a comment.
- `DirectiveParser.ParseLine` removes `#`, then trims leading Unicode whitespace, then matches `^([a-zA-Z][a-zA-Z0-9]*)\s*=\s*(.+?)\s*$`. A non-matching line, or a key other than `syntax`, `escape`, `check`, ends the directive block for good. A blank first line therefore ends the block.
- Implementation: `crates/jackin-image/src/dockerfile.rs` `trim_leading`, used by `directive`, `skippable` and the first physical line of `Reader::read`; the BOM is stripped from line 1 in `Reader::new`.
- Beyond the plan: the same Unicode trim applies to the first physical line of an instruction, so `U+00A0 FROM ubuntu` is a `FROM` (BuildKit strips it; the old parser saw the word `U+00A0FROM` and skipped it, which let a final non-construct stage through). One extra row in T-B1 covers it.

## B-R2. Key encoding (replaces S6 fields 8 and 9)

`field(x) = len(x) as u64 big-endian (8 bytes) ++ bytes(x)`. The key is the lowercase hex SHA-256 of these fields in order:

1. role name
2. `agent.as_str()`
3. `clean` or `dirty`
4. commit
5. dirty digest hex, or empty when clean
6. hex SHA-256 of the role Dockerfile bytes
7. construct `ImageId` `Display`
8. the decimal count of derived build-arg tokens (`4` today)
9. one `field(token)` per `--build-arg=NAME=VALUE` token of the derived stage, in plan order: `--build-arg=CODEX_VERSION=<version>`, `--build-arg=CODEX_BIN_DIR=<dir>`, `--build-arg=CODEX_SHA256_ARM64=<sha>`, `--build-arg=CODEX_SHA256_AMD64=<sha>`
10. `platform.docker_arg()`
11. `absent` or `client`
12. hex SHA-256 of the embedded `jackin-start`
13. hex SHA-256 of the embedded derived Dockerfile
14. for `setup`, then `preflight`: `field(path or empty)` and `field(hex SHA-256 of bytes, or empty)`

The token is the whole string including `--build-arg=`. `ImagePlan` and `ImageKey::derive` take the list from one function (`key::derived_arguments`), so a build arg that P5.4 adds is hashed without another edit. `ImageRequest.context_files` is a new unused-by-key input (the destructure names it with `_`). `ImagePlan` no longer stores the `InstallTarget`; `derived_arguments` fails with `MissingInstallTarget` for either platform without a target.

## B-R3. Context and stdin plan data

- `ImageRequest.context: HostPath` stays the root. `ImageRequest.context_files: Vec<RepoPath>` is the output of `git ls-files -co --exclude-standard` in the role root (the D08 upload rule), supplied by the session layer (P5).
- `ContextSource::RoleRepository(HostPath)` is replaced by `ContextSource::Listed { root: HostPath, files: Vec<RepoPath> }`. `ImagePlan::final_builds` sorts and deduplicates `files`.
- `PlannedBuild` gains `stdin: Option<Vec<u8>>`. The role stage has `Some(request.dockerfile.bytes)`, the exact bytes that were validated and hashed. The construct and derived stages have `None`.
- S3 changes: the role stage carries the single token `--file=-` and no path (it replaces `--file=<context>/<dockerfile path>`). Nothing in any stage's args names the context path or the Dockerfile path.
- Engine contract for P4.3: `build` must copy exactly `files` (no symlink that leaves `root`) into a fresh directory under `work_dir`, run `docker buildx build ... --file=- <context dir>` with `stdin` as the process stdin, and not read the Dockerfile from disk.

## B-R4. `--pull`

`docker buildx build --help` (buildx v0.37.2, Docker 29.8.2, run 2026-11-02): `--pull    Always attempt to pull all referenced images`. It is a boolean flag. The buildx source (`build/opt.go` at tag `v0.37.2`, lines 580 to 585): `--pull` sets `image-resolve-mode=pull` (force pull); without it the docker driver sets `image-resolve-mode=local` preference (use the local image, pull only when it is absent). So `--pull=false` is the default and changes nothing, and no buildx flag stops a pull of a missing `docker-image://` named context. Decision: no `--pull` flag is added, and `FLAG_PREFIXES` in test 10 stays closed, so any `--pull=` fails the test. P4.3 proves live that a role build with an absent construct tag fails without registry contact (or records the exact behavior). The statement about the missing-image pull is from the buildx source and BuildKit's resolve-mode names; it is not run.

## B-R5. Syntax pin

Both Dockerfiles carry line 1:

`# syntax=docker/dockerfile:1.28.0@sha256:bb22d9815c728170f72750f4e5b0d672e06176142e1d602c7e66c050100b7e5b`

Source and date, 2026-11-02:
- `docker buildx imagetools inspect docker/dockerfile:1.28.0` printed `MediaType: application/vnd.oci.image.index.v1+json` and `Digest: sha256:bb22d981...e5b`. The same digest is in the Docker Hub tag list (`hub.docker.com/v2/repositories/docker/dockerfile/tags`, tag `1.28.0`, pushed 2026-10-07T23:50Z).
- `1.28.0` is the newest tag without `-labs` or `-rc`. The GitHub release `dockerfile/1.28.0` is not a pre-release.
- The moving tags `1` and `latest` still pointed at `1.27.1` (`sha256:4edf897a3ffa55b89f906fc8cc78afdb3f1834cc9c7083565e611a8a7d5fe99e`, pushed 2026-09-30) on this date. The pin follows the newest stable tag, not the moving tag. If P4.3 finds that the Docker 29.8.2 engine cannot run the 1.28.0 frontend, the fallback is the `1.27.1` tag with that digest; both lines must change together (T-B5 enforces equal lines).
- Parser directive lines are the only comment allowed in these files (AGENTS.md Code rules).

## B-R6. Construct

- The apt list gains `libcap2-bin` (it holds `getcap`; the NVIDIA base does not).
- The last root `RUN` before `USER sandbox` keeps the forbidden-tool loop and the setuid strip (`find / -xdev -type f -perm /6000 -print -exec chmod a-s {} +`, then a `test -z` re-scan) and ends with `command -v getcap >/dev/null && test -z "$(getcap -r / 2>/dev/null)"`. Any file capability, or a missing `getcap`, fails the build. `getcap` stderr is silenced; the `command -v` guard keeps a missing binary from passing as an empty result. The check has not run in a build (no image build in this task); P4.3 or P7.1 proves it live, including its run time over `/`.

## B-R7. Derived re-assertion

After the last `COPY` and before the final `USER sandbox`, still as root, one `RUN`:

```text
chown root:root /usr/local/bin /usr/local/lib/jackin /usr/local/lib/jackin/hooks
chmod 0755 the same three paths
for tool in sudo doas su-exec gosu docker podman iptables nft: command -v hit -> exit 1
test -z "$(find / -xdev -type f -perm /6000 -print)"
```

The scan fails the build; it does not strip. Residual, for the orchestrator: P5.4 installs the Docker client in the derived stage when the capability is enabled, so P5.4 must place its stage after this scan or make the tool list depend on `DockerLayer`. The derived stage has no `getcap` scan (file capabilities are inert under NoNewPrivs, per P0.5).

## B-R8. Drafts deletion and references

Deleted with `git rm`: `rewrite/drafts/construct.Dockerfile`, `rewrite/drafts/derived.Dockerfile`, `rewrite/drafts/jackin-start`, `rewrite/drafts/codex-provider.yaml`. Before deletion each was byte-identical to its crate copy (`crates/jackin-image/images/...` and `crates/jackin-core/src/assets/codex-provider.yaml`).

Fixed here: `rewrite/drafts/agentspec-codex.toml` line 34 (`profile_file` now `crates/jackin-core/src/assets/codex-provider.yaml`); a one-line pointer bullet after the `Task:` line of `rewrite/evidence/P0.3-codex.md`, `P7.1-image.md`, `P7.4-supervisor.md` (the history text is unchanged); `rewrite/README.md` line 17 (the `drafts/` row).

Left for the orchestrator (orchestrator-owned files):
- `rewrite/PLAN.md` line 162 (P0.3 scope names `drafts/construct.Dockerfile`, `derived.Dockerfile`, `codex-provider.yaml`), line 430 (P7.1 scope), line 431 (steps: "from the P0.3 drafts ... P4.3 moves them into `jackin-image`" is now past tense), line 456 (P7.4 scope). Line 289 names `agentspec-codex.toml`, which still exists; no change needed.
- `rewrite/API.md` line 319 (finding 1: "The P7.4 draft (`drafts/jackin-start`)": the script is now `crates/jackin-image/images/jackin-start`), line 3106 (U4 S2: "byte copies of `rewrite/drafts/{...}`" is replaced by "the crate files are the single source"), line 3034 (R3 names `codex-provider.yaml` without a path; no path change needed).
- `rewrite/DESIGN.md` has no path reference to the deleted files. D05 step 3 and the image paragraph may need two sentences: the role Dockerfile reaches `docker buildx build` on stdin from the validated bytes (`--file=-`), and the role context is the `git ls-files -co --exclude-standard` set; also the key now hashes the derived build-arg list (B-R2).
- `rewrite/evidence/P4.1-adapter-live.md` has no reference to the deleted files (checked with `grep`).
- `rewrite/decisions.tsv` mentions the draft names only as history.

## Other decisions the plan did not state

- `jackin-start` read line is `IFS= read -r attempt`. T-B7 found that `read -r` strips leading and trailing blanks, so `1 ` and ` 1` were accepted by the shell and rejected by `AttemptId::parse`. The script now rejects them (checked with `/bin/dash`, the image shell, and macOS `sh`).
- `BuildStage` implements `Display` (`construct`, `role`, `derived`). `EngineError` messages use Display for `BuilderName`, `ImageTag`, `BuildStage`, and `TerminalText::as_str()` (the tail starts on a new line). `ProcessStatus` has no `Display` in `jackin-core` (not in this writer's scope), so `engine.rs` renders it as `exit code N` or `signal N` through a private function.
- T-B6 and T-B7 use `std::env::temp_dir()` plus a `Drop` guard instead of the `tempfile` crate, so no dev-dependency is added. They spawn `Command::new("sh")` with `spawn` plus `wait_with_output` and `LC_ALL=C`.
- T-B3 and T-B4 read the Dockerfiles as text. The behavior itself (hostile role with `sudo`, a setuid binary, a file capability) is a P4.3 or P7.1 live hand-off.
