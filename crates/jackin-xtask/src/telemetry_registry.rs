//! Closed telemetry-registry validation and generation gate.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use clap::Args;
use serde::Deserialize;
use serde_yaml_ng::Value as YamlValue;
use sha2::{Digest as _, Sha256};
use syn::parse::Parser as _;
use syn::spanned::Spanned as _;
use syn::visit::Visit as _;

use crate::docs::repo_root;

mod source_policy;
use source_policy::{
    AsyncScopeGuardScanner, SpawnDeclarations, SpawnTypeResolver, WorkspaceSpawnTypes,
    spawn_receiver_type,
};

// Shrink-only migration inventories. A new file never joins these lists: it
// must use the governed facade/spawn helpers from its first commit.
const RAW_SPAWN_ALLOWLIST: &[&str] = &[];

const RAW_SCOPED_THREAD_ALLOWLIST: &[(&str, &str)] = &[(
    "crates/jackin-process/src/lib.rs",
    "T0 process transport cannot depend on peer T0 telemetry facade",
)];

const RAW_TRACING_ALLOWLIST: &[&str] = &[];

const NON_TELEMETRY_EXEMPTIONS: &[(&str, &str, &str)] = &[
    (
        "crates/jackin-core/src/constants.rs",
        "const:MANIFEST_FILENAME",
        "jackin.role.toml",
    ),
    (
        "crates/jackin-manifest/src/repo_contract.rs",
        "const:LABEL_PUBLISHED_IMAGE_CONSTRUCT_VERSION",
        "jackin.construct.version",
    ),
    (
        "crates/jackin-manifest/src/repo_contract.rs",
        "const:LABEL_PUBLISHED_IMAGE_ROLE_GIT_SHA",
        "jackin.role.git.sha",
    ),
    (
        "crates/jackin-runtime/src/runtime/snapshot.rs",
        "fn:socket_path",
        "jackin.sock",
    ),
    (
        "crates/jackin-runtime/src/runtime/discovery.rs",
        "fn:list_running_agent_display_names",
        "jackin.display.name",
    ),
    (
        "crates/jackin-runtime/src/runtime/launch/launch_dind.rs",
        "fn:prewarmed_dind_state_is_live",
        "jackin.kind",
    ),
    (
        "crates/jackin-runtime/src/runtime/launch/launch_dind.rs",
        "fn:adopt_prewarmed_dind_sidecar",
        "jackin.kind",
    ),
    (
        "crates/jackin-runtime/src/runtime/cleanup.rs",
        "fn:collect_labeled_dind",
        "jackin.kind",
    ),
    (
        "crates/jackin-runtime/src/runtime/naming.rs",
        "const:LABEL_ROLE_KEY",
        "jackin.role",
    ),
    (
        "crates/jackin-runtime/src/runtime/naming.rs",
        "const:LABEL_IMAGE_KEY",
        "jackin.image",
    ),
    (
        "crates/jackin-image/src/naming.rs",
        "const:LABEL_IMAGE_CONSTRUCT",
        "jackin.construct.image",
    ),
    (
        "crates/jackin-image/src/naming.rs",
        "const:LABEL_IMAGE_RECIPE_HASH",
        "jackin.image.recipe.hash",
    ),
    (
        "crates/jackin-image/src/naming.rs",
        "const:LABEL_IMAGE_RECIPE_VERSION",
        "jackin.image.recipe.version",
    ),
    (
        "crates/jackin-image/src/naming.rs",
        "const:LABEL_IMAGE_AGENT_VERSION_PREFIX",
        "jackin.agent",
    ),
    (
        "crates/jackin-image/src/naming.rs",
        "const:LABEL_IMAGE_CAPSULE_VERSION",
        "jackin.capsule.version",
    ),
    (
        "crates/jackin-image/src/naming.rs",
        "const:LABEL_IMAGE_MANIFEST_VERSION",
        "jackin.manifest.version",
    ),
];

const TELEMETRY_NEGATIVE_TEST_EXEMPTIONS: &[(&str, &str, &str)] = &[
    (
        "crates/jackin-diagnostics/src/tests.rs",
        "fn:conformance_no_prohibited_keys_or_bracket_bodies_on_records",
        "parallax.run.id",
    ),
    (
        "crates/jackin-diagnostics/src/tests.rs",
        "fn:conformance_no_prohibited_keys_or_bracket_bodies_on_records",
        "jackin.component",
    ),
    (
        "crates/jackin-diagnostics/src/tests.rs",
        "fn:conformance_has_no_legacy_screen_span_attributes",
        "jackin.screen.name",
    ),
    (
        "crates/jackin-diagnostics/src/observability/otlp/tests.rs",
        "fn:resource_matrix_has_exact_allowlist_and_ignores_secret_env_injection",
        "parallax.run.id",
    ),
    (
        "crates/jackin-otlp-testbed/src/tests.rs",
        "fn:namespace_detector_rejects_synthetic_legacy_attribute",
        "jackin.synthetic",
    ),
];

const NAMESPACE_TEST_FIXTURES: &[(&str, &str)] = &[
    ("crates/jackin/src/app/context/tests.rs", "jackin.role.toml"),
    (
        "crates/jackin/src/role_authoring/tests.rs",
        "jackin.role.toml",
    ),
    (
        "crates/jackin/tests/agent_validation.rs",
        "jackin.role.toml",
    ),
    ("crates/jackin/tests/amp_launch.rs", "jackin.role.toml"),
    ("crates/jackin/tests/codex_launch.rs", "jackin.role.toml"),
    (
        "crates/jackin/tests/dind_e2e/fixtures.rs",
        "jackin.role.toml",
    ),
    (
        "crates/jackin/tests/migration_fixtures.rs",
        "jackin.role.toml",
    ),
    ("crates/jackin/tests/role_cli.rs", "jackin.role.toml"),
    ("crates/jackin/tests/validate_cli.rs", "jackin.role.toml"),
    ("crates/jackin-capsule/src/socket/tests.rs", "jackin.sock"),
    (
        "crates/jackin-image/src/derived_image/tests.rs",
        "jackin.role.toml",
    ),
    (
        "crates/jackin-image/src/image_decision/tests.rs",
        "jackin.role.toml",
    ),
    (
        "crates/jackin-image/src/image_recipe/tests.rs",
        "jackin.role.toml",
    ),
    (
        "crates/jackin-instance/src/auth/tests.rs",
        "jackin.role.toml",
    ),
    ("crates/jackin-instance/src/tests.rs", "jackin.role.toml"),
    (
        "crates/jackin-manifest/src/manifest/tests.rs",
        "jackin.role.toml",
    ),
    (
        "crates/jackin-manifest/src/migrations/tests.rs",
        "jackin.role.toml",
    ),
    (
        "crates/jackin-manifest/src/repo/tests.rs",
        "jackin.role.toml",
    ),
    (
        "crates/jackin-manifest/src/validate/tests.rs",
        "jackin.role.toml",
    ),
    (
        "crates/jackin-runtime/benches/launch_pipeline.rs",
        "jackin.role.toml",
    ),
    (
        "crates/jackin-runtime/src/runtime/cleanup/tests.rs",
        "jackin.kind",
    ),
    (
        "crates/jackin-runtime/src/runtime/cleanup/tests.rs",
        "jackin.prewarm",
    ),
    (
        "crates/jackin-runtime/src/runtime/discovery/tests.rs",
        "jackin.display.name",
    ),
    (
        "crates/jackin-runtime/src/runtime/image/tests.rs",
        "jackin.role.toml",
    ),
    (
        "crates/jackin-runtime/src/runtime/launch/launch_pipeline/launch_phases/tests.rs",
        "jackin.role.toml",
    ),
    (
        "crates/jackin-runtime/src/runtime/launch/launch_pipeline/tests.rs",
        "jackin.role.toml",
    ),
    (
        "crates/jackin-runtime/src/runtime/launch/tests.rs",
        "jackin.kind",
    ),
    (
        "crates/jackin-runtime/src/runtime/launch/tests.rs",
        "jackin.prewarm",
    ),
    (
        "crates/jackin-runtime/src/runtime/launch/tests.rs",
        "jackin.role.toml",
    ),
    (
        "crates/jackin-runtime/src/runtime/repo_cache/tests.rs",
        "jackin.role.toml",
    ),
    ("crates/jackin-test-support/src/seed.rs", "jackin.role.toml"),
    ("crates/jackin-xtask/src/telemetry_registry.rs", "jackin."),
    ("crates/jackin-xtask/src/telemetry_registry.rs", "parallax."),
    (
        "crates/jackin-xtask/src/telemetry_registry/tests.rs",
        "jackin.unregistered.field",
    ),
    (
        "crates/jackin-xtask/src/telemetry_registry/tests.rs",
        "parallax.unregistered",
    ),
    (
        "crates/jackin-xtask/src/telemetry_registry/tests.rs",
        "jackin.unregistered",
    ),
    (
        "crates/jackin-xtask/src/telemetry_registry/tests.rs",
        "parallax.bad",
    ),
    (
        "crates/jackin-xtask/src/telemetry_registry/tests.rs",
        "jackin.state",
    ),
    (
        "crates/jackin-xtask/src/telemetry_registry/tests.rs",
        "jackin.bad",
    ),
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

struct SourcePolicyScanner<'a> {
    path: &'a str,
    violations: BTreeSet<(usize, &'static str)>,
    spawn_aliases: BTreeSet<String>,
    spawn_module_aliases: BTreeMap<String, String>,
    spawn_receivers: BTreeSet<String>,
    spawn_type_resolver: SpawnTypeResolver,
    spawn_fields: BTreeSet<String>,
    spawn_factories: BTreeSet<String>,
}

impl<'a> SourcePolicyScanner<'a> {
    fn new(path: &'a str, syntax: &syn::File, workspace: &WorkspaceSpawnTypes) -> Self {
        let declarations = SpawnDeclarations::collect(path, syntax, workspace);
        Self {
            path,
            violations: BTreeSet::new(),
            spawn_aliases: BTreeSet::new(),
            spawn_module_aliases: BTreeMap::new(),
            spawn_receivers: BTreeSet::new(),
            spawn_type_resolver: declarations.resolver,
            spawn_fields: declarations.fields,
            spawn_factories: declarations.factories,
        }
    }

    fn allows_spawn(&self) -> bool {
        self.path == "crates/jackin-telemetry/src/spawn.rs"
            || self.path.starts_with("crates/jackin-otlp-testbed/")
            || RAW_SPAWN_ALLOWLIST.contains(&self.path)
    }

    fn allows_telemetry_apis(&self) -> bool {
        self.path.starts_with("crates/jackin-telemetry/")
            || self.path.starts_with("crates/jackin-diagnostics/")
    }

    fn path_name(path: &syn::Path) -> String {
        path.segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>()
            .join("::")
    }

    fn reject(&mut self, span: proc_macro2::Span, message: &'static str) {
        self.violations.insert((span.start().line, message));
    }

    fn raw_spawn_path(name: &str) -> bool {
        matches!(
            name,
            "tokio::spawn"
                | "tokio::task::spawn"
                | "tokio::task::spawn_blocking"
                | "tokio::task::spawn_local"
                | "std::thread::spawn"
                | "thread::spawn"
        )
    }

    fn spawn_module_path(name: &str) -> bool {
        matches!(name, "tokio" | "tokio::task" | "std::thread" | "thread")
    }

    fn resolved_spawn_path(&self, name: &str) -> String {
        let Some((head, tail)) = name.split_once("::") else {
            return name.to_owned();
        };
        self.spawn_module_aliases
            .get(head)
            .map_or_else(|| name.to_owned(), |module| format!("{module}::{tail}"))
    }

    fn typed_spawn_receiver(&self, pat: &syn::Pat, ty: &syn::Type) -> Option<String> {
        if !spawn_receiver_type(ty, &self.spawn_type_resolver) {
            return None;
        }
        match pat {
            syn::Pat::Ident(binding) => Some(binding.ident.to_string()),
            syn::Pat::Reference(reference) => match reference.pat.as_ref() {
                syn::Pat::Ident(binding) => Some(binding.ident.to_string()),
                _ => None,
            },
            _ => None,
        }
    }

    fn collect_spawn_imports(&mut self, tree: &syn::UseTree, prefix: &mut Vec<String>) {
        match tree {
            syn::UseTree::Path(path) => {
                prefix.push(path.ident.to_string());
                self.collect_spawn_imports(&path.tree, prefix);
                prefix.pop();
            }
            syn::UseTree::Name(name) => {
                prefix.push(name.ident.to_string());
                let source = prefix.join("::");
                if Self::raw_spawn_path(&self.resolved_spawn_path(&source)) {
                    self.spawn_aliases.insert(name.ident.to_string());
                }
                prefix.pop();
            }
            syn::UseTree::Rename(rename) => {
                prefix.push(rename.ident.to_string());
                let source = prefix.join("::").trim_end_matches("::self").to_owned();
                if Self::raw_spawn_path(&self.resolved_spawn_path(&source)) {
                    self.spawn_aliases.insert(rename.rename.to_string());
                } else if Self::spawn_module_path(&source) {
                    self.spawn_module_aliases
                        .insert(rename.rename.to_string(), source);
                }
                prefix.pop();
            }
            syn::UseTree::Group(group) => {
                for item in &group.items {
                    self.collect_spawn_imports(item, prefix);
                }
            }
            syn::UseTree::Glob(_) => {}
        }
    }

    fn spawn_method_receiver(&self, receiver: &syn::Expr) -> bool {
        match receiver {
            syn::Expr::Path(path) => path.path.segments.last().is_some_and(|segment| {
                let name = segment.ident.to_string();
                self.spawn_receivers.contains(&name)
                    || matches!(
                        name.as_str(),
                        "scope" | "s" | "tasks" | "join_set" | "handle" | "runtime"
                    )
            }),
            syn::Expr::Call(call) => match call.func.as_ref() {
                syn::Expr::Path(path) => {
                    let name = Self::path_name(&path.path);
                    name.ends_with("JoinSet::new")
                        || name.ends_with("Handle::current")
                        || name.ends_with("Builder::new")
                        || path.path.segments.last().is_some_and(|segment| {
                            self.spawn_factories.contains(&segment.ident.to_string())
                        })
                }
                _ => false,
            },
            syn::Expr::MethodCall(call) => {
                self.spawn_factories.contains(&call.method.to_string())
                    || self.spawn_method_receiver(&call.receiver)
            }
            syn::Expr::Field(field) => match &field.member {
                syn::Member::Named(name) => self.spawn_fields.contains(&name.to_string()),
                syn::Member::Unnamed(_) => false,
            },
            syn::Expr::Paren(paren) => self.spawn_method_receiver(&paren.expr),
            syn::Expr::Reference(reference) => self.spawn_method_receiver(&reference.expr),
            _ => false,
        }
    }
}

impl<'ast> syn::visit::Visit<'ast> for SourcePolicyScanner<'_> {
    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        self.collect_spawn_imports(&node.tree, &mut Vec::new());
        syn::visit::visit_item_use(self, node);
    }

    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let syn::Expr::Path(function) = node.func.as_ref() {
            let name = Self::path_name(&function.path);
            let resolved_name = self.resolved_spawn_path(&name);
            if !self.allows_spawn()
                && (Self::raw_spawn_path(&resolved_name)
                    || self.spawn_aliases.contains(&name)
                    || matches!(name.as_str(), "spawn_blocking" | "spawn_local"))
            {
                self.reject(node.span(), "unmanaged async/thread spawn");
            }
            if !self.allows_telemetry_apis()
                && matches!(
                    name.as_str(),
                    "opentelemetry::global::meter" | "global::meter"
                )
            {
                self.reject(node.span(), "raw OpenTelemetry meter construction");
            }
        }
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if node.method == "with_callback" {
            for argument in &node.args {
                let mut callback = ObservableCallbackScanner::default();
                callback.visit_expr(argument);
                for (span, violation) in callback.violations {
                    self.reject(span, violation);
                }
            }
        }
        if !self.allows_spawn()
            && (matches!(
                node.method.to_string().as_str(),
                "spawn_local" | "spawn_blocking"
            ) || node.method == "spawn" && self.spawn_method_receiver(&node.receiver))
        {
            let scoped_allowlisted = node.method == "spawn"
                && RAW_SCOPED_THREAD_ALLOWLIST
                    .iter()
                    .any(|(path, _reason)| *path == self.path)
                && matches!(node.receiver.as_ref(), syn::Expr::Path(path) if path.path.segments.last().is_some_and(|segment| matches!(segment.ident.to_string().as_str(), "scope" | "s")));
            if !scoped_allowlisted {
                self.reject(node.span(), "unmanaged async/thread spawn");
            }
        }
        if !self.allows_telemetry_apis() && node.method == "meter" {
            self.reject(node.span(), "raw OpenTelemetry meter construction");
        }
        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_local(&mut self, node: &'ast syn::Local) {
        if let syn::Pat::Type(typed) = &node.pat
            && let Some(receiver) = self.typed_spawn_receiver(&typed.pat, &typed.ty)
        {
            self.spawn_receivers.insert(receiver);
        }
        if let syn::Pat::Ident(binding) = &node.pat
            && let Some(initializer) = &node.init
        {
            if let syn::Expr::Path(path) = initializer.expr.as_ref() {
                let source = Self::path_name(&path.path);
                if Self::raw_spawn_path(&self.resolved_spawn_path(&source)) {
                    self.spawn_aliases.insert(binding.ident.to_string());
                }
            }
            if matches!(initializer.expr.as_ref(), syn::Expr::Call(call) if matches!(call.func.as_ref(), syn::Expr::Path(path) if {
                let source = Self::path_name(&path.path);
                source.ends_with("JoinSet::new") || source.ends_with("Handle::current") || source.ends_with("LocalSet::new")
            })) {
                self.spawn_receivers.insert(binding.ident.to_string());
            }
        }
        syn::visit::visit_local(self, node);
    }

    fn visit_signature(&mut self, node: &'ast syn::Signature) {
        for input in &node.inputs {
            if let syn::FnArg::Typed(typed) = input
                && let Some(receiver) = self.typed_spawn_receiver(&typed.pat, &typed.ty)
            {
                self.spawn_receivers.insert(receiver);
            }
        }
        syn::visit::visit_signature(self, node);
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        let name = Self::path_name(&node.path);
        if !self.allows_telemetry_apis()
            && !RAW_TRACING_ALLOWLIST.contains(&self.path)
            && matches!(
                name.as_str(),
                "tracing::event"
                    | "tracing::info"
                    | "tracing::warn"
                    | "tracing::error"
                    | "tracing::debug"
                    | "tracing::trace"
                    | "tracing::span"
                    | "tracing::info_span"
            )
        {
            self.reject(node.span(), "raw tracing call outside governed facade");
        }
        syn::visit::visit_macro(self, node);
    }

    fn visit_attribute(&mut self, node: &'ast syn::Attribute) {
        let name = Self::path_name(node.path());
        if !self.allows_telemetry_apis()
            && matches!(name.as_str(), "tracing::instrument" | "instrument")
        {
            self.reject(node.span(), "tracing instrument outside governed facade");
        }
        syn::visit::visit_attribute(self, node);
    }

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if node.sig.asyncness.is_some() {
            let mut scanner = AsyncScopeGuardScanner::for_signature(&node.sig);
            scanner.visit_block(&node.block);
            for (span, violation) in scanner.violations {
                self.reject(span, violation);
            }
        }
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_expr_async(&mut self, node: &'ast syn::ExprAsync) {
        let mut scanner = AsyncScopeGuardScanner::default();
        scanner.visit_block(&node.block);
        for (span, violation) in scanner.violations {
            self.reject(span, violation);
        }
        syn::visit::visit_expr_async(self, node);
    }
}

#[derive(Default)]
struct ObservableCallbackScanner {
    violations: Vec<(proc_macro2::Span, &'static str)>,
}

impl ObservableCallbackScanner {
    fn reject(&mut self, span: proc_macro2::Span) {
        self.violations
            .push((span, "observable callback performs blocking/runtime work"));
    }
}

impl<'ast> syn::visit::Visit<'ast> for ObservableCallbackScanner {
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let syn::Expr::Path(function) = node.func.as_ref() {
            let name = SourcePolicyScanner::path_name(&function.path);
            let snapshot_call = matches!(name.as_str(), "f64::from_bits" | "health::count");
            if !snapshot_call
                || name.starts_with("std::fs::")
                || name.starts_with("tokio::fs::")
                || name.starts_with("fs::")
            {
                self.reject(node.span());
            }
        }
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if matches!(
            node.method.to_string().as_str(),
            "lock"
                | "read"
                | "read_to_string"
                | "read_dir"
                | "write"
                | "open"
                | "connect"
                | "enter"
                | "entered"
                | "block_on"
        ) {
            self.reject(node.span());
        }
        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_expr_await(&mut self, node: &'ast syn::ExprAwait) {
        self.reject(node.span());
        syn::visit::visit_expr_await(self, node);
    }
}

#[cfg(test)]
fn source_policy_violations(path: &str, source: &str) -> Vec<&'static str> {
    source_policy_violations_for_files(&[(path, source)])
}

#[cfg(test)]
fn source_policy_violations_for_files(files: &[(&str, &str)]) -> Vec<&'static str> {
    let parsed = files
        .iter()
        .map(|(path, source)| {
            (
                (*path).to_owned(),
                syn::parse_file(source).expect("source-policy fixture must parse"),
            )
        })
        .collect::<Vec<_>>();
    let indexed = parsed
        .iter()
        .map(|(path, syntax)| (path.as_str(), syntax))
        .collect::<Vec<_>>();
    let workspace = WorkspaceSpawnTypes::collect(&indexed);
    let mut violations = Vec::new();
    for (path, syntax) in &parsed {
        let mut scanner = SourcePolicyScanner::new(path, syntax, &workspace);
        scanner.visit_file(syntax);
        violations.extend(scanner.violations.iter().map(|(_, violation)| *violation));
    }
    violations.sort_unstable();
    violations
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
            || text.starts_with("crates/jackin-xtask/")
            || text.starts_with("crates/jackin-dev/")
            || text.starts_with("crates/jackin-pr-trailers/")
            || text.contains("lookbook")
            || text.starts_with("crates/jackin-lints/")
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
        "crates/jackin-telemetry/registry",
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
    let registry = root.join("crates/jackin-telemetry/registry");
    let metadata = load_rust_metadata(&registry)?;
    let header_path = root.join("crates/jackin-telemetry/templates/rust.j2");
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

    let generated = vec![
        (
            "crates/jackin-telemetry/src/schema/attrs.rs".to_owned(),
            generate_attributes(&header, attributes, &metadata)?,
        ),
        (
            "crates/jackin-telemetry/src/schema/enums.rs".to_owned(),
            generate_enums(&header, attributes, &metadata)?,
        ),
        (
            "crates/jackin-telemetry/src/schema/events.rs".to_owned(),
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
            "crates/jackin-telemetry/src/event_emit.rs".to_owned(),
            generate_event_emitters(&header, &local_groups)?,
        ),
        (
            "crates/jackin-telemetry/src/schema/spans.rs".to_owned(),
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
            "crates/jackin-telemetry/src/schema/metrics.rs".to_owned(),
            generate_signal_constants(
                &header,
                &local_groups,
                attributes,
                "metric_name",
                SignalKind::Metric,
                &metadata,
            )?,
        ),
    ];
    generated
        .into_iter()
        .map(|(path, source)| Ok((path, format_generated_rust(&source)?)))
        .collect()
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
            SignalKind::Event => {}
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
            // `tracing` has no array Value variant. Keep the registered field in
            // metadata but let the synchronous governed processor attach its
            // typed value through `take_pending_event_arrays`.
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
        "crates/jackin-telemetry/registry",
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
    let registry = root.join("crates/jackin-telemetry/registry");
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
        "1.43.0",
        "https://jackin.tailrocks.com/telemetry/schemas/1.43.0",
        "1.43.0",
        "https://opentelemetry.io/schemas/1.43.0",
        "crates/jackin-telemetry/vendor/semconv-1.43.0",
        "=0.32.1",
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
            || !url.is_some_and(|value| value.contains("/v0.24.2/weaver-"))
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

#[cfg(test)]
fn contains_legacy_telemetry_name(path: &str, source: &str) -> bool {
    let fixture = format!("fn namespace_fixture() {{ {source} }}");
    let Ok(syntax) = syn::parse_file(&fixture) else {
        return false;
    };
    let mut scanner = NamespaceScanner::new(path);
    scanner.visit_file(&syntax);
    !scanner.violations.is_empty()
}

fn is_project_namespace(literal: &str) -> bool {
    (literal.starts_with("jackin.") || literal.starts_with("parallax."))
        && literal
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "._-".contains(character))
}

fn is_non_telemetry_name(path: &str, context: &str, literal: &str) -> bool {
    NON_TELEMETRY_EXEMPTIONS
        .iter()
        .any(|(exempt_path, exempt_context, exempt_literal)| {
            *exempt_path == path && *exempt_context == context && *exempt_literal == literal
        })
        || TELEMETRY_NEGATIVE_TEST_EXEMPTIONS.iter().any(
            |(exempt_path, exempt_context, exempt_literal)| {
                *exempt_path == path && *exempt_context == context && *exempt_literal == literal
            },
        )
        || (path == "crates/jackin-xtask/src/telemetry_registry.rs"
            && matches!(
                context,
                "const:NON_TELEMETRY_EXEMPTIONS"
                    | "const:TELEMETRY_NEGATIVE_TEST_EXEMPTIONS"
                    | "const:NAMESPACE_TEST_FIXTURES"
            )
            && NON_TELEMETRY_EXEMPTIONS
                .iter()
                .map(|(_, _, fixture_name)| fixture_name)
                .chain(
                    TELEMETRY_NEGATIVE_TEST_EXEMPTIONS
                        .iter()
                        .map(|(_, _, fixture_name)| fixture_name),
                )
                .chain(
                    NAMESPACE_TEST_FIXTURES
                        .iter()
                        .map(|(_, fixture_name)| fixture_name),
                )
                .any(|fixture_name| *fixture_name == literal))
        || NAMESPACE_TEST_FIXTURES
            .iter()
            .any(|(fixture_path, fixture_name)| *fixture_path == path && *fixture_name == literal)
}

struct NamespaceScanner<'a> {
    path: &'a str,
    context: String,
    violations: BTreeSet<(usize, String)>,
}

impl<'a> NamespaceScanner<'a> {
    fn new(path: &'a str) -> Self {
        Self {
            path,
            context: String::from("file"),
            violations: BTreeSet::new(),
        }
    }

    fn inspect(&mut self, literal: &str, line: usize) {
        if is_project_namespace(literal)
            && !is_non_telemetry_name(self.path, &self.context, literal)
        {
            self.violations.insert((line, literal.to_owned()));
        }
    }
}

impl<'ast> syn::visit::Visit<'ast> for NamespaceScanner<'_> {
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
    fn visit_lit_str(&mut self, literal: &'ast syn::LitStr) {
        self.inspect(&literal.value(), literal.span().start().line);
    }

    fn visit_lit_byte_str(&mut self, literal: &'ast syn::LitByteStr) {
        if let Ok(value) = String::from_utf8(literal.value()) {
            self.inspect(&value, literal.span().start().line);
        }
    }

    fn visit_macro(&mut self, invocation: &'ast syn::Macro) {
        if invocation.path.is_ident("concat") {
            let parser =
                syn::punctuated::Punctuated::<syn::LitStr, syn::Token![,]>::parse_terminated;
            if let Ok(parts) = parser.parse2(invocation.tokens.clone()) {
                let value = parts.iter().map(syn::LitStr::value).collect::<String>();
                self.inspect(&value, invocation.span().start().line);
            }
        } else if invocation.path.is_ident("stringify") {
            let value = invocation
                .tokens
                .to_string()
                .chars()
                .filter(|character| !character.is_whitespace())
                .collect::<String>();
            self.inspect(&value, invocation.span().start().line);
        }
    }
}

#[cfg(test)]
mod tests;
