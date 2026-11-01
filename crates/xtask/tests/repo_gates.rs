// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::Command;

use jackin_xtask::{Check, CiStep, CommentsCheck, ContainersCheck, LayersCheck, Repo, Violation};
use tempfile::TempDir;

type TestResult = Result<(), Box<dyn Error>>;

type Found = (CiStep, String, Option<u32>);

#[derive(Clone, Copy)]
enum Dep {
    Normal(&'static str),
    Dev(&'static str),
    Optional(&'static str),
    Renamed(&'static str, &'static str),
}

impl Dep {
    fn name(self) -> &'static str {
        match self {
            Dep::Normal(name) | Dep::Dev(name) | Dep::Optional(name) => name,
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
                "[workspace]\nmembers = [{}]\nresolver = \"3\"\n\n[workspace.dependencies]\n{shared}",
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
        let status = Command::new("git")
            .args(args)
            .current_dir(self.dir.path())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("git {args:?} exited with {status}").into())
        }
    }

    fn repo(&self) -> Result<Repo, Box<dyn Error>> {
        self.git(&["init", "-q"])?;
        self.git(&["add", "-A"])?;
        Ok(Repo::open(self.dir.path().to_path_buf())?)
    }
}

fn manifest(name: &str, deps: &[Dep]) -> String {
    let mut normal = String::new();
    let mut dev = String::new();
    for dep in deps {
        match dep {
            Dep::Normal(dep) => normal.push_str(&format!("{dep} = {{ workspace = true }}\n")),
            Dep::Optional(dep) => {
                normal.push_str(&format!(
                    "{dep} = {{ workspace = true, optional = true }}\n"
                ));
            }
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

fn remove(specs: &mut Specs, member: &str, dep_name: &str) {
    for (name, deps) in specs.iter_mut() {
        if *name == member {
            deps.retain(|dep| dep.name() != dep_name);
        }
    }
}

fn d14() -> Specs {
    use Dep::{Dev, Normal, Optional};
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
        ("jackin-testkit", vec![core, image, openshell, session, tui]),
        (
            "jackin",
            vec![
                core,
                image,
                openshell,
                session,
                tui,
                Normal("clap"),
                testkit,
            ],
        ),
        (
            "jackin-xtask",
            vec![
                Optional("jackin-core"),
                Optional("jackin-openshell"),
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
    fn scope_skips_build_output_other_types_and_generated_dir_only() -> TestResult {
        let generated = "# Generated by Velnor Actions 0.1.69; do not edit\necho \"${#v}\"\n";
        let fixture = Fixture::new(&bare(&["jackin-core"]))?;
        fixture.write("target/debug/gen.rs", "fn f() {} // c\n")?;
        fixture.write("target/x.toml", "# c\n")?;
        fixture.write("docs/page.md", "# Heading\n")?;
        fixture.write(".github/workflows/ci.yml", "# note\n")?;
        fixture.write(".github/scripts/gen.sh", generated)?;
        fixture.write("scripts/gen.sh", generated)?;
        assert_eq!(
            run(&fixture)?,
            [(CiStep::Comments, "scripts/gen.sh".to_owned(), Some(1))]
        );
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
        use Dep::{Dev, Normal, Optional};
        let mut specs = d14();
        add(&mut specs, "jackin-image", Normal("jackin-openshell"));
        add(&mut specs, "jackin-openshell", Normal("jackin-image"));
        add(&mut specs, "jackin-core", Normal("jackin-session"));
        add(&mut specs, "jackin-session", Normal("jackin-usage"));
        add(&mut specs, "jackin", Normal("jackin-usage-provider-codex"));
        add(&mut specs, "jackin-xtask", Optional("jackin-session"));
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
                &["jackin-xtask", "jackin-session"],
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
        add(&mut specs, "jackin-session", Normal("russh"));
        add(&mut specs, "jackin-image", Dev("openshell-policy"));
        add(&mut specs, "jackin-tui", Normal("openshell-sdk"));
        add(&mut specs, "jackin-testkit", Renamed("grpc", "tonic"));
        let violations = run(&specs, &[])?;
        let expected: Vec<Found> = [
            "jackin-core",
            "jackin-image",
            "jackin-session",
            "jackin-testkit",
            "jackin-tui",
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
                &["jackin-testkit", "tonic"],
                &["jackin-tui", "openshell-sdk"],
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
        let adapter = "use openshell_policy::Policy;\nuse openshell_sdk::Client;\nuse russh::client;\nuse tonic::Status;\n";
        let quoted = "fn docs() {\n    let normal = \"use openshell_sdk::Client;\";\n    let raw = r#\"use openshell_sdk::Client;\"#;\n    let tonic_status = 1;\n    tonicity::run();\n}\n";
        let violations = run(
            &d14(),
            &[
                ("crates/jackin-session/src/gw.rs", gateway),
                ("crates/jackin-image/src/net.rs", net),
                ("crates/jackin/src/m.rs", channel),
                ("crates/jackin-openshell/src/lib.rs", adapter),
                ("crates/jackin-session/src/doc.rs", quoted),
            ],
        )?;
        let expected = vec![
            (
                CiStep::Layers,
                "crates/jackin-image/src/net.rs".to_owned(),
                Some(line_of(net, "russh")),
            ),
            (
                CiStep::Layers,
                "crates/jackin-session/src/gw.rs".to_owned(),
                Some(1),
            ),
            (CiStep::Layers, "crates/jackin/src/m.rs".to_owned(), Some(1)),
        ];
        assert_eq!(summary(&violations), expected);
        assert_messages(
            &violations,
            &[
                &["jackin-image", "russh"],
                &["jackin-session", "openshell-sdk"],
                &["jackin", "tonic"],
            ],
        );
        Ok(())
    }

    #[test]
    fn optional_edges_must_be_optional_in_the_manifest() -> TestResult {
        use Dep::Normal;
        let mut specs = d14();
        remove(&mut specs, "jackin-xtask", "jackin-openshell");
        remove(&mut specs, "jackin-xtask", "jackin-core");
        add(&mut specs, "jackin-xtask", Normal("jackin-openshell"));
        add(&mut specs, "jackin-xtask", Normal("jackin-core"));
        let violations = run(&specs, &[])?;
        assert_eq!(
            summary(&violations),
            [
                manifest_found(CiStep::Layers, "jackin-xtask"),
                manifest_found(CiStep::Layers, "jackin-xtask"),
            ]
        );
        assert_messages(
            &violations,
            &[
                &["jackin-xtask", "jackin-core"],
                &["jackin-xtask", "jackin-openshell"],
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

mod containers {
    use super::*;

    const MEMBERS: [&str; 4] = ["jackin-core", "jackin-image", "jackin-session", "jackin"];

    fn run(files: &[(&str, &str)]) -> Result<Vec<Violation>, Box<dyn Error>> {
        let fixture = Fixture::new(&bare(&MEMBERS))?;
        for (path, text) in files {
            fixture.write(path, text)?;
        }
        Ok(ContainersCheck.run(&fixture.repo()?))
    }

    #[test]
    fn permitted_docker_use_and_look_alikes_pass() -> TestResult {
        let engine = "fn specs(build: &Build, tag: Tag) {\n    let a = ProcessSpec { program: Program::Docker, args: build.args.clone() };\n    let b = ProcessSpec { program: Program::Docker, args: vec![\"buildx\".to_owned(), \"create\".to_owned()] };\n    let c = ProcessSpec { program: Program::Docker, args: vec![\"buildx\".to_owned(), \"inspect\".to_owned()] };\n    let d = ProcessSpec {\n        args: vec![\"image\".to_owned(), \"inspect\".to_owned()],\n        program: Program::Docker,\n    };\n    let e = ProcessSpec { program: Program::Docker, args: vec![\"push\".to_owned(), tag] };\n}\n";
        let host = "fn host() {\n    let a = ProcessSpec { program: Program::Openshell, args: vec![\"sandbox\".to_owned(), \"run\".to_owned()] };\n    let b = ProcessSpec { program: Program::Git, args: vec![\"push\".to_owned()] };\n    let c = ProcessSpec { program: Program::Ssh, args: vec![\"run\".to_owned()] };\n    let d = Command::new(\"git\");\n    let e = Command::new(\"jackin\");\n    tracing::info!(\"docker ready, container stopped, podman absent\");\n    let container = 1;\n    let s = \"Command::new(\\\"docker\\\")\";\n    let t = r#\"Program::Docker docker run\"#;\n}\nfn docker_args() {}\n";
        let violations = run(&[
            ("crates/jackin-image/src/engine.rs", engine),
            ("crates/jackin-session/src/host.rs", host),
        ])?;
        assert_eq!(summary(&violations), []);
        Ok(())
    }

    #[test]
    fn docker_subcommands_outside_the_permitted_list_are_reported() -> TestResult {
        let engine = "fn bad() {\n    let multi = ProcessSpec { program: Program::Docker, args: vec![\n        \"run\".to_owned(),\n    ] };\n    let exec = ProcessSpec {\n        args: vec![\"exec\".to_owned()],\n        program: Program::Docker,\n    };\n    let create = ProcessSpec { program: Program::Docker, args: vec![\"create\".to_owned()] };\n    let start = ProcessSpec { program: Program::Docker, args: vec![\"start\".to_owned()] };\n    let pull = ProcessSpec { program: Program::Docker, args: vec![\"pull\".to_owned()] };\n    let bake = ProcessSpec { program: jackin_core::Program::Docker, args: vec![\"buildx\".to_owned(), \"bake\".to_owned()] };\n    let rm = ProcessSpec { program: Program::Docker, args: vec![\"buildx\".to_owned(), \"rm\".to_owned()] };\n    let image_rm = ProcessSpec { program: Program::Docker, args: vec![\"image\".to_owned(), \"rm\".to_owned()] };\n}\n";
        let violations = run(&[("crates/jackin-image/src/engine.rs", engine)])?;
        let tags = [
            "let multi",
            "        program: Program::Docker,",
            "let create",
            "let start",
            "let pull",
            "let bake",
            "let rm",
            "let image_rm",
        ];
        let expected = lines(
            CiStep::Containers,
            "crates/jackin-image/src/engine.rs",
            engine,
            &tags,
        );
        assert_eq!(summary(&violations), expected);
        assert_messages(
            &violations,
            &[
                &["run"],
                &["exec"],
                &["create"],
                &["start"],
                &["pull"],
                &["buildx bake"],
                &["buildx rm"],
                &["image rm"],
            ],
        );
        Ok(())
    }

    #[test]
    fn docker_started_outside_jackin_image_is_reported() -> TestResult {
        let session = "fn a() {\n    let spec = ProcessSpec { program: Program::Docker, args: vec![\"buildx\".to_owned(), \"build\".to_owned()] };\n}\n";
        let core = "fn b() {\n    let c = std::process::Command::new(\"docker\");\n}\n";
        let binary = "fn c() {\n    let a = tokio::process::Command::new(\"docker\");\n    let b = Command::new(\n        \"docker\",\n    );\n    let c = Command::new(\"/usr/local/bin/docker\");\n}\n";
        let image = "fn d() {\n    let d = Command::new(\"docker\");\n}\n";
        let violations = run(&[
            ("crates/jackin-session/src/a.rs", session),
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
            "crates/jackin-session/src/a.rs",
            session,
            &["let spec"],
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
        let core = "fn b() {\n    let c = Command::new(\"container\");\n}\n";
        let image = "fn c() {\n    let d = std::process::Command::new(\"podman\");\n}\n";
        let binary = "fn d() {\n    let e = Command::new(\n        \"container\",\n    );\n}\n";
        let violations = run(&[
            ("crates/jackin-session/src/a.rs", session),
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
            "crates/jackin/src/d.rs",
            binary,
            &["let e"],
        ));
        assert_eq!(summary(&violations), expected);
        Ok(())
    }
}
