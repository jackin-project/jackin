// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::Command;

use jackin_xtask::{
    Check, CiStep, CommentsCheck, ContainersCheck, ErrorsCheck, LayersCheck, PinsCheck, Repo,
    Violation,
};
use tempfile::TempDir;

type TestResult = Result<(), Box<dyn Error>>;

type Found = (CiStep, String, Option<u32>);

const PIN: &str = env!("CARGO_PKG_RUST_VERSION");
const OTHER: &str = "1.0.0";

#[derive(Clone, Copy)]
enum Dep {
    Normal(&'static str),
    Dev(&'static str),
    Renamed(&'static str, &'static str),
}

impl Dep {
    fn name(self) -> &'static str {
        match self {
            Dep::Normal(name) | Dep::Dev(name) => name,
            Dep::Renamed(_, package) => package,
        }
    }
}

type Specs = Vec<(&'static str, Vec<Dep>)>;

struct Fixture {
    dir: TempDir,
}

impl Fixture {
    fn new(specs: &Specs) -> Result<Self, Box<dyn Error>> {
        let fixture = Self {
            dir: tempfile::tempdir()?,
        };
        fixture.write(".gitignore", "target/\n")?;
        let members: Vec<String> = specs
            .iter()
            .map(|(name, _)| format!("\"crates/{name}\""))
            .collect();
        let mut shared: Vec<&str> = specs
            .iter()
            .flat_map(|(_, deps)| deps.iter())
            .filter(|dep| !matches!(dep, Dep::Renamed(..)))
            .map(|dep| dep.name())
            .collect();
        shared.sort_unstable();
        shared.dedup();
        let shared: String = shared
            .iter()
            .map(|name| [*name, " = \"0.1.0\"\n"].concat())
            .collect();
        fixture.write(
            "Cargo.toml",
            &format!(
                "[workspace]\nmembers = [{}]\nresolver = \"3\"\n\n[workspace.package]\nrust-version = \"{PIN}\"\n\n[workspace.dependencies]\n{shared}",
                members.join(", ")
            ),
        )?;
        for (name, deps) in specs {
            fixture.write(&format!("crates/{name}/Cargo.toml"), &manifest(name, deps))?;
            fixture.write(&format!("crates/{name}/src/lib.rs"), "")?;
        }
        Ok(fixture)
    }

    fn write(&self, relative: &str, text: &str) -> TestResult {
        let path = self.dir.path().join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, text)?;
        Ok(())
    }

    fn git(&self, args: &[&str]) -> TestResult {
        let status = jackin_testkit::git_command()
            .args(args)
            .current_dir(self.dir.path())
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("git {args:?} exited with {status}").into())
        }
    }

    fn write_bytes(&self, relative: &str, bytes: &[u8]) -> TestResult {
        let path = self.dir.path().join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, bytes)?;
        Ok(())
    }

    fn remove(&self, relative: &str) -> TestResult {
        fs::remove_file(self.dir.path().join(relative))?;
        Ok(())
    }

    fn write_pins(
        &self,
        cargo: &str,
        toolchain: &str,
        mise_tool: &str,
        fast: &str,
        no_docs: &str,
    ) -> TestResult {
        let root = fs::read_to_string(self.dir.path().join("Cargo.toml"))?;
        self.write(
            "Cargo.toml",
            &root.replace(
                &format!("rust-version = \"{PIN}\""),
                &format!("rust-version = \"{cargo}\""),
            ),
        )?;
        self.write(
            "rust-toolchain.toml",
            &format!("[toolchain]\nchannel = \"{toolchain}\"\n"),
        )?;
        self.write("mise.toml", &mise_text(mise_tool, fast, no_docs))
    }

    fn prepare(&self) -> TestResult {
        self.git(&["init", "-q"])?;
        self.git(&["add", "-A"])
    }

    fn open_at(&self, relative: &str) -> Result<Repo, Box<dyn Error>> {
        Ok(Repo::open(self.dir.path().join(relative))?)
    }

    fn repo(&self) -> Result<Repo, Box<dyn Error>> {
        self.prepare()?;
        self.open_at(".")
    }
}

fn mise_text(tool: &str, fast: &str, no_docs: &str) -> String {
    format!(
        "[tools]\nrust = {{ version = \"{tool}\", profile = \"minimal\" }}\n\n[tasks.\"ci:fast\"]\nrun = \"mbx +{fast} xtask ci --fast\"\n\n[tasks.\"ci:full\"]\nrun = \"mbx +{no_docs} xtask ci --no-docs\"\n"
    )
}

fn text(lines: &[&str]) -> String {
    let mut joined = lines.join("\n");
    joined.push('\n');
    joined
}

fn at(step: CiStep, path: &str, line: u32) -> Found {
    (step, path.to_owned(), Some(line))
}

fn manifest(name: &str, deps: &[Dep]) -> String {
    let mut normal = String::new();
    let mut dev = String::new();
    for dep in deps {
        match dep {
            Dep::Normal(dep) => normal.push_str(&format!("{dep} = {{ workspace = true }}\n")),
            Dep::Dev(dep) => dev.push_str(&format!("{dep} = {{ workspace = true }}\n")),
            Dep::Renamed(alias, package) => normal.push_str(&format!(
                "{alias} = {{ package = \"{package}\", version = \"0.1.0\" }}\n"
            )),
        }
    }
    format!(
        "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\n{normal}\n[dev-dependencies]\n{dev}"
    )
}

fn line_of(text: &str, tag: &str) -> u32 {
    let hits: Vec<usize> = text
        .lines()
        .enumerate()
        .filter(|(_, line)| line.contains(tag))
        .map(|(index, _)| index)
        .collect();
    assert_eq!(hits.len(), 1, "tag {tag:?} must mark exactly one line");
    u32::try_from(hits[0] + 1).unwrap_or(u32::MAX)
}

fn summary(violations: &[Violation]) -> Vec<Found> {
    violations
        .iter()
        .map(|violation| {
            (
                violation.step,
                violation
                    .path
                    .as_deref()
                    .map(Path::to_string_lossy)
                    .map(String::from)
                    .unwrap_or_default(),
                violation.line,
            )
        })
        .collect()
}

fn lines(step: CiStep, path: &str, text: &str, tags: &[&str]) -> Vec<Found> {
    tags.iter()
        .map(|tag| (step, path.to_owned(), Some(line_of(text, tag))))
        .collect()
}

fn manifest_found(step: CiStep, member: &str) -> Found {
    (step, format!("crates/{member}/Cargo.toml"), None)
}

fn bare(names: &[&'static str]) -> Specs {
    names.iter().map(|name| (*name, Vec::new())).collect()
}

fn add(specs: &mut Specs, member: &str, dep: Dep) {
    for (name, deps) in specs.iter_mut() {
        if *name == member {
            deps.push(dep);
        }
    }
}

fn usage_provider() -> Vec<Dep> {
    vec![
        Dep::Normal("jackin-usage-provider-core"),
        Dep::Normal("serde_json"),
    ]
}

fn d14() -> Specs {
    use Dep::{Dev, Normal};
    let core = Normal("jackin-core");
    let image = Normal("jackin-image");
    let openshell = Normal("jackin-openshell");
    let session = Normal("jackin-session");
    let tui = Normal("jackin-tui");
    let testkit = Dev("jackin-testkit");
    vec![
        ("jackin-core", vec![Normal("serde"), Normal("thiserror")]),
        ("jackin-image", vec![core, Normal("tokio"), testkit]),
        (
            "jackin-openshell",
            vec![
                core,
                Normal("tonic"),
                Normal("russh"),
                Normal("openshell-sdk"),
                Normal("openshell-policy"),
                testkit,
            ],
        ),
        ("jackin-session", vec![core, image, openshell, testkit]),
        (
            "jackin-tui",
            vec![
                core,
                image,
                openshell,
                session,
                Normal("junie-tui"),
                testkit,
            ],
        ),
        (
            "jackin-testkit",
            vec![core, image, openshell, session, tui, Normal("jackin-usage")],
        ),
        (
            "jackin",
            vec![
                core,
                image,
                openshell,
                session,
                tui,
                Normal("jackin-usage"),
                Normal("clap"),
                testkit,
            ],
        ),
        (
            "jackin-usage",
            vec![
                core,
                Normal("jackin-usage-provider-core"),
                Normal("jackin-usage-provider-codex"),
                Normal("serde"),
                testkit,
            ],
        ),
        (
            "jackin-usage-provider-core",
            vec![Normal("reqwest"), Normal("serde_json"), Normal("thiserror")],
        ),
        ("jackin-usage-provider-claude", usage_provider()),
        ("jackin-usage-provider-codex", usage_provider()),
        ("jackin-usage-provider-cursor", usage_provider()),
        ("jackin-usage-provider-gemini", usage_provider()),
        ("jackin-usage-provider-grok", usage_provider()),
        ("jackin-usage-provider-kimi", usage_provider()),
        ("jackin-usage-provider-minimax", usage_provider()),
        ("jackin-usage-provider-opencode", usage_provider()),
        ("jackin-usage-provider-openrouter", usage_provider()),
        ("jackin-usage-provider-zai", usage_provider()),
        (
            "jackin-xtask",
            vec![
                Normal("jackin-core"),
                Normal("jackin-openshell"),
                Normal("clap"),
                testkit,
            ],
        ),
    ]
}

fn assert_messages(violations: &[Violation], needles: &[&[&str]]) {
    assert_eq!(violations.len(), needles.len());
    for (violation, needles) in violations.iter().zip(needles) {
        for needle in *needles {
            assert!(
                violation.message.contains(needle),
                "message {:?} must name {needle:?}",
                violation.message
            );
        }
    }
}

mod comments {
    use super::*;

    const HEADER: &str =
        "// SPDX-FileCopyrightText: 2026 Alexey Zhokhov\n// SPDX-License-Identifier: Apache-2.0\n";

    const HASH_HEADER: &str =
        "# SPDX-FileCopyrightText: 2026 Alexey Zhokhov\n# SPDX-License-Identifier: Apache-2.0\n";

    fn run(fixture: &Fixture) -> Result<Vec<Found>, Box<dyn Error>> {
        Ok(summary(&CommentsCheck.run(&fixture.repo()?)))
    }

    #[test]
    fn rust_header_and_string_contents_are_not_comments() -> TestResult {
        let source = format!(
            "{HEADER}\nfn f<'a>(x: &'a str) {{\n    let a = \"// x\";\n    let b = \"/* x */\";\n    let c = \"/// x\";\n    let d = r#\"// raw \"q /* \"#;\n    let e = b\"//\";\n    let f = \"\\\" // still string\";\n    let g = '\"';\n    let h = \"a // b\";\n    let i = '/';\n}}\n"
        );
        let fixture = Fixture::new(&bare(&["jackin-core"]))?;
        fixture.write("crates/jackin-core/src/lib.rs", &source)?;
        assert_eq!(run(&fixture)?, []);
        Ok(())
    }

    #[test]
    fn a_comment_in_the_usage_subtree_is_reported() -> TestResult {
        let source = format!("{HEADER}\n// note\nfn f() {{}}\n");
        let fixture = Fixture::new(&bare(&["jackin-usage"]))?;
        fixture.write("crates/usage/jackin-usage/src/lib.rs", &source)?;
        let expected = lines(
            CiStep::Comments,
            "crates/usage/jackin-usage/src/lib.rs",
            &source,
            &["note"],
        );
        assert_eq!(run(&fixture)?, expected);
        Ok(())
    }

    #[test]
    fn every_rust_comment_form_is_reported_at_its_line() -> TestResult {
        let late = ["// SPDX-License", "-Identifier: MIT"].concat();
        let source = format!(
            "{HEADER}\n//! inner\nfn f<'a>() {{ // after lifetime\n    let c = '\"'; // after char quote\n    /* one-line */\n    /* start\n       middle\n       end */\n    /* outer /* nested */ tail\n       // inside outer */\n    /** doc block */\n    /// doc line\n    // plain\n}}\n{late}\n"
        );
        let fixture = Fixture::new(&bare(&["jackin-core"]))?;
        fixture.write("crates/jackin-core/src/lib.rs", &source)?;
        let expected = lines(
            CiStep::Comments,
            "crates/jackin-core/src/lib.rs",
            &source,
            &[
                "inner",
                "after lifetime",
                "after char quote",
                "one-line",
                "/* start",
                "nested",
                "doc block",
                "doc line",
                "plain",
                "Identifier: MIT",
            ],
        );
        assert_eq!(run(&fixture)?, expected);
        Ok(())
    }

    #[test]
    fn non_rust_headers_directives_and_quoted_hashes_pass() -> TestResult {
        let toml = format!(
            "{HASH_HEADER}\nurl = \"http://h/#frag\"\nl = 'a # b'\n\"q # k\" = 1\ntext = \"\"\"\n# inside\nmore\n\"\"\"\n"
        );
        let shell = format!(
            "#!/usr/bin/env bash\n{HASH_HEADER}\necho \"a # b\"\necho 'a # b'\necho a#b\necho $# \"${{#1}}\"\n"
        );
        let dockerfile = format!(
            "# syntax=docker/dockerfile:1\n{HASH_HEADER}\nFROM scratch\nRUN echo \"a # b\"\n"
        );
        let fixture = Fixture::new(&bare(&["jackin-core"]))?;
        fixture.write("config/extra.toml", &toml)?;
        fixture.write("scripts/run.sh", &shell)?;
        fixture.write("images/Dockerfile", &dockerfile)?;
        fixture.write("images/construct.Dockerfile", &dockerfile)?;
        assert_eq!(run(&fixture)?, []);
        Ok(())
    }

    #[test]
    fn non_rust_hash_comments_and_misplaced_directives_are_reported() -> TestResult {
        let toml = format!("{HASH_HEADER}\n# alpha\nk = 1 # beta\n[t] # gamma\n");
        let shell = format!("#!/bin/sh\n{HASH_HEADER}\n# alpha\ncmd # beta\n#!/bin/bash\n");
        let dockerfile = format!(
            "# syntax=docker/dockerfile:1\n{HASH_HEADER}\nFROM scratch\n# alpha\n# syntax=docker/dockerfile:1.7\n"
        );
        let fixture = Fixture::new(&bare(&["jackin-core"]))?;
        fixture.write("config/extra.toml", &toml)?;
        fixture.write("scripts/run.sh", &shell)?;
        fixture.write("images/Dockerfile", &dockerfile)?;
        let mut expected = lines(
            CiStep::Comments,
            "config/extra.toml",
            &toml,
            &["# alpha", "beta", "gamma"],
        );
        expected.extend(lines(
            CiStep::Comments,
            "images/Dockerfile",
            &dockerfile,
            &["# alpha", "dockerfile:1.7"],
        ));
        expected.extend(lines(
            CiStep::Comments,
            "scripts/run.sh",
            &shell,
            &["# alpha", "beta", "#!/bin/bash"],
        ));
        assert_eq!(run(&fixture)?, expected);
        Ok(())
    }

    #[test]
    fn build_output_and_unscanned_file_types_are_skipped() -> TestResult {
        let fixture = Fixture::new(&bare(&["jackin-core"]))?;
        fixture.remove(".gitignore")?;
        fixture.write("target/debug/gen.rs", "fn f() {} // c\n")?;
        fixture.write("target/x.toml", "# c\n")?;
        fixture.write("docs/page.md", "# Heading\n")?;
        fixture.write(".github/workflows/ci.yml", "# note\n")?;
        fixture.write("scripts/real.sh", "# real\n")?;
        assert_eq!(run(&fixture)?, [at(CiStep::Comments, "scripts/real.sh", 1)]);
        Ok(())
    }

    #[test]
    fn generated_exemption_needs_the_velnor_header_on_line_one_in_any_path() -> TestResult {
        let header =
            "# Generated by Velnor Actions 0.1.71; edit .velnor/config.toml and regenerate.";
        let generated = text(&[header, "# body note", "echo \"${#v}\""]);
        let fixture = Fixture::new(&bare(&["jackin-core"]))?;
        fixture.write(".github/scripts/gen.sh", &generated)?;
        fixture.write("tools/gen.sh", &generated)?;
        fixture.write(
            ".github/scripts/deploy.sh",
            &text(&["# hand written note", "echo hi"]),
        )?;
        fixture.write(
            ".github/scripts/late.sh",
            &text(&["#!/bin/sh", header, "# body note"]),
        )?;
        fixture.write("scripts/almost.sh", "# Generated by hand\n")?;
        assert_eq!(
            run(&fixture)?,
            [
                at(CiStep::Comments, ".github/scripts/deploy.sh", 1),
                at(CiStep::Comments, ".github/scripts/late.sh", 2),
                at(CiStep::Comments, ".github/scripts/late.sh", 3),
                at(CiStep::Comments, "scripts/almost.sh", 1),
            ]
        );
        Ok(())
    }

    #[test]
    fn spdx_lines_must_carry_a_known_tag_and_a_value() -> TestResult {
        let empty_copyright = ["// SPDX-FileCopyrightText", ":"].concat();
        let blank_license = ["// SPDX-License-Identifier", ":   "].concat();
        let rust = format!(
            "{HEADER}// SPDX-this is a free form comment\n{empty_copyright}\n{blank_license}\n// SPDX-Notes: not a tag\nfn f() {{}}\n"
        );
        let empty_hash = ["# SPDX-License-Identifier", ":"].concat();
        let toml = format!("{HASH_HEADER}# SPDX-this is free form\n{empty_hash}\nk = 1\n");
        let fixture = Fixture::new(&bare(&["jackin-core"]))?;
        fixture.write("crates/jackin-core/src/lib.rs", &rust)?;
        fixture.write("config/extra.toml", &toml)?;
        let rust_path = "crates/jackin-core/src/lib.rs";
        assert_eq!(
            run(&fixture)?,
            [
                at(CiStep::Comments, "config/extra.toml", 3),
                at(CiStep::Comments, "config/extra.toml", 4),
                at(CiStep::Comments, rust_path, 3),
                at(CiStep::Comments, rust_path, 4),
                at(CiStep::Comments, rust_path, 5),
                at(CiStep::Comments, rust_path, 6),
            ]
        );
        Ok(())
    }

    #[test]
    fn shell_heredoc_bodies_are_skipped_and_the_code_after_them_is_scanned() -> TestResult {
        let shell = text(&[
            "#!/bin/sh",
            "cat <<EOF2",
            "it's a heredoc",
            "# inside body",
            "EOF2",
            "echo 'a # b'",
            "cat <<-DASH",
            "\tit's tabbed",
            "\t# tab body",
            "\tDASH",
            "echo \"x # y\"",
            "cat <<'QUOTED'",
            "don't # body",
            "QUOTED",
            "cat <<\"DQ\"",
            "# dq body \"",
            "DQ",
            "cat <<EOF3 # opener note",
            "body 3",
            "EOF3",
            "cat <<< \"it's\"",
            "# after herestring",
            "echo done # trailing note",
        ]);
        let fixture = Fixture::new(&bare(&["jackin-core"]))?;
        fixture.write("scripts/run.sh", &shell)?;
        assert_eq!(
            run(&fixture)?,
            [
                at(CiStep::Comments, "scripts/run.sh", 18),
                at(CiStep::Comments, "scripts/run.sh", 22),
                at(CiStep::Comments, "scripts/run.sh", 23),
            ]
        );
        Ok(())
    }

    #[test]
    fn shell_arithmetic_shift_is_not_a_heredoc() -> TestResult {
        let shell = text(&[
            "#!/bin/sh",
            "x=$((1<<2))",
            "y=$(( ((1 + 2)) << 3 ))",
            "echo \"$x\" \"$y\" # note one",
            "# note two",
            "z=$(( 1 << 4 )) # note three",
            "cat <<EOF",
            "# inside the heredoc body",
            "EOF",
            "echo done # note four",
        ]);
        let fixture = Fixture::new(&bare(&["jackin-core"]))?;
        fixture.write("scripts/arith.sh", &shell)?;
        assert_eq!(
            run(&fixture)?,
            [4, 5, 6, 10].map(|line| at(CiStep::Comments, "scripts/arith.sh", line))
        );
        Ok(())
    }

    #[test]
    fn shell_backtick_starts_a_word() -> TestResult {
        let shell = text(&["#!/bin/sh", "x=`# alpha", "echo y`", "echo a#b `echo c#d`"]);
        let fixture = Fixture::new(&bare(&["jackin-core"]))?;
        fixture.write("scripts/sub.sh", &shell)?;
        assert_eq!(run(&fixture)?, [at(CiStep::Comments, "scripts/sub.sh", 2)]);
        Ok(())
    }

    #[test]
    fn dockerfile_variants_and_shebang_scripts_are_scanned() -> TestResult {
        let dev = format!("# syntax=docker/dockerfile:1\n{HASH_HEADER}FROM scratch\n# c3 note\n");
        let plain = format!("{HASH_HEADER}FROM scratch\n# note\n");
        let start = format!(
            "#!/bin/sh\n{HASH_HEADER}echo \"a # b\"\n# supervisor note\necho $# \"${{#1}}\"\n"
        );
        let fixture = Fixture::new(&bare(&["jackin-core"]))?;
        fixture.write("images/Dockerfile.dev", &dev)?;
        fixture.write("images/Containerfile", &plain)?;
        fixture.write("images/role.Containerfile", &plain)?;
        fixture.write("crates/jackin-image/assets/jackin-start", &start)?;
        fixture.write("bin/tool", "#!/usr/bin/env bash\necho \"a # b\"\n")?;
        fixture.write("docs/notes", "# Heading\n")?;
        fixture.write("docs/late", "echo hi\n#!/bin/sh\n# note\n")?;
        let mut expected = lines(
            CiStep::Comments,
            "crates/jackin-image/assets/jackin-start",
            &start,
            &["supervisor note"],
        );
        expected.extend(lines(
            CiStep::Comments,
            "images/Containerfile",
            &plain,
            &["# note"],
        ));
        expected.extend(lines(
            CiStep::Comments,
            "images/Dockerfile.dev",
            &dev,
            &["c3 note"],
        ));
        expected.extend(lines(
            CiStep::Comments,
            "images/role.Containerfile",
            &plain,
            &["# note"],
        ));
        assert_eq!(run(&fixture)?, expected);
        Ok(())
    }

    #[test]
    fn open_ignores_binary_files_without_a_shebang() -> TestResult {
        let bytes = [0xff, 0xfe, 0x00, b'#', b'!'];
        let fixture = Fixture::new(&bare(&["jackin-core"]))?;
        fixture.write_bytes("assets/blob", &bytes)?;
        fixture.write_bytes("assets/pic.png", &bytes)?;
        assert_eq!(run(&fixture)?, []);
        Ok(())
    }
}

mod layers {
    use super::*;

    fn run(specs: &Specs, files: &[(&str, &str)]) -> Result<Vec<Violation>, Box<dyn Error>> {
        let fixture = Fixture::new(specs)?;
        for (path, text) in files {
            fixture.write(path, text)?;
        }
        Ok(LayersCheck.run(&fixture.repo()?))
    }

    #[test]
    fn d14_graph_with_inheritance_and_external_crates_passes() -> TestResult {
        assert_eq!(summary(&run(&d14(), &[])?), []);
        Ok(())
    }

    #[test]
    fn edges_outside_the_row_are_reported() -> TestResult {
        use Dep::{Dev, Normal};
        let mut specs = d14();
        add(&mut specs, "jackin-image", Normal("jackin-openshell"));
        add(&mut specs, "jackin-openshell", Normal("jackin-image"));
        add(&mut specs, "jackin-core", Normal("jackin-session"));
        add(&mut specs, "jackin-session", Normal("jackin-usage"));
        add(&mut specs, "jackin", Normal("jackin-usage-provider-codex"));
        add(&mut specs, "jackin-xtask", Normal("jackin-tui"));
        add(&mut specs, "jackin-tui", Normal("jackin-testkit"));
        add(&mut specs, "jackin-image", Dev("jackin-session"));
        let violations = run(&specs, &[])?;
        let expected: Vec<Found> = [
            "jackin-core",
            "jackin-image",
            "jackin-image",
            "jackin-openshell",
            "jackin-session",
            "jackin-tui",
            "jackin-xtask",
            "jackin",
        ]
        .iter()
        .map(|member| manifest_found(CiStep::Layers, member))
        .collect();
        assert_eq!(summary(&violations), expected);
        assert_messages(
            &violations,
            &[
                &["jackin-core", "jackin-session"],
                &["jackin-image", "jackin-openshell"],
                &["jackin-image", "jackin-session"],
                &["jackin-openshell", "jackin-image"],
                &["jackin-session", "jackin-usage"],
                &["jackin-tui", "jackin-testkit"],
                &["jackin-xtask", "jackin-tui"],
                &["jackin", "jackin-usage-provider-codex"],
            ],
        );
        Ok(())
    }

    #[test]
    fn gateway_crates_in_manifests_outside_openshell_are_reported() -> TestResult {
        use Dep::{Dev, Normal, Renamed};
        let mut specs = d14();
        add(&mut specs, "jackin-core", Normal("tonic"));
        add(&mut specs, "jackin-image", Dev("openshell-policy"));
        add(&mut specs, "jackin-session", Normal("russh"));
        add(&mut specs, "jackin-tui", Normal("openshell-sdk"));
        add(&mut specs, "jackin-testkit", Renamed("grpc", "tonic-prost"));
        add(&mut specs, "jackin", Normal("openshell-core"));
        add(&mut specs, "jackin-xtask", Normal("russh-sftp"));
        add(&mut specs, "jackin-openshell", Normal("tonic-prost"));
        add(&mut specs, "jackin-openshell", Normal("russh-sftp"));
        add(&mut specs, "jackin-openshell", Normal("openshell-core"));
        add(&mut specs, "jackin-session", Normal("tonicity"));
        add(&mut specs, "jackin-session", Normal("russhound"));
        let violations = run(&specs, &[])?;
        let expected: Vec<Found> = [
            "jackin-core",
            "jackin-image",
            "jackin-session",
            "jackin-testkit",
            "jackin-tui",
            "jackin-xtask",
            "jackin",
        ]
        .iter()
        .map(|member| manifest_found(CiStep::Layers, member))
        .collect();
        assert_eq!(summary(&violations), expected);
        assert_messages(
            &violations,
            &[
                &["jackin-core", "tonic"],
                &["jackin-image", "openshell-policy"],
                &["jackin-session", "russh"],
                &["jackin-testkit", "tonic-prost"],
                &["jackin-tui", "openshell-sdk"],
                &["jackin-xtask", "russh-sftp"],
                &["jackin", "openshell-core"],
            ],
        );
        assert!(!violations[3].message.contains("grpc"));
        Ok(())
    }

    #[test]
    fn gateway_crate_imports_in_source_outside_openshell_are_reported() -> TestResult {
        let gateway = "use openshell_sdk::Client;\n";
        let net = "fn connect() {\n    let _ = russh::client::connect;\n}\n";
        let channel = "use tonic::{Status, transport::Channel};\n";
        let prost = "use tonic_prost::Message;\n";
        let sftp = "fn f() { let _ = russh_sftp::client::SftpSession::new; }\n";
        let core = "use openshell_core::Thing;\n";
        let adapter = "use openshell_policy::Policy;\nuse openshell_sdk::Client;\nuse russh::client;\nuse tonic::Status;\nuse tonic_prost::Message;\nuse russh_sftp::client;\nuse openshell_core::Thing;\n";
        let quoted = "fn docs() {\n    let normal = \"use openshell_sdk::Client;\";\n    let raw = r#\"use openshell_sdk::Client;\"#;\n    let tonic_status = 1;\n    tonicity::run();\n    openshellfish::run();\n    crate::tonic_helpers::f();\n}\n";
        let violations = run(
            &d14(),
            &[
                ("crates/jackin-session/src/gw.rs", gateway),
                ("crates/jackin-image/src/net.rs", net),
                ("crates/jackin/src/m.rs", channel),
                ("crates/jackin-tui/src/p.rs", prost),
                ("crates/jackin-core/src/s.rs", sftp),
                ("crates/jackin-testkit/src/o.rs", core),
                ("crates/jackin-openshell/src/lib.rs", adapter),
                ("crates/jackin-session/src/doc.rs", quoted),
            ],
        )?;
        let expected = vec![
            at(CiStep::Layers, "crates/jackin-core/src/s.rs", 1),
            at(
                CiStep::Layers,
                "crates/jackin-image/src/net.rs",
                line_of(net, "russh"),
            ),
            at(CiStep::Layers, "crates/jackin-session/src/gw.rs", 1),
            at(CiStep::Layers, "crates/jackin-testkit/src/o.rs", 1),
            at(CiStep::Layers, "crates/jackin-tui/src/p.rs", 1),
            at(CiStep::Layers, "crates/jackin/src/m.rs", 1),
        ];
        assert_eq!(summary(&violations), expected);
        assert_messages(
            &violations,
            &[
                &["jackin-core", "russh"],
                &["jackin-image", "russh"],
                &["jackin-session", "openshell"],
                &["jackin-testkit", "openshell"],
                &["jackin-tui", "tonic"],
                &["jackin", "tonic"],
            ],
        );
        Ok(())
    }

    #[test]
    fn usage_edges_outside_the_rows_are_reported() -> TestResult {
        use Dep::{Dev, Normal};
        let mut specs = d14();
        add(&mut specs, "jackin-usage", Normal("jackin-openshell"));
        add(&mut specs, "jackin-usage", Normal("jackin-session"));
        add(
            &mut specs,
            "jackin-usage-provider-codex",
            Normal("jackin-core"),
        );
        add(
            &mut specs,
            "jackin-usage-provider-claude",
            Normal("jackin-usage"),
        );
        add(
            &mut specs,
            "jackin-usage-provider-core",
            Normal("jackin-usage-provider-codex"),
        );
        add(&mut specs, "jackin-tui", Normal("jackin-usage"));
        add(&mut specs, "jackin", Normal("jackin-usage-provider-core"));
        add(
            &mut specs,
            "jackin-usage-provider-kimi",
            Dev("jackin-testkit"),
        );
        let violations = run(&specs, &[])?;
        let expected: Vec<Found> = [
            "jackin-tui",
            "jackin-usage-provider-claude",
            "jackin-usage-provider-codex",
            "jackin-usage-provider-core",
            "jackin-usage-provider-kimi",
            "jackin-usage",
            "jackin-usage",
            "jackin",
        ]
        .iter()
        .map(|member| manifest_found(CiStep::Layers, member))
        .collect();
        assert_eq!(summary(&violations), expected);
        assert_messages(
            &violations,
            &[
                &["jackin-tui", "jackin-usage"],
                &["jackin-usage-provider-claude", "jackin-usage"],
                &["jackin-usage-provider-codex", "jackin-core"],
                &["jackin-usage-provider-core", "jackin-usage-provider-codex"],
                &["jackin-usage-provider-kimi", "jackin-testkit"],
                &["jackin-usage", "jackin-openshell"],
                &["jackin-usage", "jackin-session"],
                &["jackin", "jackin-usage-provider-core"],
            ],
        );
        Ok(())
    }

    #[test]
    fn usage_edges_inside_the_rows_pass() -> TestResult {
        assert_eq!(summary(&run(&d14(), &[])?), []);
        Ok(())
    }

    #[test]
    fn reqwest_in_a_manifest_outside_the_http_owner_is_reported() -> TestResult {
        use Dep::{Dev, Normal};
        let mut specs = d14();
        add(&mut specs, "jackin-session", Normal("reqwest"));
        add(&mut specs, "jackin-image", Dev("reqwest"));
        let violations = run(&specs, &[])?;
        assert_eq!(
            summary(&violations),
            [
                manifest_found(CiStep::Layers, "jackin-image"),
                manifest_found(CiStep::Layers, "jackin-session"),
            ]
        );
        assert_messages(
            &violations,
            &[
                &["jackin-image", "reqwest", "jackin-usage-provider-core"],
                &["jackin-session", "reqwest", "jackin-usage-provider-core"],
            ],
        );
        Ok(())
    }

    #[test]
    fn reqwest_imports_in_source_outside_the_http_owner_are_reported() -> TestResult {
        let group = "use reqwest::{Client, StatusCode};\n";
        let path = "fn f() {\n    let _client = reqwest::Client::new();\n}\n";
        let extern_crate = "extern crate reqwest;\n";
        let in_macro = "fn f() {\n    wrap!(reqwest::get(\"x\"));\n}\n";
        let owner = "use reqwest::Client;\nfn f() { let _ = reqwest::get(\"x\"); }\n";
        let quiet = "fn docs() {\n    let text = \"use reqwest::Client;\";\n    let raw = r#\"reqwest::Client\"#;\n    reqwest_helper::run();\n    crate::reqwest_stub::f();\n}\n";
        let violations = run(
            &d14(),
            &[
                ("crates/jackin-session/src/a.rs", group),
                ("crates/jackin-image/src/b.rs", path),
                ("crates/jackin-core/src/c.rs", extern_crate),
                ("crates/jackin-tui/src/d.rs", in_macro),
                ("crates/usage/jackin-usage-provider-core/src/lib.rs", owner),
                ("crates/jackin-testkit/src/q.rs", quiet),
            ],
        )?;
        let expected = vec![
            at(CiStep::Layers, "crates/jackin-core/src/c.rs", 1),
            at(
                CiStep::Layers,
                "crates/jackin-image/src/b.rs",
                line_of(path, "reqwest"),
            ),
            at(CiStep::Layers, "crates/jackin-session/src/a.rs", 1),
            at(
                CiStep::Layers,
                "crates/jackin-tui/src/d.rs",
                line_of(in_macro, "reqwest"),
            ),
        ];
        assert_eq!(summary(&violations), expected);
        assert_messages(
            &violations,
            &[
                &["jackin-core", "reqwest"],
                &["jackin-image", "reqwest"],
                &["jackin-session", "reqwest"],
                &["jackin-tui", "reqwest"],
            ],
        );
        Ok(())
    }

    #[test]
    fn member_without_a_row_is_reported() -> TestResult {
        let mut specs = d14();
        specs.push(("jackin-newcrate", vec![Dep::Normal("serde")]));
        let violations = run(&specs, &[])?;
        assert_eq!(
            summary(&violations),
            [manifest_found(CiStep::Layers, "jackin-newcrate")]
        );
        assert_messages(&violations, &[&["jackin-newcrate"]]);
        Ok(())
    }
}

mod expose {
    use super::*;

    fn run(files: &[(&str, &str)]) -> Result<Vec<Violation>, Box<dyn Error>> {
        let fixture = Fixture::new(&d14())?;
        for (path, text) in files {
            fixture.write(path, text)?;
        }
        Ok(LayersCheck.run(&fixture.repo()?))
    }

    #[test]
    fn expose_calls_outside_the_allowlist_are_reported_at_their_line() -> TestResult {
        let source = "fn f(secret: &Secret) {\n    let _text = secret.expose();\n    let _bytes = SecretBytes::expose(&secret);\n}\n";
        let path = "crates/usage/jackin-usage/src/cache.rs";
        let violations = run(&[(path, source)])?;
        assert_eq!(
            summary(&violations),
            [at(CiStep::Layers, path, 2), at(CiStep::Layers, path, 3)]
        );
        assert_messages(
            &violations,
            &[
                &["Secret::expose", "allowlist"],
                &["Secret::expose", "allowlist"],
            ],
        );
        Ok(())
    }

    #[test]
    fn expose_calls_in_the_allowlisted_credential_file_pass() -> TestResult {
        let source = "fn f(secret: &Secret) {\n    let _text = secret.expose();\n    let _bytes = SecretBytes::expose(&secret);\n}\n";
        let violations = run(&[("crates/usage/jackin-usage/src/credential.rs", source)])?;
        assert_eq!(summary(&violations), []);
        Ok(())
    }

    #[test]
    fn expose_in_strings_and_definitions_is_not_a_call() -> TestResult {
        let source = "impl Secret {\n    pub fn expose(&self) -> &str { &self.0 }\n}\nfn docs() {\n    let a = \"secret.expose()\";\n    let b = r#\"secret.expose()\"#;\n    let expose = 1;\n}\n";
        let violations = run(&[("crates/usage/jackin-usage/src/model.rs", source)])?;
        assert_eq!(summary(&violations), []);
        Ok(())
    }

    #[test]
    fn expose_calls_under_a_tests_directory_pass() -> TestResult {
        let source = "fn f(secret: &Secret) {\n    let _text = secret.expose();\n}\n";
        let violations = run(&[("crates/usage/jackin-usage/tests/usage/secrets.rs", source)])?;
        assert_eq!(summary(&violations), []);
        Ok(())
    }
}

mod containers {
    use super::*;

    const MEMBERS: [&str; 6] = [
        "jackin-core",
        "jackin-image",
        "jackin-session",
        "jackin-testkit",
        "jackin-tui",
        "jackin",
    ];

    fn run(files: &[(&str, &str)]) -> Result<Vec<Violation>, Box<dyn Error>> {
        let fixture = Fixture::new(&bare(&MEMBERS))?;
        for (path, text) in files {
            fixture.write(path, text)?;
        }
        Ok(ContainersCheck.run(&fixture.repo()?))
    }

    #[test]
    fn permitted_docker_use_and_look_alikes_pass() -> TestResult {
        let engine = text(&[
            "use jackin_core::{DockerSubcommand, Program};",
            "use jackin_core::Program as P;",
            "use jackin_core::Program::Docker;",
            "use jackin_core::Program::*;",
            "type P2 = Program;",
            "fn specs(build: &Build, args: Vec<String>) {",
            "    let a = ProcessSpec { program: Program::Docker(DockerSubcommand::BuildxBuild), args: build.args.clone() };",
            "    let b = ProcessSpec { program: Program::Docker(DockerSubcommand::BuildxCreate), args };",
            "    let c = ProcessSpec::new(Program::Docker(DockerSubcommand::Push), args);",
            "    let d = ProcessSpec { program: P::Docker(DockerSubcommand::ImageInspect), args: vec![\"--tag\".to_owned(), \"run\".to_owned()] };",
            "}",
        ]);
        let program = text(&[
            "pub enum Program { Docker(DockerSubcommand), Git }",
            "pub enum DockerSubcommand { Push }",
            "pub type Alias = Program;",
            "impl Program {",
            "    fn binary(self) -> &'static str {",
            "        match self { Self::Docker(_) => \"docker\", Self::Git => \"git\" }",
            "    }",
            "}",
            "fn wrap(sub: DockerSubcommand) -> Program { Program::Docker(sub) }",
        ]);
        let testkit = text(&[
            "use jackin_core::{DockerSubcommand, Program};",
            "fn fake(spec: &ProcessSpec) -> u8 {",
            "    match spec.program { Program::Docker(DockerSubcommand::Push) => 1, Program::Docker(_) => 2, _ => 0 }",
            "}",
        ]);
        let host = text(&[
            "use jackin_core::Program;",
            "use jackin_core::Program::{Git, Ssh};",
            "use jackin_core::ProgramRun as Run;",
            "use jackin_testkit::ScriptedProgram;",
            "use std::process::Command as Cmd;",
            "use clap::Command as Cli;",
            "type Run = ProgramRun;",
            "fn host(name: &str) {",
            "    let a = ProcessSpec { program: Program::Openshell, args: vec![\"sandbox\".to_owned(), \"run\".to_owned()] };",
            "    let b = ProcessSpec { program: Program::Git, args: vec![\"push\".to_owned()] };",
            "    let c = ProcessSpec { program: Program::Ssh, args: vec![\"run\".to_owned()] };",
            "    let d = ProcessSpec { program: Program::Op, args: vec![\"read\".to_owned()] };",
            "    let e = Command::new(\"git\");",
            "    let f = Command::new(\"jackin\");",
            "    let i = Cmd::new(\"git\");",
            "    let j = Cli::new(name);",
            "    let g = ScriptedProgram::Docker;",
            "    let h = MyDockerSubcommand::default();",
            "    tracing::info!(\"docker ready, container stopped, podman absent\");",
            "    let container = 1;",
            "    let s = \"Command::new(\\\"docker\\\")\";",
            "    let t = r#\"Program::Docker(DockerSubcommand::Push) use jackin_core::Program as P;\"#;",
            "}",
            "fn docker_args() {}",
        ]);
        let violations = run(&[
            ("crates/jackin-image/src/engine.rs", &engine),
            ("crates/jackin-core/src/program.rs", &program),
            ("crates/jackin-testkit/src/host.rs", &testkit),
            ("crates/jackin-session/src/host.rs", &host),
        ])?;
        assert_eq!(summary(&violations), []);
        Ok(())
    }

    #[test]
    fn docker_program_and_subcommand_type_outside_the_allowed_crates_are_reported() -> TestResult {
        let session = text(&[
            "fn f(sub: Sub, args: Vec<String>) {",
            "    let spec = ProcessSpec { program: Program::Docker(sub), args };",
            "    let q = jackin_core::Program::Docker(sub);",
            "}",
            "fn typed(sub: DockerSubcommand) {}",
            "use jackin_core::DockerSubcommand;",
        ]);
        let tui = text(&[
            "fn m(s: Spec) -> u8 {",
            "    match s.program {",
            "        Program::Docker(_) => 1,",
            "        _ => 0,",
            "    }",
            "}",
        ]);
        let binary = text(&[
            "fn g(s: Spec) {",
            "    if let Program::Docker(sub) = s.program {}",
            "}",
        ]);
        let violations = run(&[
            ("crates/jackin-session/src/a.rs", &session),
            ("crates/jackin-tui/src/b.rs", &tui),
            ("crates/jackin/src/c.rs", &binary),
        ])?;
        assert_eq!(
            summary(&violations),
            [
                at(CiStep::Containers, "crates/jackin-session/src/a.rs", 2),
                at(CiStep::Containers, "crates/jackin-session/src/a.rs", 3),
                at(CiStep::Containers, "crates/jackin-session/src/a.rs", 5),
                at(CiStep::Containers, "crates/jackin-session/src/a.rs", 6),
                at(CiStep::Containers, "crates/jackin-tui/src/b.rs", 3),
                at(CiStep::Containers, "crates/jackin/src/c.rs", 2),
            ]
        );
        assert_messages(
            &violations,
            &[
                &["Program::Docker", "jackin-image"],
                &["Program::Docker"],
                &["DockerSubcommand"],
                &["DockerSubcommand"],
                &["Program::Docker"],
                &["Program::Docker"],
            ],
        );
        Ok(())
    }

    #[test]
    fn aliases_and_globs_that_hide_program_docker_are_reported() -> TestResult {
        let gateway = text(&[
            "use jackin_core::Program as P;",
            "use jackin_core::Program::Docker;",
            "use jackin_core::Program::*;",
            "use jackin_core::Program::{Docker as D, Git};",
            "use jackin_core::{Program as Q, Secret};",
            "use jackin_core::Program;",
            "use jackin_core::Program::{Git, Ssh};",
            "use jackin_core::ProgramRun as Run;",
            "use jackin_testkit::ScriptedProgram::*;",
        ]);
        let violations = run(&[("crates/jackin-session/src/gw.rs", &gateway)])?;
        assert_eq!(
            summary(&violations),
            (1..=5)
                .map(|line| at(CiStep::Containers, "crates/jackin-session/src/gw.rs", line))
                .collect::<Vec<_>>()
        );
        Ok(())
    }

    #[test]
    fn docker_started_outside_jackin_image_is_reported() -> TestResult {
        let core = "fn b() {\n    let c = std::process::Command::new(\"docker\");\n}\n";
        let binary = "fn c() {\n    let a = tokio::process::Command::new(\"docker\");\n    let b = Command::new(\n        \"docker\",\n    );\n    let c = Command::new(\"/usr/local/bin/docker\");\n}\n";
        let image = "fn d() {\n    let d = Command::new(\"docker\");\n}\n";
        let violations = run(&[
            ("crates/jackin-core/src/b.rs", core),
            ("crates/jackin/src/c.rs", binary),
            ("crates/jackin-image/src/d.rs", image),
        ])?;
        let mut expected = lines(
            CiStep::Containers,
            "crates/jackin-core/src/b.rs",
            core,
            &["let c"],
        );
        expected.extend(lines(
            CiStep::Containers,
            "crates/jackin-image/src/d.rs",
            image,
            &["let d"],
        ));
        expected.extend(lines(
            CiStep::Containers,
            "crates/jackin/src/c.rs",
            binary,
            &["let a", "let b", "let c"],
        ));
        assert_eq!(summary(&violations), expected);
        Ok(())
    }

    #[test]
    fn podman_and_container_are_reported_in_every_crate() -> TestResult {
        let session = "fn a() {\n    let p = Command::new(\"podman\");\n}\n";
        let paths = "fn s() {\n    let p = Command::new(\"/usr/local/bin/container\");\n    let q = Command::new(\"bin/podman\");\n}\n";
        let core = "fn b() {\n    let c = Command::new(\"container\");\n}\n";
        let image = "fn c() {\n    let d = std::process::Command::new(\"/opt/bin/podman\");\n}\n";
        let binary = "fn d() {\n    let e = Command::new(\n        \"container\",\n    );\n}\n";
        let violations = run(&[
            ("crates/jackin-session/src/a.rs", session),
            ("crates/jackin-session/src/b.rs", paths),
            ("crates/jackin-core/src/b.rs", core),
            ("crates/jackin-image/src/c.rs", image),
            ("crates/jackin/src/d.rs", binary),
        ])?;
        let mut expected = lines(
            CiStep::Containers,
            "crates/jackin-core/src/b.rs",
            core,
            &["let c"],
        );
        expected.extend(lines(
            CiStep::Containers,
            "crates/jackin-image/src/c.rs",
            image,
            &["let d"],
        ));
        expected.extend(lines(
            CiStep::Containers,
            "crates/jackin-session/src/a.rs",
            session,
            &["let p"],
        ));
        expected.extend(lines(
            CiStep::Containers,
            "crates/jackin-session/src/b.rs",
            paths,
            &["let p", "let q"],
        ));
        expected.extend(lines(
            CiStep::Containers,
            "crates/jackin/src/d.rs",
            binary,
            &["let e"],
        ));
        assert_eq!(summary(&violations), expected);
        Ok(())
    }

    #[test]
    fn command_new_without_a_single_string_literal_is_reported() -> TestResult {
        let session = text(&[
            "const D: &str = \"docker\";",
            "fn f(name: &str) {",
            "    let a = Command::new(D);",
            "    let b = std::process::Command::new(String::from(\"podman\"));",
            "    let c = tokio::process::Command::new(&\"podman\");",
            "    let d = Command::new(name);",
            "    let e = Command::new(",
            "        name,",
            "    );",
            "    let ok1 = Command::new(\"git\");",
            "    let ok2 = Command::new(r#\"jackin\"#);",
            "    let ok3 = Command::new(",
            "        \"git\",",
            "    );",
            "}",
        ]);
        let image = "fn g(cfg: Cfg) {\n    let x = Command::new(cfg.bin);\n}\n";
        let core = "fn h(p: P) {\n    let y = std::process::Command::new(p);\n}\n";
        let violations = run(&[
            ("crates/jackin-session/src/a.rs", &session),
            ("crates/jackin-image/src/b.rs", image),
            ("crates/jackin-core/src/c.rs", core),
        ])?;
        let mut expected = vec![at(CiStep::Containers, "crates/jackin-core/src/c.rs", 2)];
        expected.push(at(CiStep::Containers, "crates/jackin-image/src/b.rs", 2));
        expected.extend(
            [3, 4, 5, 6, 7]
                .map(|line| at(CiStep::Containers, "crates/jackin-session/src/a.rs", line)),
        );
        assert_eq!(summary(&violations), expected);
        Ok(())
    }

    #[test]
    fn non_literal_command_new_is_allowed_in_xtask_the_session_host_file_and_the_jackin_process_main_file_only()
    -> TestResult {
        let tool = text(&[
            "fn a() {",
            "    let x = Command::new(cargo_program());",
            "    let y = Command::new(&program);",
            "    let bad1 = Command::new(\"podman\");",
            "    let bad2 = Command::new(\"/usr/bin/docker\");",
            "}",
        ]);
        let tests =
            "fn r() {\n    let c = Command::new(env!(\"CARGO_BIN_EXE_jackin-xtask\"));\n}\n";
        let session = "fn y(program: P) {\n    let z = Command::new(program);\n}\n";
        let host = text(&[
            "fn c(spec: &ProcessSpec) {",
            "    let ok = Command::new(spec.program.binary());",
            "    let bad1 = Command::new(\"docker\");",
            "    let bad2 = Command::new(\"podman\");",
            "    let value = Command::new;",
            "}",
        ]);
        let violations = run(&[
            ("crates/xtask/src/tool.rs", &tool),
            ("crates/xtask/tests/run.rs", tests),
            ("crates/jackin-session/src/y.rs", session),
            ("crates/jackin-session/src/host.rs", &host),
            ("crates/jackin-session/src/launch.rs", session),
            ("crates/jackin/tests/process/main.rs", tests),
            ("crates/jackin/tests/process/other.rs", tests),
            ("crates/jackin/tests/cli/main.rs", tests),
        ])?;
        assert_eq!(
            summary(&violations),
            [
                at(CiStep::Containers, "crates/jackin-session/src/host.rs", 3),
                at(CiStep::Containers, "crates/jackin-session/src/host.rs", 4),
                at(CiStep::Containers, "crates/jackin-session/src/launch.rs", 2),
                at(CiStep::Containers, "crates/jackin-session/src/y.rs", 2),
                at(CiStep::Containers, "crates/jackin/tests/cli/main.rs", 2),
                at(
                    CiStep::Containers,
                    "crates/jackin/tests/process/other.rs",
                    2
                ),
                at(CiStep::Containers, "crates/xtask/src/tool.rs", 4),
                at(CiStep::Containers, "crates/xtask/src/tool.rs", 5),
            ]
        );
        Ok(())
    }

    #[test]
    fn command_aliases_are_checked_like_command_new() -> TestResult {
        let session = text(&[
            "use std::process::Command as Cmd;",
            "use tokio::process::{Command as Tcmd, Stdio};",
            "use std::{process::{Command as Nested}};",
            "use std::process::Command as _;",
            "fn f(name: &str) {",
            "    let a = Cmd::new(\"docker\");",
            "    let b = Tcmd::new(\"podman\");",
            "    let c = Nested::new(name);",
            "    let d = Cmd::new(\"git\");",
            "    let e = Tcmd::new(r#\"jackin\"#);",
            "    let g = Other::new(name);",
            "}",
        ]);
        let image = text(&[
            "use std::process::Command as C;",
            "fn g(cfg: Cfg) {",
            "    let a = C::new(\"docker\");",
            "    let b = C::new(cfg.bin);",
            "    let c = C::new(\"podman\");",
            "}",
        ]);
        let tool = text(&[
            "use std::process::Command as C;",
            "fn h(program: P) {",
            "    let a = C::new(program);",
            "    let b = C::new(\"podman\");",
            "}",
        ]);
        let violations = run(&[
            ("crates/jackin-session/src/a.rs", &session),
            ("crates/jackin-image/src/b.rs", &image),
            ("crates/xtask/src/c.rs", &tool),
        ])?;
        assert_eq!(
            summary(&violations),
            [
                at(CiStep::Containers, "crates/jackin-image/src/b.rs", 3),
                at(CiStep::Containers, "crates/jackin-image/src/b.rs", 4),
                at(CiStep::Containers, "crates/jackin-image/src/b.rs", 5),
                at(CiStep::Containers, "crates/jackin-session/src/a.rs", 6),
                at(CiStep::Containers, "crates/jackin-session/src/a.rs", 7),
                at(CiStep::Containers, "crates/jackin-session/src/a.rs", 8),
                at(CiStep::Containers, "crates/xtask/src/c.rs", 4),
            ]
        );
        assert_messages(
            &violations,
            &[
                &["docker"],
                &["program"],
                &["podman"],
                &["docker"],
                &["podman"],
                &["program"],
                &["podman"],
            ],
        );
        Ok(())
    }

    #[test]
    fn type_aliases_of_program_are_reported_outside_the_allowed_crates() -> TestResult {
        let session = text(&[
            "type P = Program;",
            "pub type Q = jackin_core::Program;",
            "pub(crate) type R = crate::Program;",
            "type Run = ProgramRun;",
            "type Lists = Vec<ProgramSpec>;",
            "type S = DockerSubcommand;",
            "let s = \"type P = Program;\";",
        ]);
        let alias = "type X = Program;\n";
        let allowed = "type P = Program;\n";
        let violations = run(&[
            ("crates/jackin-session/src/t.rs", &session),
            ("crates/jackin-tui/src/x.rs", alias),
            ("crates/jackin-image/src/t.rs", allowed),
            ("crates/jackin-core/src/t.rs", allowed),
            ("crates/jackin-testkit/src/t.rs", allowed),
        ])?;
        assert_eq!(
            summary(&violations),
            [
                at(CiStep::Containers, "crates/jackin-session/src/t.rs", 1),
                at(CiStep::Containers, "crates/jackin-session/src/t.rs", 2),
                at(CiStep::Containers, "crates/jackin-session/src/t.rs", 3),
                at(CiStep::Containers, "crates/jackin-session/src/t.rs", 6),
                at(CiStep::Containers, "crates/jackin-tui/src/x.rs", 1),
            ]
        );
        assert_messages(
            &violations,
            &[
                &["Program", "jackin-image"],
                &["Program", "jackin-image"],
                &["Program", "jackin-image"],
                &["DockerSubcommand"],
                &["Program", "jackin-image"],
            ],
        );
        Ok(())
    }

    #[test]
    fn command_new_used_as_a_value_is_reported() -> TestResult {
        let session = text(&[
            "use std::process::Command as Cmd;",
            "fn f(names: Vec<&str>) {",
            "    let a = Command::new;",
            "    let b = std::process::Command::new;",
            "    let c = names.iter().map(Command::new).count();",
            "    let d = [tokio::process::Command::new];",
            "    let e = Cmd::new;",
            "    let ok = Command::new(\"git\");",
            "    let s = \"Command::new\";",
            "    let n = Command::new_in(\"x\");",
            "}",
        ]);
        let value = "fn f() {\n    let f = Command::new;\n}\n";
        let violations = run(&[
            ("crates/jackin-session/src/v.rs", &session),
            ("crates/jackin-image/src/v.rs", value),
            ("crates/jackin-core/src/v.rs", value),
            ("crates/xtask/src/v.rs", value),
        ])?;
        assert_eq!(
            summary(&violations),
            [
                at(CiStep::Containers, "crates/jackin-core/src/v.rs", 2),
                at(CiStep::Containers, "crates/jackin-image/src/v.rs", 2),
                at(CiStep::Containers, "crates/jackin-session/src/v.rs", 3),
                at(CiStep::Containers, "crates/jackin-session/src/v.rs", 4),
                at(CiStep::Containers, "crates/jackin-session/src/v.rs", 5),
                at(CiStep::Containers, "crates/jackin-session/src/v.rs", 6),
                at(CiStep::Containers, "crates/jackin-session/src/v.rs", 7),
            ]
        );
        let needle: &[&str] = &["Command::new"];
        assert_messages(&violations, &[needle; 7]);
        Ok(())
    }
}

mod errors {
    use super::*;

    const LIB: &str = "crates/jackin-core/src/lib.rs";

    fn run(files: &[(&str, &str)]) -> Result<Vec<Violation>, Box<dyn Error>> {
        let fixture = Fixture::new(&bare(&["jackin-core", "jackin-testkit", "jackin"]))?;
        for (path, text) in files {
            fixture.write(path, text)?;
        }
        Ok(ErrorsCheck.run(&fixture.repo()?))
    }

    fn message_at(violations: &[Violation], line: u32) -> &str {
        violations
            .iter()
            .find(|violation| violation.line == Some(line))
            .map_or("", |violation| violation.message.as_str())
    }

    #[test]
    fn debug_placeholder_forms_are_reported_at_the_literal_line() -> TestResult {
        let source = text(&[
            "enum E {",
            "    #[error(\"named {what:?}\")]",
            "    A,",
            "    #[error(\"positional {0:?}\")]",
            "    B(Id),",
            "    #[error(\"bare {:?}\", .0)]",
            "    C(Id),",
            "    #[error(\"alt {x:#?}\")]",
            "    D,",
            "    #[error(\"lower {x:x?}\")]",
            "    E,",
            "    #[error(\"upper {x:X?}\")]",
            "    F,",
            "    #[error(\"wide {x:>8?}\")]",
            "    G,",
            "    #[error(\"middle {a} {b:?} {c}\")]",
            "    H,",
            "    #[error(",
            "        \"multi {name:?} line\"",
            "    )]",
            "    I,",
            "    #[error(r#\"raw {y:?}\"#)]",
            "    J,",
            "    #[error(\"{}\", format!(\"args {x:?}\"))]",
            "    K,",
            "    #[error(\"{{{x:?}}} around\")]",
            "    L,",
            "    #[error(\"{{}}{x:?} after\")]",
            "    M,",
            "    #[error(\"pair {a:?} and {b:?}\")]",
            "    N,",
            "}",
            "#[error(\"tail {0:?}\")]",
            "struct S(Id);",
        ]);
        let violations = run(&[(LIB, &source)])?;
        let tags = [
            "named",
            "positional",
            "bare",
            "alt",
            "lower",
            "upper",
            "wide",
            "middle",
            "multi",
            "raw",
            "args",
            "around",
            "after",
            "pair",
            "tail",
        ];
        assert_eq!(
            summary(&violations),
            lines(CiStep::Errors, LIB, &source, &tags)
        );
        let middle = message_at(&violations, line_of(&source, "middle"));
        assert!(middle.contains("{b:?}"), "{middle}");
        assert!(
            !middle.contains("{a}") && !middle.contains("{c}"),
            "{middle}"
        );
        Ok(())
    }

    #[test]
    fn the_scan_covers_every_rust_file_under_crates() -> TestResult {
        let defect = "#[error(\"{0:?}\")]\nstruct S(Id);\n";
        let paths = [
            "crates/jackin-core/tests/b.rs",
            "crates/jackin-testkit/src/lib.rs",
            "crates/jackin/tests/cli/a.rs",
            "crates/xtask/src/ci.rs",
        ];
        let files: Vec<(&str, &str)> = paths.iter().map(|path| (*path, defect)).collect();
        let expected: Vec<Found> = paths
            .iter()
            .map(|path| at(CiStep::Errors, path, 1))
            .collect();
        assert_eq!(summary(&run(&files)?), expected);
        Ok(())
    }

    #[test]
    fn display_placeholders_and_debug_outside_error_attributes_are_not_reported() -> TestResult {
        let source = text(&[
            "enum E {",
            "    #[error(\"{x} {0} {} {x:>8} {x:.3} {x:#x} {x:x}\")]",
            "    A,",
            "    #[error(\"{{x:?}}\")]",
            "    B,",
            "    #[error(\"fill {x:?>8} {x:?<4} {x:?^5}\")]",
            "    C,",
            "    #[error(transparent)]",
            "    D(Inner),",
            "    #[error(\"no placeholders\")]",
            "    E,",
            "    #[error(\"{} then {}\", list(.0), found_text(.found.as_ref()))]",
            "    F(Vec<String>),",
            "}",
            "#[derive(Debug)]",
            "#[doc = \"{x:?}\"]",
            "#[serde(rename = \"{x:?}\")]",
            "struct T;",
            "fn f() {",
            "    println!(\"{x:?}\");",
            "    let _ = format!(\"{:?}\", y);",
            "    assert!(ok, \"{e:?}\");",
            "    tracing::debug!(\"{x:?}\");",
            "    a.expect(\"{x:?}\");",
            "}",
            "const S: &str = \"#[error(\\\"{x:?}\\\")]\";",
            "const R: &str = r#\"#[error(\"{x:?}\")]\"#;",
            "// #[error(\"{x:?}\")]",
        ]);
        let violations = run(&[
            (LIB, &source),
            ("crates/jackin-core/notes.toml", "#[error(\"{x:?}\")]\n"),
            (
                "crates/jackin-core/run.sh",
                "#!/bin/sh\n#[error(\"{x:?}\")]\n",
            ),
        ])?;
        assert_eq!(summary(&violations), []);
        Ok(())
    }
}

mod error_source {
    use super::*;

    const LIB: &str = "crates/jackin-core/src/lib.rs";

    fn run(source: &str) -> Result<Vec<Violation>, Box<dyn Error>> {
        let fixture = Fixture::new(&bare(&["jackin-core"]))?;
        fixture.write(LIB, source)?;
        Ok(ErrorsCheck.run(&fixture.repo()?))
    }

    #[test]
    fn a_source_placeholder_in_an_error_message_is_reported_at_the_literal_line() -> TestResult {
        let source = text(&[
            "enum E {",
            "    #[error(\"manifest at {path}: {source}\")]",
            "    A { path: Id, source: M },",
            "    #[error(\"alt {source:#}\")]",
            "    B { source: M },",
            "    #[error(\"wide {source:>8}\")]",
            "    C { source: M },",
            "    #[error(",
            "        \"multi {source} line\"",
            "    )]",
            "    D { source: M },",
            "    #[error(r#\"raw {source}\"#)]",
            "    E { source: M },",
            "    #[error(\"argform: {}\", .source)]",
            "    F { source: M },",
            "    #[error(\"both {path:?} and {source}\")]",
            "    G { path: Id, source: M },",
            "}",
            "#[error(\"tail {source}\")]",
            "struct S { source: M }",
        ]);
        let violations = run(&source)?;
        let tags = [
            "manifest", "alt", "wide", "multi", "raw", "argform", "both", "tail",
        ];
        assert_eq!(
            summary(&violations),
            lines(CiStep::Errors, LIB, &source, &tags)
        );
        let both = &violations[6].message;
        assert!(
            both.contains("{path:?}") && both.contains("{source}"),
            "{both}"
        );
        assert!(violations[0].message.contains("{source}"));
        Ok(())
    }

    #[test]
    fn placeholders_that_only_resemble_source_and_source_outside_error_attributes_pass()
    -> TestResult {
        let source = text(&[
            "enum E {",
            "    #[error(\"cannot discover gateway {gateway}\")]",
            "    Discovery { gateway: G, source: D },",
            "    #[error(\"names {source_path} {sources} {resource} {source_moved} {error}\")]",
            "    A,",
            "    #[error(\"arg {}\", .source_path)]",
            "    B,",
            "    #[error(\"escaped {{source}}\")]",
            "    C,",
            "    #[error(transparent)]",
            "    D(Inner),",
            "    #[error(\"x\")]",
            "    F { source: M },",
            "}",
            "impl fmt::Display for T {",
            "    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {",
            "        write!(f, \"{source}\")",
            "    }",
            "}",
            "fn f() {",
            "    let _ = format!(\"{source}\");",
            "    println!(\"{source}\");",
            "    let _ = run().map_err(|source| wrap(source));",
            "}",
            "const S: &str = \"#[error(\\\"{source}\\\")]\";",
            "// #[error(\"{source}\")]",
        ]);
        assert_eq!(summary(&run(&source)?), []);
        Ok(())
    }

    #[test]
    fn a_placeholder_naming_a_source_or_from_field_of_its_own_variant_is_reported() -> TestResult {
        let source = text(&[
            "enum E {",
            "    #[error(\"launch failed t_named {error}\")]",
            "    A { id: Id, #[source] error: Box<M> },",
            "    #[error(\"wrapped t_from {0}\")]",
            "    C(#[from] M),",
            "    #[error(\"arg t_form {}\", .0)]",
            "    D(#[from] M),",
            "    #[error(\"second t_index {1}\")]",
            "    F(HashMap<String, u8>, #[source] M),",
            "    #[error(\"cause t_generic {cause}\")]",
            "    I { map: HashMap<String, u8>, #[source] cause: Box<dyn Error + Send + Sync> },",
            "    #[error(\"launch {id} t_wrapped {}\", .error)]",
            "    J { id: Id, #[source] error: Box<M> },",
            "    #[error(\"x t_nested {} {}\", list(a), .0)]",
            "    L(#[source] M),",
            "    #[error(\"t_attr {0}\")]",
            "    #[cfg(unix)]",
            "    N(#[from] M),",
            "}",
            "#[derive(Debug, Error)]",
            "#[error(\"launch {launch_id} failed t_struct: {error}\")]",
            "pub struct S { pub launch_id: L, #[source] pub error: Box<M> }",
            "#[error(\"tuple t_tuple {0}\")]",
            "pub struct T(#[from] M);",
            "#[error(\"t_gstruct {e}\")]",
            "pub struct G<T> { #[source] e: T }",
        ]);
        let violations = run(&source)?;
        let tags = [
            "t_named",
            "t_from",
            "t_form",
            "t_index",
            "t_generic",
            "t_wrapped",
            "t_nested",
            "t_attr",
            "t_struct",
            "t_tuple",
            "t_gstruct",
        ];
        assert_eq!(
            summary(&violations),
            lines(CiStep::Errors, LIB, &source, &tags)
        );
        let named = &violations[0].message;
        assert!(named.contains("{error}"), "{named}");
        Ok(())
    }

    #[test]
    fn a_source_field_that_the_message_does_not_print_and_unmarked_fields_pass() -> TestResult {
        let source = text(&[
            "enum E {",
            "    #[error(\"launch failed {id}\")]",
            "    A { id: Id, #[source] error: Box<M> },",
            "    #[error(\"plain {error}\")]",
            "    B { error: Id },",
            "    #[error(\"first {0}\")]",
            "    C(Id, #[source] M),",
            "    #[error(\"nested {}\", .0.error)]",
            "    D(Wrapper),",
            "    #[error(\"other variant {error}\")]",
            "    F { error: Id },",
            "    #[error(\"has {n}\")]",
            "    G { n: usize, #[from] inner: M },",
            "    #[error(\"member {}\", .inner.error)]",
            "    K { inner: Wrapper, #[source] error: M },",
            "    #[error(\"n {}\", .0.source)]",
            "    L(Wrapper),",
            "    #[error(\"c {}\", f(a).source)]",
            "    M2 { source: M },",
            "    #[error(\"i {}\", a[0].source)]",
            "    N2 { source: M },",
            "    #[error(\"s {}\", \"text\".source)]",
            "    O { source: M },",
            "}",
            "#[error(\"launch {launch_id} failed\")]",
            "pub struct S { pub launch_id: L, #[source] pub error: Box<M> }",
            "#[error(\"tuple {0}\")]",
            "pub struct T(Id, #[from] M);",
        ]);
        assert_eq!(summary(&run(&source)?), []);
        Ok(())
    }
}

mod error_strings {
    use super::*;

    const LIB: &str = "crates/jackin-core/src/lib.rs";

    fn run(source: &str) -> Result<Vec<Violation>, Box<dyn Error>> {
        let fixture = Fixture::new(&bare(&["jackin-core"]))?;
        fixture.write(LIB, source)?;
        Ok(ErrorsCheck.run(&fixture.repo()?))
    }

    #[test]
    fn a_bare_placeholder_of_a_string_field_is_reported_at_the_literal_line() -> TestResult {
        let source = text(&[
            "enum E {",
            "    #[error(\"named s_named {message}\")]",
            "    A { message: String },",
            "    #[error(\"tuple s_tuple {0}\")]",
            "    B(String),",
            "    #[error(\"spec s_spec {message:>8}\")]",
            "    C { message: String },",
            "    #[error(\"arg s_arg {}\", .message)]",
            "    D { message: String },",
            "    #[error(\"pub s_pub {message}\")]",
            "    F { id: u8, pub message: String },",
            "    #[error(\"two s_two {first} {second} {id}\")]",
            "    G { id: u8, first: String, #[source] second: String },",
            "    #[error(\"after s_after {1}\")]",
            "    H(HashMap<String, u8>, String),",
            "}",
            "#[error(\"struct s_struct {text}\")]",
            "pub struct S<T> { text: String, other: T }",
        ]);
        let violations = run(&source)?;
        let tags = [
            "s_named", "s_tuple", "s_spec", "s_arg", "s_pub", "s_two", "s_after", "s_struct",
        ];
        assert_eq!(
            summary(&violations),
            lines(CiStep::Errors, LIB, &source, &tags)
        );
        let named = &violations[0].message;
        assert!(
            named.contains("{message}") && named.contains("plain"),
            "{named}"
        );
        Ok(())
    }

    #[test]
    fn string_fields_passed_through_plain_and_other_types_pass() -> TestResult {
        let source = text(&[
            "enum E {",
            "    #[error(\"wrapped {}\", plain(.message))]",
            "    A { message: String },",
            "    #[error(\"list {}\", plain_list(.items))]",
            "    B { items: Vec<String> },",
            "    #[error(\"count {count}\")]",
            "    C { count: usize },",
            "    #[error(\"kind {kind}\")]",
            "    D { kind: &'static str },",
            "    #[error(\"tail {tail}\")]",
            "    E { tail: TerminalText },",
            "    #[error(\"path {path}\")]",
            "    F { path: HostPath },",
            "    #[error(\"not printed\")]",
            "    G { message: String },",
            "    #[error(\"other variant {message}\")]",
            "    H { message: HostPath },",
            "    #[error(\"escaped {{message}}\")]",
            "    I { message: String },",
            "    #[error(\"tuple {}\", plain(.0))]",
            "    J(String),",
            "    #[error(\"first {0}\")]",
            "    K(HostPath, String),",
            "    #[error(\"opt {}\", scheme_text(.scheme.as_deref()))]",
            "    L { scheme: Option<String> },",
            "    #[error(\"call {}\", pair(a, .message))]",
            "    P { message: String },",
            "    #[error(\"call {}\", wrap(.text, a))]",
            "    Q { text: String },",
            "    #[error(\"wrapped {wrapped} {items}\")]",
            "    N { wrapped: Wrapper<String>, items: Vec<String> },",
            "    #[error(\"map {name}\")]",
            "    M { name: Name, other: String },",
            "    #[error(\"x {messages}\")]",
            "    R { message: String, messages: HostPath },",
            "}",
            "#[error(\"struct {}\", plain(.text))]",
            "pub struct S { text: String }",
        ]);
        assert_eq!(summary(&run(&source)?), []);
        Ok(())
    }
}

mod repo {
    use super::*;

    #[test]
    fn opening_from_a_subdirectory_checks_the_whole_workspace() -> TestResult {
        let fixture = Fixture::new(&d14())?;
        fixture.write("crates/jackin-session/src/a.rs", "fn f() {} // note\n")?;
        fixture.write("crates/jackin-session/src/gw.rs", "use tonic::Status;\n")?;
        fixture.write(
            "crates/jackin/src/c.rs",
            "fn g() {\n    let _ = Command::new(\"podman\");\n}\n",
        )?;
        fixture.write(
            "crates/jackin-core/src/e.rs",
            "\n#[error(\"{x:?}\")]\nstruct S;\n",
        )?;
        fixture.write("docs/index.md", "# Docs\n")?;
        fixture.prepare()?;
        let root = fs::canonicalize(fixture.dir.path())?;
        for at_path in [".", "crates/jackin-core", "crates/jackin-core/src", "docs"] {
            let repo = fixture.open_at(at_path)?;
            assert_eq!(repo.root(), root.as_path(), "root from {at_path}");
            assert_eq!(
                summary(&CommentsCheck.run(&repo)),
                [at(CiStep::Comments, "crates/jackin-session/src/a.rs", 1)],
                "comments from {at_path}"
            );
            assert_eq!(
                summary(&LayersCheck.run(&repo)),
                [at(CiStep::Layers, "crates/jackin-session/src/gw.rs", 1)],
                "layers from {at_path}"
            );
            assert_eq!(
                summary(&ContainersCheck.run(&repo)),
                [at(CiStep::Containers, "crates/jackin/src/c.rs", 2)],
                "containers from {at_path}"
            );
            assert_eq!(
                summary(&ErrorsCheck.run(&repo)),
                [at(CiStep::Errors, "crates/jackin-core/src/e.rs", 2)],
                "errors from {at_path}"
            );
        }
        Ok(())
    }
}

mod pins {
    use super::*;

    fn run(pins: [&str; 5]) -> Result<(Vec<Violation>, String), Box<dyn Error>> {
        let fixture = Fixture::new(&bare(&["jackin-core"]))?;
        fixture.write_pins(pins[0], pins[1], pins[2], pins[3], pins[4])?;
        let violations = PinsCheck.run(&fixture.repo()?);
        Ok((violations, mise_text(pins[2], pins[3], pins[4])))
    }

    #[test]
    fn agreeing_pins_pass() -> TestResult {
        assert_eq!(summary(&run([PIN; 5])?.0), []);
        Ok(())
    }

    #[test]
    fn each_disagreeing_file_is_reported_at_its_line() -> TestResult {
        let mise = mise_text(PIN, PIN, PIN);
        let tool = line_of(&mise, "rust = {");
        let fast = line_of(&mise, "--fast");
        let full = line_of(&mise, "--no-docs");
        let toolchain = || at(CiStep::Pins, "rust-toolchain.toml", 2);
        let in_mise = |line| at(CiStep::Pins, "mise.toml", line);

        let (violations, _) = run([OTHER, PIN, OTHER, OTHER, OTHER])?;
        assert_eq!(summary(&violations), [toolchain()]);
        assert_messages(&violations, &[&[PIN, OTHER]]);

        let (violations, _) = run([PIN, PIN, OTHER, PIN, PIN])?;
        assert_eq!(summary(&violations), [in_mise(tool)]);
        assert_messages(&violations, &[&[PIN, OTHER]]);

        let (violations, _) = run([PIN, PIN, PIN, PIN, OTHER])?;
        assert_eq!(summary(&violations), [in_mise(full)]);
        assert_messages(&violations, &[&[PIN, OTHER]]);

        let (violations, _) = run([OTHER, PIN, PIN, PIN, PIN])?;
        assert_eq!(
            summary(&violations),
            [in_mise(tool), in_mise(fast), in_mise(full), toolchain()]
        );
        assert_messages(
            &violations,
            &[&[PIN, OTHER], &[PIN, OTHER], &[PIN, OTHER], &[PIN, OTHER]],
        );
        Ok(())
    }

    #[test]
    fn a_missing_pin_is_reported_not_skipped() -> TestResult {
        let green = || -> Result<Fixture, Box<dyn Error>> {
            let fixture = Fixture::new(&bare(&["jackin-core"]))?;
            fixture.write_pins(PIN, PIN, PIN, PIN, PIN)?;
            Ok(fixture)
        };
        let file = |path: &str| (CiStep::Pins, path.to_owned(), None);

        let fixture = green()?;
        fixture.remove("rust-toolchain.toml")?;
        assert_eq!(
            summary(&PinsCheck.run(&fixture.repo()?)),
            [file("rust-toolchain.toml")]
        );

        let fixture = green()?;
        fixture.write("mise.toml", "[tools]\nbun = \"1.4.4\"\n")?;
        assert_eq!(
            summary(&PinsCheck.run(&fixture.repo()?)),
            [file("mise.toml")]
        );

        let fixture = green()?;
        let root = fs::read_to_string(fixture.dir.path().join("Cargo.toml"))?;
        fixture.write(
            "Cargo.toml",
            &root.replace(&format!("rust-version = \"{PIN}\"\n"), ""),
        )?;
        assert_eq!(
            summary(&PinsCheck.run(&fixture.repo()?)),
            [file("Cargo.toml")]
        );
        Ok(())
    }
}

#[cfg(unix)]
mod ci_run {
    use std::fs::File;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Stdio;

    use super::*;

    const FULL: [&str; 8] = [
        "cargo fmt --all --check",
        "cargo clippy --workspace --all-targets --all-features --locked -- -D warnings",
        "cargo nextest run --workspace --all-features --locked",
        "cargo deny --locked check",
        "cargo shear --deny-warnings",
        "reuse lint",
        "bun install --frozen-lockfile @docs",
        "bun run build @docs",
    ];

    const CARGO_SCRIPT: &str = "#!/bin/sh\nif [ \"$1\" = metadata ]; then exec \"$REAL_CARGO\" \"$@\"; fi\nprintf 'cargo %s\\n' \"$*\" >> \"$XTASK_LOG\"\nif [ \"$1\" = \"$FAIL_ON\" ]; then exit 7; fi\nexit 0\n";

    const REUSE_SCRIPT: &str = "#!/bin/sh\nprintf 'reuse %s\\n' \"$*\" >> \"$XTASK_LOG\"\nif [ \"$FAIL_ON\" = reuse ]; then exit 7; fi\nexit 0\n";

    const BUN_SCRIPT: &str = "#!/bin/sh\nprintf 'bun %s @%s\\n' \"$*\" \"$(basename \"$(pwd -P)\")\" >> \"$XTASK_LOG\"\nif [ \"$FAIL_ON\" = bun ]; then exit 7; fi\nexit 0\n";

    struct Outcome {
        code: Option<i32>,
        success: bool,
        log: Vec<String>,
        stderr: String,
    }

    struct Spy {
        dir: TempDir,
    }

    impl Spy {
        fn new() -> Result<Self, Box<dyn Error>> {
            let spy = Self {
                dir: tempfile::tempdir()?,
            };
            for (name, script) in [
                ("cargo", CARGO_SCRIPT),
                ("reuse", REUSE_SCRIPT),
                ("bun", BUN_SCRIPT),
            ] {
                let path = spy.dir.path().join(name);
                fs::write(&path, script)?;
                fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
            }
            Ok(spy)
        }

        fn run(
            &self,
            fixture: &Fixture,
            args: &[&str],
            fail_on: &str,
        ) -> Result<Outcome, Box<dyn Error>> {
            fixture.prepare()?;
            let log = self.dir.path().join("log");
            let stderr = self.dir.path().join("stderr");
            if log.exists() {
                fs::remove_file(&log)?;
            }
            let inherited = std::env::var_os("PATH").unwrap_or_default();
            let path = std::env::join_paths(
                std::iter::once(self.dir.path().to_path_buf())
                    .chain(std::env::split_paths(&inherited)),
            )?;
            let status = Command::new(env!("CARGO_BIN_EXE_jackin-xtask"))
                .arg("ci")
                .args(args)
                .current_dir(fixture.dir.path())
                .env("PATH", path)
                .env("CARGO", self.dir.path().join("cargo"))
                .env(
                    "REAL_CARGO",
                    std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()),
                )
                .env("XTASK_LOG", &log)
                .env("FAIL_ON", fail_on)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(File::create(&stderr)?)
                .status()?;
            Ok(Outcome {
                code: status.code(),
                success: status.success(),
                log: fs::read_to_string(&log)
                    .unwrap_or_default()
                    .lines()
                    .map(str::to_owned)
                    .collect(),
                stderr: fs::read_to_string(&stderr)?,
            })
        }
    }

    fn green() -> Result<Fixture, Box<dyn Error>> {
        let fixture = Fixture::new(&bare(&["jackin-core"]))?;
        fixture.write_pins(PIN, PIN, PIN, PIN, PIN)?;
        fixture.write("docs/index.md", "# Docs\n")?;
        Ok(fixture)
    }

    #[test]
    fn a_full_run_executes_each_tool_once_in_plan_order_and_exits_zero() -> TestResult {
        let outcome = Spy::new()?.run(&green()?, &[], "")?;
        assert_eq!(outcome.log, FULL);
        assert!(outcome.success, "{}", outcome.stderr);
        Ok(())
    }

    #[test]
    fn fast_and_no_docs_skip_only_the_docs_build() -> TestResult {
        let spy = Spy::new()?;
        let fixture = green()?;
        for args in [&["--fast"][..], &["--no-docs"], &["--fast", "--no-docs"]] {
            let outcome = spy.run(&fixture, args, "")?;
            assert_eq!(outcome.log, FULL[..6], "args {args:?}");
            assert!(outcome.success, "{}", outcome.stderr);
        }
        Ok(())
    }

    #[test]
    fn only_runs_the_named_steps_in_plan_order() -> TestResult {
        let spy = Spy::new()?;
        let fixture = green()?;
        let cases: [(&str, &[&str]); 4] = [
            ("reuse,fmt", &[FULL[0], FULL[5]]),
            ("docs", &FULL[6..]),
            ("deny,deny", &[FULL[3]]),
            ("comments,layers,pins,containers", &[]),
        ];
        for (selection, expected) in cases {
            let outcome = spy.run(&fixture, &["--only", selection], "")?;
            assert_eq!(outcome.log, expected, "--only {selection}");
            assert!(outcome.success, "{}", outcome.stderr);
        }
        Ok(())
    }

    #[test]
    fn a_failing_tool_stops_the_run_and_fails_the_exit_status() -> TestResult {
        let spy = Spy::new()?;
        let fixture = green()?;
        for (fail_on, count) in [("clippy", 2), ("deny", 4), ("reuse", 6), ("bun", 7)] {
            let outcome = spy.run(&fixture, &[], fail_on)?;
            assert_eq!(outcome.log, FULL[..count], "FAIL_ON {fail_on}");
            assert!(!outcome.success, "FAIL_ON {fail_on}");
            assert_eq!(outcome.code, Some(1), "FAIL_ON {fail_on}");
        }
        Ok(())
    }

    #[test]
    fn a_violation_in_a_repository_step_stops_before_the_later_tools() -> TestResult {
        let lib = "crates/jackin-core/src/lib.rs";
        let spy = Spy::new()?;
        let comments = green()?;
        comments.write(lib, "fn f() {} // note\n")?;
        let layers = green()?;
        layers.write(lib, "use tonic::Status;\n")?;
        let pins = green()?;
        pins.write_pins(OTHER, PIN, PIN, PIN, PIN)?;
        let containers = green()?;
        containers.write(
            lib,
            "fn g() { let _ = std::process::Command::new(\"podman\"); }\n",
        )?;
        let errors = green()?;
        errors.write(lib, "#[error(\"{x:?}\")]\nstruct S;\n")?;
        let cases = [
            (&comments, "crates/jackin-core/src/lib.rs:1"),
            (&layers, "crates/jackin-core/src/lib.rs:1"),
            (&pins, "rust-toolchain.toml"),
            (&containers, "crates/jackin-core/src/lib.rs:1"),
            (&errors, "crates/jackin-core/src/lib.rs:1"),
        ];
        for (fixture, needle) in cases {
            let outcome = spy.run(fixture, &[], "")?;
            assert_eq!(outcome.log, FULL[..5], "{needle}");
            assert_eq!(outcome.code, Some(1), "{needle}");
            assert!(outcome.stderr.contains(needle), "{}", outcome.stderr);
        }
        Ok(())
    }

    #[test]
    fn with_defects_in_every_repository_step_only_the_first_step_is_reported() -> TestResult {
        let fixture = green()?;
        fixture.write("crates/jackin-core/src/a.rs", "fn f() {} // note\n")?;
        fixture.write("crates/jackin-core/src/b.rs", "use tonic::Status;\n")?;
        fixture.write_pins(OTHER, PIN, PIN, PIN, PIN)?;
        fixture.write(
            "crates/jackin-core/src/d.rs",
            "fn g() { let _ = std::process::Command::new(\"podman\"); }\n",
        )?;
        fixture.write(
            "crates/jackin-core/src/e.rs",
            "#[error(\"{x:?}\")]\nstruct S;\n",
        )?;
        let spy = Spy::new()?;
        let cases: [(&str, &str, &[&str]); 6] = [
            (
                "containers,comments,pins,layers",
                "a.rs:1",
                &["b.rs", "d.rs", "e.rs", "rust-toolchain.toml"],
            ),
            (
                "layers,pins,containers",
                "b.rs:1",
                &["a.rs", "d.rs", "e.rs", "rust-toolchain.toml"],
            ),
            (
                "pins,containers",
                "rust-toolchain.toml",
                &["a.rs", "b.rs", "d.rs", "e.rs"],
            ),
            (
                "containers",
                "d.rs:1",
                &["a.rs", "b.rs", "e.rs", "rust-toolchain.toml"],
            ),
            (
                "containers,errors",
                "d.rs:1",
                &["a.rs", "b.rs", "e.rs", "rust-toolchain.toml"],
            ),
            (
                "errors",
                "e.rs:1",
                &["a.rs", "b.rs", "d.rs", "rust-toolchain.toml"],
            ),
        ];
        for (selection, present, absent) in cases {
            let outcome = spy.run(&fixture, &["--only", selection], "")?;
            assert_eq!(outcome.code, Some(1), "--only {selection}");
            assert!(outcome.stderr.contains(present), "{}", outcome.stderr);
            for needle in absent {
                assert!(
                    !outcome.stderr.contains(needle),
                    "--only {selection} must not report {needle}: {}",
                    outcome.stderr
                );
            }
        }
        Ok(())
    }

    #[test]
    fn a_tool_that_cannot_start_is_named_with_its_os_cause_once() -> TestResult {
        let fixture = Fixture::new(&bare(&["jackin-core"]))?;
        fixture.write_pins(PIN, PIN, PIN, PIN, PIN)?;
        let outcome = Spy::new()?.run(&fixture, &["--only", "docs"], "")?;
        assert_eq!(outcome.code, Some(1));
        assert_eq!(outcome.log, Vec::<String>::new());
        assert!(
            outcome.stderr.contains("cannot start bun"),
            "{}",
            outcome.stderr
        );
        assert_eq!(
            outcome.stderr.matches("No such file or directory").count(),
            1,
            "{}",
            outcome.stderr
        );
        Ok(())
    }

    #[test]
    fn invalid_selections_are_rejected_before_any_step_runs() -> TestResult {
        let spy = Spy::new()?;
        let fixture = green()?;
        let cases: [&[&str]; 6] = [
            &["--only", "bogus"],
            &["--only", "fmt,bogus"],
            &["--only", ""],
            &["--only", "fmt", "--fast"],
            &["--only", "fmt", "--no-docs"],
            &["--skip-clippy"],
        ];
        for args in cases {
            let outcome = spy.run(&fixture, args, "")?;
            assert_eq!(outcome.code, Some(2), "args {args:?}");
            assert_eq!(outcome.log, Vec::<String>::new(), "args {args:?}");
        }
        Ok(())
    }
}

mod tool_text {
    use jackin_xtask::CiError;

    #[test]
    fn output_of_other_tools_is_shown_without_control_characters() {
        let hostile = || "a\u{1b}[2J\nb".to_owned();
        let rows = [
            (
                CiError::Metadata(hostile()).to_string(),
                "cargo metadata failed: a\\u{1b}[2J\\nb",
            ),
            (
                CiError::Metadata("a \"b\" c\\d".to_owned()).to_string(),
                "cargo metadata failed: a \"b\" c\\d",
            ),
            (
                CiError::ListFiles(hostile()).to_string(),
                "git ls-files failed: a\\u{1b}[2J\\nb",
            ),
            (
                CiError::Spawn {
                    program: hostile(),
                    source: std::io::Error::other("x"),
                }
                .to_string(),
                "cannot start a\\u{1b}[2J\\nb",
            ),
        ];
        for (message, expected) in rows {
            assert_eq!(message, expected);
        }
    }
}
