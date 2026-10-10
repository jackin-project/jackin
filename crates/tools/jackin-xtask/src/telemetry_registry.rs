//! Closed telemetry-registry validation and generation gate.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::docs::repo_root;
use anyhow::{Context, Result, bail};
use clap::Args;
use serde::Deserialize;
use serde_yaml_ng::Value as YamlValue;
use sha2::{Digest as _, Sha256};
use syn::parse::Parser as _;
use syn::spanned::Spanned as _;
use syn::visit::Visit as _;
mod ownership_census;
mod source_policy;
use source_policy::{SourcePolicyScanner, WorkspaceSpawnTypes};

// Shrink-only: new files must use governed facades and helpers immediately.
const RAW_SPAWN_ALLOWLIST: &[&str] = &[];

const RAW_SCOPED_THREAD_ALLOWLIST: &[(&str, &str)] = &[(
    "crates/core/jackin-process/src/lib.rs",
    "T0 process transport cannot depend on peer T0 telemetry facade",
)];

const RAW_TRACING_ALLOWLIST: &[&str] = &[];

const PROHIBITED_TELEMETRY_MACROS: &[&str] = &[
    "debug_log",
    "clog",
    "cdebug",
    "ctrace_payload",
    "cwarn",
    "cerror",
    "telemetry_info",
    "telemetry_debug",
    "telemetry_warn",
    "telemetry_error",
];

#[derive(Args, Debug)]
pub(crate) struct TelemetryRegistryArgs {
    /// Regenerate Rust sources from the registry before validating.
    #[arg(long)]
    pub(crate) generate: bool,
}

#[derive(Debug, Deserialize)]
struct RustRegistryMetadata {
    registry_schema_version: String,
    registry_schema_url: String,
    dependency_schema_version: String,
    dependency_schema_url: String,
    dependency_registry_path: String,
    dependency_registry_revision: String,
    dependency_registry_checksum: String,
    rust_crate_version: String,
    rust_crate_checksum: String,
    rust_crate_schema_url: String,
    root_standard_reexports: Vec<String>,
    standard_event_groups: Vec<String>,
    adoption: AdoptionPolicy,
    standard_upstream: BTreeMap<String, String>,
    standard_local: BTreeMap<String, String>,
    #[serde(default)]
    standard_enum: Vec<StandardEnum>,
    #[serde(default)]
    scoped_enum: Vec<ScopedEnum>,
    #[serde(default)]
    rust_type_overrides: BTreeMap<String, String>,
    #[serde(default)]
    skip_attribute_enums: Vec<String>,
    #[serde(default)]
    metric_boundaries: BTreeMap<String, Vec<f64>>,
    #[serde(default)]
    facade_name_overrides: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct AdoptionPolicy {
    upstream_available: String,
    upstream_absent: String,
    wire_name_change: String,
    schema_version_change: String,
}

#[derive(Debug, Deserialize)]
struct StandardEnum {
    rust_type: String,
    attribute: String,
    values: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ScopedEnum {
    rust_type: String,
    values: Vec<String>,
}

pub(crate) fn run(args: TelemetryRegistryArgs) -> Result<()> {
    let root = repo_root()?;
    validate_with_weaver(&root)?;
    let generated = generate_rust_sources(&root)?;
    if args.generate {
        write_generated_sources(&root, &generated)?;
    }
    validate_registry_matches_rust(&root, &generated)?;
    validate_adoption_metadata(&root)?;
    validate_legacy_namespaces(&root)?;
    ownership_census::validate(&root, args.generate)?;
    validate_source_policy(&root)
}

fn validate_source_policy(root: &Path) -> Result<()> {
    let mut files = Vec::new();
    collect_source_files(&root.join("crates"), root, &mut files)?;
    let parsed = files
        .into_iter()
        .map(|(relative, source)| {
            let path = relative.to_string_lossy().into_owned();
            let syntax = syn::parse_file(&source)
                .with_context(|| format!("parsing {path} for telemetry source policy"))?;
            Ok((path, syntax))
        })
        .collect::<Result<Vec<_>>>()?;
    let indexed = parsed
        .iter()
        .map(|(path, syntax)| (path.as_str(), syntax))
        .collect::<Vec<_>>();
    let workspace_spawn_types = WorkspaceSpawnTypes::collect(&indexed);
    let mut violations = Vec::new();
    for (path, syntax) in parsed {
        let mut scanner = SourcePolicyScanner::new(&path, &syntax, &workspace_spawn_types);
        scanner.visit_file(&syntax);
        violations.extend(
            scanner
                .violations
                .into_iter()
                .map(|(line, violation)| format!("{path}:{line}: {violation}")),
        );
    }
    if violations.is_empty() {
        Ok(())
    } else {
        bail!(
            "telemetry source-policy violations:\n  {}",
            violations.join("\n  ")
        )
    }
}

fn collect_source_files(
    dir: &Path,
    root: &Path,
    files: &mut Vec<(std::path::PathBuf, String)>,
) -> Result<()> {
    for entry in crate::fs_util::read_dir_sorted(dir)? {
        let path = entry.path();
        if path.is_dir() {
            collect_source_files(&path, root, files)?;
            continue;
        }
        if path.extension().and_then(|value| value.to_str()) != Some("rs") {
            continue;
        }
        let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
        let text = relative.to_string_lossy();
        if text.contains("/tests/")
            || text.ends_with("/tests.rs")
            || text.contains("/benches/")
            || text.contains("/fuzz/")
            || text.starts_with("crates/tools/jackin-xtask/")
            || text.starts_with("crates/tools/jackin-dev/")
            || text.starts_with("crates/tools/jackin-pr-trailers/")
            || text.contains("lookbook")
            || text.starts_with("crates/tools/jackin-lints/")
        {
            continue;
        }
        files.push((relative, fs::read_to_string(&path)?));
    }
    Ok(())
}

fn validate_with_weaver(root: &Path) -> Result<()> {
    let mut version = crate::cmd::command("weaver");
    version.arg("--version");
    if crate::cmd::output_raw(&mut version).is_err() {
        return Ok(());
    }
    let mut command = crate::cmd::command("weaver");
    command.current_dir(root).args([
        "registry",
        "check",
        "-r",
        "crates/services/jackin-telemetry/registry",
    ]);
    crate::cmd::run_streaming(&mut command)
}

fn validate_registry_matches_rust(root: &Path, generated: &[(String, String)]) -> Result<()> {
    let mut drift = Vec::new();
    for (relative, expected) in generated {
        let path = root.join(relative);
        let actual = fs::read_to_string(&path)
            .with_context(|| format!("reading generated source {}", path.display()))?;
        if actual != *expected {
            drift.push(relative.clone());
        }
    }
    if drift.is_empty() {
        Ok(())
    } else {
        bail!(
            "telemetry registry/Rust drift in {}; run `cargo xtask telemetry-registry --generate`",
            drift.join(", ")
        )
    }
}

fn write_generated_sources(root: &Path, generated: &[(String, String)]) -> Result<()> {
    for (relative, contents) in generated {
        let path = root.join(relative);
        fs::write(&path, contents).with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(())
}

fn generate_rust_sources(root: &Path) -> Result<Vec<(String, String)>> {
    let registry = root.join("crates/services/jackin-telemetry/registry");
    let metadata = load_rust_metadata(&registry)?;
    let header_path = root.join("crates/services/jackin-telemetry/templates/rust.j2");
    let mut header = fs::read_to_string(&header_path)
        .with_context(|| format!("reading {}", header_path.display()))?;
    if !header.ends_with('\n') {
        header.push('\n');
    }
    header.push('\n');

    let resolved = load_weaver_resolved_registry(root)?;
    let groups = yaml_sequence(&resolved, "groups")
        .ok_or_else(|| anyhow::anyhow!("Weaver resolved registry has no groups array"))?;
    let attribute_group = groups
        .iter()
        .find(|group| group.get("id").and_then(YamlValue::as_str) == Some("registry.jackin"))
        .ok_or_else(|| anyhow::anyhow!("Weaver resolved registry has no registry.jackin group"))?;
    let attributes = yaml_sequence(attribute_group, "attributes").unwrap_or_default();
    let mut local_groups = groups
        .iter()
        .filter(|group| is_local_resolved_group(group, &metadata.registry_schema_url))
        .cloned()
        .collect::<Vec<_>>();
    for id in &metadata.standard_event_groups {
        let group = groups
            .iter()
            .find(|group| group.get("id").and_then(YamlValue::as_str) == Some(id.as_str()))
            .ok_or_else(|| anyhow::anyhow!("resolved upstream registry has no {id} group"))?;
        local_groups.push(group.clone());
    }
    validate_facade_name_overrides(&local_groups, &metadata)?;

    let generated = vec![
        (
            "crates/services/jackin-telemetry/src/schema/attrs.rs".to_owned(),
            generate_attributes(&header, attributes, &metadata)?,
        ),
        (
            "crates/services/jackin-telemetry/src/schema/enums.rs".to_owned(),
            generate_enums(&header, attributes, &metadata)?,
        ),
        (
            "crates/services/jackin-telemetry/src/schema/events.rs".to_owned(),
            generate_signal_constants(
                &header,
                &local_groups,
                attributes,
                "name",
                SignalKind::Event,
                &metadata,
            )?,
        ),
        (
            "crates/services/jackin-telemetry/src/event_emit.rs".to_owned(),
            generate_event_emitters(&header, &local_groups)?,
        ),
        (
            "crates/services/jackin-telemetry/src/event_defs.rs".to_owned(),
            generate_facade_definitions(&header, &local_groups, SignalKind::Event, &metadata)?,
        ),
        (
            "crates/services/jackin-telemetry/src/schema/spans.rs".to_owned(),
            generate_signal_constants(
                &header,
                &local_groups,
                attributes,
                "id",
                SignalKind::Span,
                &metadata,
            )?,
        ),
        (
            "crates/services/jackin-telemetry/src/operation_defs.rs".to_owned(),
            generate_facade_definitions(&header, &local_groups, SignalKind::Span, &metadata)?,
        ),
        (
            "crates/services/jackin-telemetry/src/schema/metrics.rs".to_owned(),
            generate_signal_constants(
                &header,
                &local_groups,
                attributes,
                "metric_name",
                SignalKind::Metric,
                &metadata,
            )?,
        ),
        (
            "crates/services/jackin-telemetry/src/metric_defs.rs".to_owned(),
            generate_facade_definitions(&header, &local_groups, SignalKind::Metric, &metadata)?,
        ),
    ];
    generated
        .into_iter()
        .map(|(path, source)| Ok((path, format_generated_rust(&source)?)))
        .collect()
}

fn validate_facade_name_overrides(
    groups: &[YamlValue],
    metadata: &RustRegistryMetadata,
) -> Result<()> {
    let signal_names = groups
        .iter()
        .filter_map(
            |group| match group.get("type").and_then(YamlValue::as_str) {
                Some("event") => group.get("name").and_then(YamlValue::as_str),
                Some("span") => group
                    .get("id")
                    .and_then(YamlValue::as_str)
                    .map(|name| name.strip_prefix("span.").unwrap_or(name)),
                Some("metric") => group.get("metric_name").and_then(YamlValue::as_str),
                _ => None,
            },
        )
        .collect::<BTreeSet<_>>();
    let unknown = metadata
        .facade_name_overrides
        .keys()
        .filter(|name| !signal_names.contains(name.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if !unknown.is_empty() {
        bail!(
            "facade name overrides reference unknown signals: {}",
            unknown.join(", ")
        );
    }
    let mut identifiers = BTreeSet::new();
    for name in signal_names {
        let identifier = metadata
            .facade_name_overrides
            .get(name)
            .cloned()
            .unwrap_or_else(|| rust_constant(name));
        if !identifiers.insert(identifier.clone()) {
            bail!("duplicate generated facade identifier {identifier}");
        }
    }
    Ok(())
}

fn format_generated_rust(source: &str) -> Result<String> {
    let mut child = Command::new("rustfmt")
        .args(["--emit", "stdout", "--edition", "2024"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("starting rustfmt for generated telemetry schema")?;
    child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("rustfmt stdin was not piped"))?
        .write_all(source.as_bytes())?;
    let output = child.wait_with_output()?;
    if !output.status.success() {
        bail!("rustfmt rejected generated telemetry schema");
    }
    String::from_utf8(output.stdout).context("rustfmt produced non-UTF-8 output")
}

#[derive(Clone, Copy)]
enum SignalKind {
    Event,
    Span,
    Metric,
}

fn generate_attributes(
    header: &str,
    attributes: &[YamlValue],
    metadata: &RustRegistryMetadata,
) -> Result<String> {
    let mut output = header.to_owned();
    let mut names = Vec::new();
    let mut definitions = Vec::new();
    for attribute in attributes {
        let id = yaml_string(attribute, "name")?;
        let constant = rust_constant(id);
        let kind = if yaml_sequence(yaml_required(attribute, "type")?, "members").is_some() {
            "enum"
        } else {
            yaml_required(attribute, "type")?
                .as_str()
                .unwrap_or("structured")
        };
        output.push_str(&format!(
            "// registry-type: {kind}\npub const {constant}: &str = \"{id}\";\n"
        ));
        output.push_str(&format!(
            "pub const {constant}_DEF: super::AttributeMetadata = super::AttributeMetadata {{ name: {constant}, description: {:?}, value_type: super::ValueType::{}, allowed_values: {} }};\n",
            yaml_string(attribute, "brief")?,
            value_type_variant(yaml_required(attribute, "type")?)?,
            allowed_values(yaml_required(attribute, "type")?)?
        ));
        definitions.push(format!("{constant}_DEF"));
        names.push(constant);
    }
    write_all_slice(&mut output, "ALL_KEYS", &names);
    output.push_str("\npub const ALL_DEFINITIONS: &[super::AttributeMetadata] = &[\n");
    for definition in definitions {
        output.push_str(&format!("    {definition},\n"));
    }
    output.push_str("];\n");
    output.push_str(
        "\npub fn definition(name: &str) -> Option<&'static super::AttributeMetadata> {\n    ALL_DEFINITIONS.iter().find(|definition| definition.name == name)\n}\n",
    );
    output.push_str("\n/// Standard semantic-convention keys isolated behind a stable facade.\n");
    output.push_str("pub mod std_attrs {\n");
    let mut standard_names = Vec::new();
    for constant in metadata.standard_upstream.keys() {
        output.push_str(&format!(
            "    pub use opentelemetry_semantic_conventions::attribute::{constant};\n"
        ));
        standard_names.push(constant.clone());
    }
    for (constant, wire_name) in &metadata.standard_local {
        output.push_str(&format!(
            "    // Local pin: not authoritative in opentelemetry-semantic-conventions {}; registry schema {}.\n",
            metadata.rust_crate_version, metadata.registry_schema_version
        ));
        output.push_str(&format!(
            "    pub const {constant}: &str = \"{wire_name}\";\n"
        ));
        standard_names.push(constant.clone());
    }
    output.push_str("    pub const ALL_KEYS: &[&str] = &[\n");
    for name in standard_names {
        output.push_str(&format!("        {name},\n"));
    }
    output.push_str("    ];\n");
    output.push_str("    pub const UPSTREAM_ALIASES: &[(&str, &str)] = &[\n");
    for (constant, wire_name) in &metadata.standard_upstream {
        output.push_str(&format!("        ({constant}, {wire_name:?}),\n"));
    }
    output.push_str("    ];\n");
    output.push_str(&format!(
        "    pub const RUST_CRATE_SCHEMA_URL: &str = {:?};\n",
        metadata.rust_crate_schema_url
    ));
    output.push_str("}\n");
    if !metadata.root_standard_reexports.is_empty() {
        output.push_str("\n// Compatibility re-exports; new code uses `std_attrs`.\n");
        output.push_str(&format!(
            "pub use std_attrs::{{{}}};\n",
            metadata.root_standard_reexports.join(", ")
        ));
    }
    Ok(output)
}

fn generate_enums(
    header: &str,
    attributes: &[YamlValue],
    metadata: &RustRegistryMetadata,
) -> Result<String> {
    let mut output = header.to_owned();
    output.push_str(
        "macro_rules! bounded_values {\n    ($name:ident { $($variant:ident => $value:literal),+ $(,)? }) => {\n        #[derive(Debug, Clone, Copy, PartialEq, Eq)]\n        pub enum $name { $($variant),+ }\n        impl $name {\n            pub const ALL: &'static [Self] = &[$(Self::$variant),+];\n            #[must_use]\n            pub const fn as_str(self) -> &'static str {\n                match self { $(Self::$variant => $value),+ }\n            }\n        }\n    };\n}\n\n",
    );
    let mut emitted = BTreeMap::<String, Vec<String>>::new();
    for attribute in attributes {
        let id = yaml_string(attribute, "name")?;
        if metadata
            .skip_attribute_enums
            .iter()
            .any(|skipped| skipped == id)
        {
            continue;
        }
        let Some(members) = yaml_sequence(yaml_required(attribute, "type")?, "members") else {
            continue;
        };
        let values = members
            .iter()
            .map(|member| yaml_string(member, "value").map(str::to_owned))
            .collect::<Result<Vec<_>>>()?;
        let rust_type = metadata
            .rust_type_overrides
            .get(id)
            .cloned()
            .unwrap_or_else(|| rust_pascal(id));
        emit_enum(&mut output, &mut emitted, &rust_type, &values)?;
    }
    for item in &metadata.standard_enum {
        if !metadata
            .standard_upstream
            .values()
            .chain(metadata.standard_local.values())
            .any(|attribute| attribute == &item.attribute)
        {
            bail!(
                "standard enum {} references unregistered attribute {}",
                item.rust_type,
                item.attribute
            );
        }
        emit_enum(&mut output, &mut emitted, &item.rust_type, &item.values)?;
    }
    for item in &metadata.scoped_enum {
        emit_enum(&mut output, &mut emitted, &item.rust_type, &item.values)?;
    }
    Ok(output)
}

fn emit_enum(
    output: &mut String,
    emitted: &mut BTreeMap<String, Vec<String>>,
    rust_type: &str,
    values: &[String],
) -> Result<()> {
    if let Some(previous) = emitted.get(rust_type) {
        if previous != values {
            bail!("bounded enum {rust_type} has conflicting registry values");
        }
        return Ok(());
    }
    emitted.insert(rust_type.to_owned(), values.to_vec());
    let members = values
        .iter()
        .map(|value| format!("{} => \"{value}\"", rust_pascal(value)))
        .collect::<Vec<_>>()
        .join(", ");
    output.push_str(&format!("bounded_values!({rust_type} {{ {members} }});\n"));
    Ok(())
}

fn generate_signal_constants(
    header: &str,
    groups: &[YamlValue],
    registry_attributes: &[YamlValue],
    name_key: &str,
    kind: SignalKind,
    rust_metadata: &RustRegistryMetadata,
) -> Result<String> {
    let mut output = header.to_owned();
    let mut names = Vec::new();
    let mut definitions = Vec::new();
    for group in groups {
        let expected_type = match kind {
            SignalKind::Event => "event",
            SignalKind::Span => "span",
            SignalKind::Metric => "metric",
        };
        if group.get("type").and_then(YamlValue::as_str) != Some(expected_type) {
            continue;
        }
        let raw_name = yaml_string(group, name_key)?;
        let name = if matches!(kind, SignalKind::Span) {
            raw_name.strip_prefix("span.").unwrap_or(raw_name)
        } else {
            raw_name
        };
        let constant = rust_constant(name);
        let group_attributes = yaml_sequence(group, "attributes").unwrap_or_default();
        let attributes = group_attributes
            .iter()
            .map(|attribute| {
                let reference = yaml_string(attribute, "name")?;
                let requirement = requirement_level_name(attribute);
                Ok(format!("{reference}:{requirement}"))
            })
            .collect::<Result<Vec<_>>>()?
            .join(",");
        let metadata = match kind {
            SignalKind::Event => format!("attributes={attributes}"),
            SignalKind::Span => format!(
                "kind={}; attributes={attributes}",
                yaml_string(group, "span_kind")?
            ),
            SignalKind::Metric => format!(
                "instrument={}; unit={}; attributes={attributes}",
                yaml_string(group, "instrument")?,
                yaml_string(group, "unit")?
            ),
        };
        output.push_str(&format!(
            "// registry: {metadata}\npub const {constant}: &str = \"{name}\";\n"
        ));
        let description = yaml_string(group, "brief")?;
        output.push_str(&format!(
            "pub const {constant}_DEF: super::{} = super::{} {{\n",
            signal_metadata_type(kind),
            signal_metadata_type(kind)
        ));
        output.push_str(&format!("    name: {constant},\n"));
        output.push_str(&format!("    description: {description:?},\n"));
        match kind {
            SignalKind::Event => output.push_str(&format!(
                "    severity: super::EventSeverity::{},\n",
                rust_pascal(event_runtime_severity(group)?)
            )),
            SignalKind::Span => output.push_str(&format!(
                "    kind: super::SpanKind::{},\n",
                rust_pascal(yaml_string(group, "span_kind")?)
            )),
            SignalKind::Metric => {
                write_metric_metadata(&mut output, group, name, rust_metadata)?;
            }
        }
        output.push_str("    attributes: &[\n");
        for attribute in group_attributes {
            let attribute_name = yaml_string(attribute, "name")?;
            let attribute_type = registry_attributes
                .iter()
                .find(|candidate| {
                    candidate.get("name").and_then(YamlValue::as_str) == Some(attribute_name)
                })
                .and_then(|candidate| candidate.get("type"))
                .unwrap_or(yaml_required(attribute, "type")?);
            output.push_str("        super::AttributeRequirement {\n");
            output.push_str(&format!("            name: {attribute_name:?},\n"));
            output.push_str(&format!(
                "            value_type: super::ValueType::{},\n",
                value_type_variant(attribute_type)?
            ));
            output.push_str(&format!(
                "            requirement: super::RequirementLevel::{},\n",
                rust_pascal(requirement_level_name(attribute))
            ));
            output.push_str(&format!(
                "            allowed_values: {},\n",
                allowed_values(attribute_type)?
            ));
            output.push_str("        },\n");
        }
        output.push_str("    ],\n};\n");
        definitions.push(format!("{constant}_DEF"));
        names.push(constant);
    }
    write_all_slice(&mut output, "ALL", &names);
    output.push_str(&format!(
        "\npub const DEFINITIONS: &[super::{}] = &[\n",
        signal_metadata_type(kind)
    ));
    for definition in definitions {
        output.push_str(&format!("    {definition},\n"));
    }
    output.push_str("];\n");
    output.push_str(&format!(
        "\n#[must_use]\npub fn definition(name: &str) -> Option<&'static super::{}> {{\n    DEFINITIONS.iter().find(|definition| definition.name == name)\n}}\n",
        signal_metadata_type(kind)
    ));
    Ok(output)
}

fn event_runtime_severity(group: &YamlValue) -> Result<&str> {
    let note = yaml_string(group, "note")?;
    let severity = note.strip_prefix("runtime_severity=").ok_or_else(|| {
        anyhow::anyhow!(
            "event {} has no runtime severity note",
            yaml_string(group, "name").unwrap_or("<unknown>")
        )
    })?;
    match severity {
        "trace" | "debug" | "info" | "warn" | "error" => Ok(severity),
        other => bail!("unsupported event runtime severity {other}"),
    }
}

fn generate_facade_definitions(
    header: &str,
    groups: &[YamlValue],
    kind: SignalKind,
    metadata: &RustRegistryMetadata,
) -> Result<String> {
    let (expected_type, schema_module, definition_type) = match kind {
        SignalKind::Event => ("event", "events", "EventDef"),
        SignalKind::Span => ("span", "spans", "SpanDef"),
        SignalKind::Metric => ("metric", "metrics", "InstrumentDef"),
    };
    let mut output = header.to_owned();
    let mut constants = Vec::new();
    for group in groups {
        if group.get("type").and_then(YamlValue::as_str) != Some(expected_type) {
            continue;
        }
        let raw_name = match kind {
            SignalKind::Event => yaml_string(group, "name")?,
            SignalKind::Span => yaml_string(group, "id")?
                .strip_prefix("span.")
                .unwrap_or(yaml_string(group, "id")?),
            SignalKind::Metric => yaml_string(group, "metric_name")?,
        };
        let schema_constant = rust_constant(raw_name);
        let constant = metadata
            .facade_name_overrides
            .get(raw_name)
            .cloned()
            .unwrap_or_else(|| schema_constant.clone());
        output.push_str(&format!(
            "pub const {constant}: {definition_type} = {definition_type}::generated(&schema::{schema_module}::{schema_constant}_DEF);\n"
        ));
        constants.push(constant);
    }
    output.push_str(&format!("\npub const ALL: &[{definition_type}] = &[\n"));
    for constant in constants {
        output.push_str(&format!("    {constant},\n"));
    }
    output.push_str("];\n");
    Ok(output)
}

fn generate_event_emitters(header: &str, groups: &[YamlValue]) -> Result<String> {
    let mut output = header.to_owned();
    let mut value_types = BTreeSet::new();
    for group in groups {
        if group.get("type").and_then(YamlValue::as_str) != Some("event") {
            continue;
        }
        for attribute in yaml_sequence(group, "attributes").unwrap_or_default() {
            value_types.insert(value_type_variant(yaml_required(attribute, "type")?)?);
        }
    }
    output.push_str("macro_rules! event_field_value {\n");
    for value_type in value_types {
        let accessor = match value_type {
            "String" => "$fields.str($key)",
            "Integer" => "$fields.integer($key)",
            "Double" => "$fields.double($key)",
            "Boolean" => "$fields.boolean($key)",
            // `tracing` has no array Value variant. The governed processor adds
            // the registered typed value through `take_pending_event_arrays`.
            "StringArray" => "tracing::field::Empty",
            _ => unreachable!("validated registry value type"),
        };
        output.push_str(&format!(
            "    ($fields:expr, $key:literal, {value_type}) => {{ {accessor} }};\n"
        ));
    }
    output.push_str("}\n\n");
    let mut events = groups
        .iter()
        .filter(|group| group.get("type").and_then(YamlValue::as_str) == Some("event"))
        .collect::<Vec<_>>();
    events.sort_by_key(|group| group.get("name").and_then(YamlValue::as_str));
    let chunks = events.chunks(16).collect::<Vec<_>>();
    output.push_str("fn emit_registered_event(def: &'static EventDef, fields: FieldSet<'_>) {\n");
    for (index, chunk) in chunks.iter().enumerate() {
        let last = chunk
            .last()
            .ok_or_else(|| anyhow::anyhow!("event emitter chunk is empty"))?;
        let last_name = yaml_string(last, "name")?;
        let prefix = if index == 0 {
            "    if"
        } else {
            "    } else if"
        };
        output.push_str(&format!(
            "{prefix} def.name <= {last_name:?} {{\n        emit_registered_event_{index}(def, fields);\n"
        ));
    }
    output.push_str(
        "    } else {\n        unreachable!(\"validated event registry\");\n    }\n}\n\n",
    );
    for (index, chunk) in chunks.iter().enumerate() {
        output.push_str(&format!(
            "fn emit_registered_event_{index}(def: &'static EventDef, fields: FieldSet<'_>) {{\n    match def.name {{\n"
        ));
        for group in *chunk {
            let name = yaml_string(group, "name")?;
            let constant = rust_constant(name);
            let emitter = rust_field_identifier(name).replacen("field_", "emit_", 1);
            output.push_str(&format!(
                "        schema::events::{constant} => {{ {emitter}(def, fields); }}\n"
            ));
        }
        output.push_str(
            "        _ => unreachable!(\"validated event registry chunk\"),\n    }\n}\n\n",
        );
    }
    for group in events {
        let name = yaml_string(group, "name")?;
        let emitter = rust_field_identifier(name).replacen("field_", "emit_", 1);
        output.push_str(&format!(
            "fn {emitter}(def: &'static EventDef, fields: FieldSet<'_>) {{\n    emit_schema_event!({name:?}, def.severity, fields, [\n"
        ));
        for attribute in yaml_sequence(group, "attributes").unwrap_or_default() {
            let attribute_name = yaml_string(attribute, "name")?;
            let field = rust_field_identifier(attribute_name);
            let value_type = value_type_variant(yaml_required(attribute, "type")?)?;
            output.push_str(&format!(
                "        ({attribute_name:?}, {field}, {value_type}),\n"
            ));
        }
        output.push_str("    ]);\n}\n\n");
    }
    Ok(output)
}

fn write_metric_metadata(
    output: &mut String,
    group: &YamlValue,
    name: &str,
    rust_metadata: &RustRegistryMetadata,
) -> Result<()> {
    let instrument = yaml_string(group, "instrument")?;
    output.push_str(&format!(
        "    instrument: super::MetricInstrument::{},\n",
        metric_instrument_variant(instrument)?
    ));
    output.push_str(&format!("    unit: {:?},\n", yaml_string(group, "unit")?));
    let boundaries = rust_metadata
        .metric_boundaries
        .get(name)
        .map(Vec::as_slice)
        .unwrap_or_default();
    if instrument == "histogram" && boundaries.is_empty() {
        bail!("histogram {name} has no fixed boundaries in registry/rust.toml");
    }
    if instrument != "histogram" && !boundaries.is_empty() {
        bail!("non-histogram {name} declares histogram boundaries");
    }
    output.push_str("    boundaries: &[");
    for (index, boundary) in boundaries.iter().enumerate() {
        if index > 0 {
            output.push_str(", ");
        }
        output.push_str(&format!("{boundary:?}"));
    }
    output.push_str("],\n");
    Ok(())
}

const fn signal_metadata_type(kind: SignalKind) -> &'static str {
    match kind {
        SignalKind::Event => "EventMetadata",
        SignalKind::Span => "SpanMetadata",
        SignalKind::Metric => "MetricMetadata",
    }
}

fn metric_instrument_variant(value: &str) -> Result<&'static str> {
    match value {
        "counter" => Ok("Counter"),
        "gauge" => Ok("Gauge"),
        "updowncounter" => Ok("UpDownCounter"),
        "histogram" => Ok("Histogram"),
        other => bail!("unsupported metric instrument {other}"),
    }
}

fn value_type_variant(value: &YamlValue) -> Result<&'static str> {
    if yaml_sequence(value, "members").is_some() {
        return Ok("String");
    }
    match value.as_str().unwrap_or("structured") {
        "string" => Ok("String"),
        "boolean" => Ok("Boolean"),
        "int" => Ok("Integer"),
        "double" => Ok("Double"),
        "string[]" => Ok("StringArray"),
        other => bail!("unsupported registry attribute type {other}"),
    }
}

fn allowed_values(value: &YamlValue) -> Result<String> {
    let Some(members) = yaml_sequence(value, "members") else {
        return Ok("&[]".to_owned());
    };
    let values = members
        .iter()
        .map(|member| yaml_string(member, "value").map(|value| format!("{value:?}")))
        .collect::<Result<Vec<_>>>()?;
    Ok(format!("&[{}]", values.join(", ")))
}

fn requirement_level_name(attribute: &YamlValue) -> &str {
    let Some(requirement) = attribute.get("requirement_level") else {
        return "recommended";
    };
    if let Some(level) = requirement.as_str() {
        return level;
    }
    requirement
        .as_mapping()
        .and_then(|mapping| mapping.keys().find_map(YamlValue::as_str))
        .unwrap_or("recommended")
}

fn write_all_slice(output: &mut String, name: &str, members: &[String]) {
    output.push_str(&format!("\npub const {name}: &[&str] = &[\n"));
    for member in members {
        output.push_str(&format!("    {member},\n"));
    }
    output.push_str("];\n");
}

fn load_weaver_resolved_registry(root: &Path) -> Result<YamlValue> {
    let mut command = crate::cmd::command("weaver");
    command.current_dir(root).args([
        "registry",
        "resolve",
        "-r",
        "crates/services/jackin-telemetry/registry",
        "-f",
        "json",
        "--quiet",
    ]);
    let output = crate::cmd::output(&mut command)
        .context("resolving pinned telemetry registry with Weaver")?;
    serde_json::from_slice(&output).context("parsing Weaver resolved registry JSON")
}

fn is_local_resolved_group(group: &YamlValue, schema_url: &str) -> bool {
    group
        .get("lineage")
        .and_then(|value| value.get("provenance"))
        .and_then(|value| value.get("schema_url"))
        .and_then(YamlValue::as_str)
        == Some(schema_url)
}

fn load_rust_metadata(registry: &Path) -> Result<RustRegistryMetadata> {
    let path = registry.join("rust.toml");
    toml::from_str(
        &fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?,
    )
    .with_context(|| format!("parsing {}", path.display()))
}

fn validate_adoption_metadata(root: &Path) -> Result<()> {
    let registry = root.join("crates/services/jackin-telemetry/registry");
    let metadata = load_rust_metadata(&registry)?;
    let manifest: YamlValue =
        serde_yaml_ng::from_str(&fs::read_to_string(registry.join("manifest.yaml"))?)?;
    let manifest_schema_url = yaml_string(&manifest, "schema_url")?;
    let dependencies = yaml_sequence(&manifest, "dependencies")
        .ok_or_else(|| anyhow::anyhow!("registry manifest has no dependencies"))?;
    let dependency_schema_url = dependencies
        .first()
        .map(|dependency| yaml_string(dependency, "schema_url"))
        .transpose()?
        .ok_or_else(|| {
            anyhow::anyhow!("registry manifest has no semantic-convention dependency")
        })?;
    let dependency_registry_path = dependencies
        .first()
        .map(|dependency| yaml_string(dependency, "registry_path"))
        .transpose()?
        .ok_or_else(|| anyhow::anyhow!("registry manifest dependency has no registry_path"))?;
    let expected = (
        "1.45.0",
        "https://jackin.tailrocks.com/telemetry/schemas/1.45.0",
        "1.43.0",
        "https://opentelemetry.io/schemas/1.43.0",
        "crates/services/jackin-telemetry/vendor/semconv-1.43.0",
        "=0.33.0",
        "https://opentelemetry.io/schemas/1.42.0",
    );
    let actual = (
        metadata.registry_schema_version.as_str(),
        metadata.registry_schema_url.as_str(),
        metadata.dependency_schema_version.as_str(),
        metadata.dependency_schema_url.as_str(),
        metadata.dependency_registry_path.as_str(),
        metadata.rust_crate_version.as_str(),
        metadata.rust_crate_schema_url.as_str(),
    );
    if actual != expected
        || manifest_schema_url != metadata.registry_schema_url
        || dependency_schema_url != metadata.dependency_schema_url
        || dependency_registry_path != metadata.dependency_registry_path
    {
        bail!("telemetry schema-version metadata does not exactly match the pinned registry");
    }
    if metadata.dependency_registry_revision != "89aae438b3b3b0a8dd33003c9d70592baf7dbd0d"
        || vendored_registry_checksum(root, &metadata.dependency_registry_path)?
            != metadata.dependency_registry_checksum
    {
        bail!("vendored semantic-conventions revision/checksum does not match registry/rust.toml");
    }
    if (
        metadata.adoption.upstream_available.as_str(),
        metadata.adoption.upstream_absent.as_str(),
        metadata.adoption.wire_name_change.as_str(),
        metadata.adoption.schema_version_change.as_str(),
    ) != (
        "reexport",
        "local_pin_with_schema_version",
        "forbidden",
        "required",
    ) {
        bail!("telemetry standard-adoption policy is not the required structured policy");
    }
    validate_rust_crate_pin(root, &metadata)?;
    validate_weaver_platform_matrix(root)?;
    let upstream_names = metadata.standard_upstream.values().collect::<BTreeSet<_>>();
    let local_names = metadata.standard_local.values().collect::<BTreeSet<_>>();
    if upstream_names.len() != metadata.standard_upstream.len()
        || local_names.len() != metadata.standard_local.len()
        || !upstream_names.is_disjoint(&local_names)
    {
        bail!("duplicate standard attribute wire name in registry/rust.toml");
    }
    for constant in &metadata.root_standard_reexports {
        if !metadata.standard_upstream.contains_key(constant)
            && !metadata.standard_local.contains_key(constant)
        {
            bail!("unknown root standard re-export {constant}");
        }
    }
    Ok(())
}

fn validate_rust_crate_pin(root: &Path, metadata: &RustRegistryMetadata) -> Result<()> {
    let workspace: toml::Value = toml::from_str(&fs::read_to_string(root.join("Cargo.toml"))?)?;
    let dependency_version = workspace
        .get("workspace")
        .and_then(|value| value.get("dependencies"))
        .and_then(|value| value.get("opentelemetry-semantic-conventions"))
        .and_then(|value| value.get("version"))
        .and_then(toml::Value::as_str);
    if dependency_version != Some(metadata.rust_crate_version.as_str()) {
        bail!("workspace semantic-conventions dependency does not match registry/rust.toml");
    }
    let lockfile: toml::Value = toml::from_str(&fs::read_to_string(root.join("Cargo.lock"))?)?;
    let locked = lockfile
        .get("package")
        .and_then(toml::Value::as_array)
        .and_then(|packages| {
            packages.iter().find(|package| {
                package.get("name").and_then(toml::Value::as_str)
                    == Some("opentelemetry-semantic-conventions")
            })
        })
        .ok_or_else(|| anyhow::anyhow!("Cargo.lock has no semantic-conventions package"))?;
    let locked_version = locked.get("version").and_then(toml::Value::as_str);
    let locked_checksum = locked.get("checksum").and_then(toml::Value::as_str);
    if locked_version != metadata.rust_crate_version.strip_prefix('=')
        || locked_checksum != Some(metadata.rust_crate_checksum.as_str())
    {
        bail!("Cargo.lock semantic-conventions version/checksum does not match registry/rust.toml");
    }
    Ok(())
}

fn vendored_registry_checksum(root: &Path, relative: &str) -> Result<String> {
    let directory = root.join(relative);
    let mut hasher = Sha256::new();
    for entry in crate::fs_util::read_dir_sorted(&directory)? {
        let path = entry.path();
        if path.is_dir() {
            bail!("vendored semantic-conventions snapshot must be a flat directory");
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow::anyhow!("non-UTF-8 vendored semantic-conventions filename"))?;
        hasher.update(name.as_bytes());
        hasher.update([0]);
        hasher.update(fs::read(&path)?);
    }
    let mut checksum = String::with_capacity(64);
    for byte in hasher.finalize() {
        write!(&mut checksum, "{byte:02x}")?;
    }
    Ok(checksum)
}

fn validate_weaver_platform_matrix(root: &Path) -> Result<()> {
    let lock: toml::Value = toml::from_str(&fs::read_to_string(root.join("mise.lock"))?)?;
    let weaver = lock
        .get("tools")
        .and_then(|value| value.get("github:open-telemetry/weaver"))
        .and_then(toml::Value::as_array)
        .and_then(|entries| entries.first())
        .and_then(toml::Value::as_table)
        .ok_or_else(|| anyhow::anyhow!("mise.lock has no Weaver platform matrix"))?;
    let version = weaver
        .get("version")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("mise.lock has no Weaver version"))?;
    let url_version_prefix = format!("/v{version}/weaver-");
    let expected = [
        "linux-arm64",
        "linux-arm64-musl",
        "linux-x64",
        "linux-x64-baseline",
        "linux-x64-musl",
        "linux-x64-musl-baseline",
        "macos-arm64",
        "macos-x64",
        "macos-x64-baseline",
    ];
    let platforms = weaver
        .iter()
        .filter_map(|(name, artifact)| name.strip_prefix("platforms.").map(|name| (name, artifact)))
        .collect::<BTreeMap<_, _>>();
    let actual = platforms.keys().copied().collect::<BTreeSet<_>>();
    if actual != expected.into_iter().collect() {
        bail!("mise.lock Weaver platform matrix is incomplete or has unexpected entries");
    }
    for (platform, artifact) in platforms {
        let checksum = artifact.get("checksum").and_then(toml::Value::as_str);
        let url = artifact.get("url").and_then(toml::Value::as_str);
        if !checksum.is_some_and(|value| value.starts_with("sha256:") && value.len() == 71)
            || !url.is_some_and(|value| value.contains(&url_version_prefix))
        {
            bail!("mise.lock Weaver artifact {platform} is not checksum/version pinned");
        }
    }
    Ok(())
}

fn yaml_required<'a>(value: &'a YamlValue, key: &str) -> Result<&'a YamlValue> {
    value
        .get(key)
        .ok_or_else(|| anyhow::anyhow!("registry entry is missing `{key}`"))
}

fn yaml_string<'a>(value: &'a YamlValue, key: &str) -> Result<&'a str> {
    yaml_required(value, key)?
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("registry `{key}` must be a string"))
}

fn yaml_sequence<'a>(value: &'a YamlValue, key: &str) -> Option<&'a [YamlValue]> {
    value.get(key)?.as_sequence().map(Vec::as_slice)
}

fn rust_constant(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn rust_field_identifier(value: &str) -> String {
    let mut identifier = String::from("field_");
    identifier.extend(value.chars().map(|character| {
        if character.is_ascii_alphanumeric() {
            character.to_ascii_lowercase()
        } else {
            '_'
        }
    }));
    identifier
}

fn rust_pascal(value: &str) -> String {
    let mut result = String::new();
    for word in value.split(|character: char| !character.is_ascii_alphanumeric()) {
        if word.is_empty() {
            continue;
        }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            result.push(first.to_ascii_uppercase());
            result.extend(chars);
        }
    }
    result.replace("V1alpha", "V1Alpha")
}

fn validate_legacy_namespaces(root: &Path) -> Result<()> {
    let crates = root.join("crates");
    let mut violations = Vec::new();
    collect_rust_files(&crates, &mut violations, root)?;
    if violations.is_empty() {
        Ok(())
    } else {
        bail!(
            "unapproved legacy telemetry namespace literals:\n  {}",
            violations.join("\n  ")
        )
    }
}

fn collect_rust_files(dir: &Path, violations: &mut Vec<String>, root: &Path) -> Result<()> {
    for entry in crate::fs_util::read_dir_sorted(dir)
        .with_context(|| format!("reading {}", dir.display()))?
    {
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, violations, root)?;
            continue;
        }
        if path.extension().and_then(|value| value.to_str()) != Some("rs") {
            continue;
        }
        let relative = path.strip_prefix(root).unwrap_or(&path);
        let relative_text = relative.to_string_lossy();
        let source =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let syntax = syn::parse_file(&source)
            .with_context(|| format!("parsing {} for telemetry namespaces", path.display()))?;
        let mut scanner = NamespaceScanner::new(&relative_text);
        scanner.visit_file(&syntax);
        violations.extend(
            scanner
                .violations
                .into_iter()
                .map(|(line, literal)| format!("{}:{line}: {literal}", relative.display())),
        );
    }
    Ok(())
}

fn is_project_namespace(literal: &str) -> bool {
    (literal.starts_with("jackin.") || literal.starts_with("parallax."))
        && literal
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "._-".contains(character))
}

fn is_schema_constant_name(name: Option<&String>) -> bool {
    name.is_some_and(|name| {
        name.chars()
            .any(|character| character.is_ascii_alphabetic())
            && name
                .chars()
                .all(|character| !character.is_ascii_lowercase())
    })
}

struct NamespaceScanner<'a> {
    path: &'a str,
    context: String,
    bindings: NamespaceBindings,
    violations: BTreeSet<(usize, String)>,
}

#[derive(Default)]
struct NamespaceBindings {
    imports: BTreeMap<String, Vec<String>>,
    ambiguous_imports: BTreeSet<String>,
    globs: BTreeSet<Vec<String>>,
    local_types: BTreeSet<String>,
    constants: BTreeMap<String, Vec<syn::Expr>>,
    locals: BTreeMap<(String, String), Vec<syn::Expr>>,
    telemetry_crate: bool,
}

#[derive(Debug)]
enum AttrKeySource {
    Literal(String),
    Schema,
    ValidatedForward,
    Unknown,
}

impl<'a> NamespaceScanner<'a> {
    fn new(path: &'a str) -> Self {
        Self {
            path,
            context: String::from("file"),
            bindings: NamespaceBindings {
                telemetry_crate: path.starts_with("crates/services/jackin-telemetry/"),
                ..NamespaceBindings::default()
            },
            violations: BTreeSet::new(),
        }
    }

    fn inspect(&mut self, literal: &str, line: usize) {
        if is_project_namespace(literal) {
            self.violations.insert((line, literal.to_owned()));
        }
    }

    fn reject_attr_key(&mut self, expression: &syn::Expr) {
        self.violations.insert((
            expression.span().start().line,
            format!("unresolved telemetry Attr.key expression: {expression:?}"),
        ));
    }

    fn is_telemetry_attr_path(&self, path: &syn::Path) -> bool {
        let Some(segments) = self.bindings.expand_path(path) else {
            return false;
        };
        if segments.last().is_none_or(|segment| segment != "Attr") {
            return false;
        }
        self.bindings.is_telemetry_root(&segments)
            || (segments.as_slice() == ["Attr"]
                && !self.bindings.local_types.contains("Attr")
                && self.bindings.has_telemetry_glob())
    }

    fn resolve_attr_key(
        &self,
        expression: &syn::Expr,
        seen: &mut BTreeSet<String>,
    ) -> AttrKeySource {
        match expression {
            syn::Expr::Lit(literal) => match &literal.lit {
                syn::Lit::Str(value) => AttrKeySource::Literal(value.value()),
                syn::Lit::ByteStr(value) => String::from_utf8(value.value())
                    .map(AttrKeySource::Literal)
                    .unwrap_or(AttrKeySource::Unknown),
                _ => AttrKeySource::Unknown,
            },
            syn::Expr::Path(path) => {
                let Some(segments) = self.bindings.expand_path(&path.path) else {
                    return AttrKeySource::Unknown;
                };
                if self.bindings.is_schema_key_path(&segments) {
                    return AttrKeySource::Schema;
                }
                if segments.len() != 1 {
                    return AttrKeySource::Unknown;
                }
                let name = segments[0].clone();
                let local_key = (self.context.clone(), name.clone());
                let local = self.bindings.locals.get(&local_key);
                let constant = self.bindings.constants.get(&name);
                let (binding_key, expressions) = if let Some(values) = local {
                    (format!("local:{}:{name}", self.context), values)
                } else if let Some(values) = constant {
                    (format!("const:{name}"), values)
                } else {
                    return AttrKeySource::Unknown;
                };
                if expressions.len() != 1 || !seen.insert(binding_key.clone()) {
                    return AttrKeySource::Unknown;
                }
                let result = self.resolve_attr_key(&expressions[0], seen);
                seen.remove(&binding_key);
                result
            }
            syn::Expr::Reference(reference) => self.resolve_attr_key(&reference.expr, seen),
            syn::Expr::Paren(parenthesized) => self.resolve_attr_key(&parenthesized.expr, seen),
            syn::Expr::Group(group) => self.resolve_attr_key(&group.expr, seen),
            syn::Expr::Field(field) => {
                let syn::Member::Named(member) = &field.member else {
                    return AttrKeySource::Unknown;
                };
                let syn::Expr::Path(base) = field.base.as_ref() else {
                    return AttrKeySource::Unknown;
                };
                let base = base
                    .path
                    .segments
                    .last()
                    .map(|segment| segment.ident.to_string());
                match (base.as_deref(), member.to_string().as_str()) {
                    (Some("attr"), "key")
                        if self.path == "crates/services/jackin-telemetry/src/event.rs"
                            && self.context == "fn:emit_event" =>
                    {
                        AttrKeySource::ValidatedForward
                    }
                    (Some("requirement"), "name")
                        if self.path == "crates/services/jackin-telemetry/src/event/tests.rs" =>
                    {
                        AttrKeySource::Schema
                    }
                    _ => AttrKeySource::Unknown,
                }
            }
            syn::Expr::Index(index) => {
                let syn::Expr::Path(path) = index.expr.as_ref() else {
                    return AttrKeySource::Unknown;
                };
                self.bindings
                    .expand_path(&path.path)
                    .filter(|path| self.bindings.is_schema_all_keys_path(path))
                    .map_or(AttrKeySource::Unknown, |_| AttrKeySource::Schema)
            }
            syn::Expr::Macro(invocation) => {
                if invocation.mac.path.is_ident("concat") {
                    let parser = syn::punctuated::Punctuated::<
                        syn::LitStr,
                        syn::Token![,],
                    >::parse_terminated;
                    return parser
                        .parse2(invocation.mac.tokens.clone())
                        .map(|parts| {
                            AttrKeySource::Literal(parts.iter().map(syn::LitStr::value).collect())
                        })
                        .unwrap_or(AttrKeySource::Unknown);
                }
                if invocation.mac.path.is_ident("stringify") {
                    return AttrKeySource::Literal(
                        invocation
                            .mac
                            .tokens
                            .to_string()
                            .chars()
                            .filter(|character| !character.is_whitespace())
                            .collect(),
                    );
                }
                AttrKeySource::Unknown
            }
            _ => AttrKeySource::Unknown,
        }
    }

    fn inspect_attr_key(&mut self, expression: &syn::Expr) {
        match self.resolve_attr_key(expression, &mut BTreeSet::new()) {
            AttrKeySource::Literal(value) => self.inspect(&value, expression.span().start().line),
            AttrKeySource::Schema | AttrKeySource::ValidatedForward => {}
            AttrKeySource::Unknown => self.reject_attr_key(expression),
        }
    }
}

impl NamespaceBindings {
    fn from_file(file: &syn::File, telemetry_crate: bool) -> Self {
        let mut bindings = Self {
            telemetry_crate,
            ..Self::default()
        };
        let mut collector = NamespaceBindingCollector {
            bindings: &mut bindings,
            context: String::from("file"),
        };
        collector.visit_file(file);
        drop(collector);
        bindings
    }

    fn is_telemetry_root(&self, path: &[String]) -> bool {
        path.first().is_some_and(|root| {
            root == "jackin_telemetry"
                || (self.telemetry_crate
                    && matches!(root.as_str(), "crate" | "self" | "super" | "schema"))
        })
    }

    fn is_schema_key_path(&self, path: &[String]) -> bool {
        self.is_schema_all_keys_path(path)
            || (self.is_telemetry_root(path)
                && is_schema_constant_name(path.last())
                && path.windows(2).any(|pair| pair == ["schema", "attrs"])
                && (path.len() == 4
                    || (path.len() == 5
                        && path.get(3).is_some_and(|segment| segment == "std_attrs"))))
    }

    fn is_schema_all_keys_path(&self, path: &[String]) -> bool {
        self.is_telemetry_root(path)
            && (path.windows(2).any(|pair| pair == ["schema", "ALL_KEYS"])
                || path
                    .windows(3)
                    .any(|triple| triple == ["schema", "attrs", "ALL_KEYS"])
                || path
                    .windows(4)
                    .any(|quad| quad == ["schema", "attrs", "std_attrs", "ALL_KEYS"]))
    }

    fn has_telemetry_glob(&self) -> bool {
        self.globs.iter().any(|path| {
            self.expand_segments(path.clone())
                .is_some_and(|expanded| self.is_telemetry_root(&expanded))
        })
    }

    fn expand_path(&self, path: &syn::Path) -> Option<Vec<String>> {
        let segments = path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>();
        self.expand_segments(segments)
    }

    fn expand_segments(&self, mut segments: Vec<String>) -> Option<Vec<String>> {
        let mut visited = BTreeSet::new();
        while let Some(first) = segments.first().cloned() {
            if self.ambiguous_imports.contains(&first) {
                return None;
            }
            let Some(replacement) = self.imports.get(&first) else {
                break;
            };
            if !visited.insert(first) {
                return None;
            }
            let mut expanded = replacement.clone();
            expanded.extend(segments.into_iter().skip(1));
            segments = expanded;
        }
        Some(segments)
    }
}

struct NamespaceBindingCollector<'a> {
    bindings: &'a mut NamespaceBindings,
    context: String,
}

impl NamespaceBindingCollector<'_> {
    fn register_import(&mut self, local: String, target: Vec<String>) {
        if local.is_empty() || target.is_empty() {
            return;
        }
        match self.bindings.imports.get(&local) {
            Some(previous) if previous != &target => {
                self.bindings.ambiguous_imports.insert(local);
            }
            Some(_) => {}
            None => {
                self.bindings.imports.insert(local, target);
            }
        }
    }

    fn add_use_tree(&mut self, prefix: Vec<String>, tree: &syn::UseTree) {
        match tree {
            syn::UseTree::Path(path) => {
                let mut nested = prefix;
                nested.push(path.ident.to_string());
                self.add_use_tree(nested, &path.tree);
            }
            syn::UseTree::Name(name) => {
                let ident = name.ident.to_string();
                let mut target = prefix;
                let local = if ident == "self" {
                    target.last().cloned().unwrap_or_default()
                } else {
                    target.push(ident.clone());
                    ident
                };
                self.register_import(local, target);
            }
            syn::UseTree::Rename(rename) => {
                let source = rename.ident.to_string();
                let mut target = prefix;
                if source != "self" {
                    target.push(source);
                }
                self.register_import(rename.rename.to_string(), target);
            }
            syn::UseTree::Group(group) => {
                for item in &group.items {
                    self.add_use_tree(prefix.clone(), item);
                }
            }
            syn::UseTree::Glob(_) => {
                self.bindings.globs.insert(prefix);
            }
        }
    }
}

impl<'ast> syn::visit::Visit<'ast> for NamespaceBindingCollector<'_> {
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        self.add_use_tree(Vec::new(), &item.tree);
        syn::visit::visit_item_use(self, item);
    }

    fn visit_item_const(&mut self, item: &'ast syn::ItemConst) {
        self.bindings
            .constants
            .entry(item.ident.to_string())
            .or_default()
            .push(item.expr.as_ref().clone());
        syn::visit::visit_item_const(self, item);
    }

    fn visit_item_static(&mut self, item: &'ast syn::ItemStatic) {
        self.bindings
            .constants
            .entry(item.ident.to_string())
            .or_default()
            .push(item.expr.as_ref().clone());
        syn::visit::visit_item_static(self, item);
    }

    fn visit_item_type(&mut self, item: &'ast syn::ItemType) {
        if let syn::Type::Path(path) = item.ty.as_ref()
            && path.qself.is_none()
        {
            self.register_import(
                item.ident.to_string(),
                path.path
                    .segments
                    .iter()
                    .map(|segment| segment.ident.to_string())
                    .collect(),
            );
        } else {
            self.bindings.local_types.insert(item.ident.to_string());
        }
        syn::visit::visit_item_type(self, item);
    }

    fn visit_item_struct(&mut self, item: &'ast syn::ItemStruct) {
        self.bindings.local_types.insert(item.ident.to_string());
        syn::visit::visit_item_struct(self, item);
    }

    fn visit_item_enum(&mut self, item: &'ast syn::ItemEnum) {
        self.bindings.local_types.insert(item.ident.to_string());
        syn::visit::visit_item_enum(self, item);
    }

    fn visit_item_union(&mut self, item: &'ast syn::ItemUnion) {
        self.bindings.local_types.insert(item.ident.to_string());
        syn::visit::visit_item_union(self, item);
    }

    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        let previous = std::mem::replace(&mut self.context, format!("fn:{}", item.sig.ident));
        syn::visit::visit_item_fn(self, item);
        self.context = previous;
    }

    fn visit_local(&mut self, local: &'ast syn::Local) {
        if let (syn::Pat::Ident(pattern), Some(initializer)) =
            (local.pat.as_ref(), local.init.as_ref())
        {
            self.bindings
                .locals
                .entry((self.context.clone(), pattern.ident.to_string()))
                .or_default()
                .push(initializer.expr.as_ref().clone());
        }
        syn::visit::visit_local(self, local);
    }
}

impl<'ast> syn::visit::Visit<'ast> for NamespaceScanner<'_> {
    fn visit_file(&mut self, file: &'ast syn::File) {
        self.bindings = NamespaceBindings::from_file(file, self.bindings.telemetry_crate);
        syn::visit::visit_file(self, file);
    }

    fn visit_item_static(&mut self, item: &'ast syn::ItemStatic) {
        let previous = std::mem::replace(&mut self.context, format!("static:{}", item.ident));
        syn::visit::visit_item_static(self, item);
        self.context = previous;
    }

    fn visit_item_const(&mut self, item: &'ast syn::ItemConst) {
        let previous = std::mem::replace(&mut self.context, format!("const:{}", item.ident));
        syn::visit::visit_item_const(self, item);
        self.context = previous;
    }

    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        let previous = std::mem::replace(&mut self.context, format!("fn:{}", item.sig.ident));
        syn::visit::visit_item_fn(self, item);
        self.context = previous;
    }

    fn visit_macro(&mut self, invocation: &'ast syn::Macro) {
        let tokens = invocation.tokens.clone();
        if let Ok(expression) = syn::parse2::<syn::Expr>(tokens.clone()) {
            self.visit_expr(&expression);
        } else if let Ok(block) = syn::parse2::<syn::Block>(tokens) {
            self.visit_block(&block);
        }
    }

    fn visit_expr_struct(&mut self, expression: &'ast syn::ExprStruct) {
        if self.is_telemetry_attr_path(&expression.path) {
            for field in &expression.fields {
                if matches!(&field.member, syn::Member::Named(member) if member == "key") {
                    self.inspect_attr_key(&field.expr);
                }
            }
        }
        syn::visit::visit_expr_struct(self, expression);
    }
}

#[cfg(test)]
mod tests;
