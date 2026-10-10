// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use super::{
    PROHIBITED_TELEMETRY_MACROS, RAW_SCOPED_THREAD_ALLOWLIST, RAW_SPAWN_ALLOWLIST,
    RAW_TRACING_ALLOWLIST,
};
use proc_macro2::{Delimiter, TokenStream, TokenTree};
use syn::spanned::Spanned as _;
use syn::visit::Visit as _;

const RAW_TRACING_MACROS: &[&str] = &[
    "event",
    "info",
    "warn",
    "error",
    "debug",
    "trace",
    "span",
    "trace_span",
    "debug_span",
    "info_span",
    "warn_span",
    "error_span",
];

#[derive(Default)]
struct TelemetryImports {
    aliases: BTreeMap<String, BTreeSet<String>>,
    globs: BTreeSet<String>,
}

impl TelemetryImports {
    fn collect(syntax: &syn::File) -> Self {
        let mut imports = Self::default();
        imports.visit_file(syntax);
        imports
    }

    fn collect_tree(&mut self, tree: &syn::UseTree, prefix: &mut Vec<String>) {
        match tree {
            syn::UseTree::Path(path) => {
                prefix.push(path.ident.to_string());
                self.collect_tree(&path.tree, prefix);
                prefix.pop();
            }
            syn::UseTree::Name(name) => {
                if name.ident == "self" {
                    if let Some(local) = prefix.last() {
                        self.record_alias_segments(local.clone(), prefix);
                    }
                } else {
                    prefix.push(name.ident.to_string());
                    self.record_alias_segments(name.ident.to_string(), prefix);
                    prefix.pop();
                }
            }
            syn::UseTree::Rename(rename) => {
                prefix.push(rename.ident.to_string());
                let source = prefix.join("::").trim_end_matches("::self").to_owned();
                self.record_alias_path(&rename.rename.to_string(), &source);
                prefix.pop();
            }
            syn::UseTree::Group(group) => {
                for item in &group.items {
                    self.collect_tree(item, prefix);
                }
            }
            syn::UseTree::Glob(_) => {
                self.globs.insert(prefix.join("::"));
            }
        }
    }

    fn record_alias_segments(&mut self, local: String, source: &[String]) {
        self.record_alias_path(&local, &source.join("::"));
    }

    fn record_alias_path(&mut self, local: &str, source: &str) {
        self.aliases
            .entry(local.to_owned())
            .or_default()
            .insert(source.to_owned());
    }

    fn collect_macro_imports(&mut self, tokens: TokenStream) {
        let trees = tokens.into_iter().collect::<Vec<_>>();
        for (index, token) in trees.iter().enumerate() {
            if matches!(token, TokenTree::Ident(name) if name == "use") {
                for (end, token) in trees.iter().enumerate().skip(index + 1) {
                    if matches!(token, TokenTree::Punct(punct) if punct.as_char() == ';') {
                        let statement = trees[index..=end].iter().cloned().collect();
                        if let Ok(item) = syn::parse2::<syn::ItemUse>(statement) {
                            self.collect_tree(&item.tree, &mut Vec::new());
                        }
                        break;
                    }
                }
            }
            if let TokenTree::Group(group) = token {
                self.collect_macro_imports(group.stream());
            }
        }
    }

    fn paths(&self, path: &str) -> BTreeSet<String> {
        let mut pending = vec![path.to_owned()];
        let mut visited = BTreeSet::new();
        while let Some(candidate) = pending.pop() {
            if !visited.insert(candidate.clone()) {
                continue;
            }
            let (head, tail) = candidate
                .split_once("::")
                .map_or((candidate.as_str(), None), |(head, tail)| {
                    (head, Some(tail))
                });
            if let Some(targets) = self.aliases.get(head) {
                for target in targets {
                    pending.push(match tail {
                        Some(tail) => format!("{target}::{tail}"),
                        None => target.clone(),
                    });
                }
            }
        }
        visited
    }

    fn glob_imports(&self, module: &str) -> bool {
        self.globs
            .iter()
            .any(|glob| self.paths(glob).contains(module))
    }

    fn is_tracing_macro(&self, path: &str) -> bool {
        self.paths(path).iter().any(|candidate| {
            match candidate.split("::").collect::<Vec<_>>().as_slice() {
                ["tracing", macro_name] => RAW_TRACING_MACROS.contains(macro_name),
                [macro_name] => {
                    RAW_TRACING_MACROS.contains(macro_name) && self.glob_imports("tracing")
                }
                _ => false,
            }
        })
    }

    fn is_tracing_instrument(&self, path: &str) -> bool {
        self.paths(path).iter().any(|candidate| {
            candidate == "tracing::instrument"
                || candidate == "instrument" && self.glob_imports("tracing")
        })
    }

    fn is_raw_meter(&self, path: &str) -> bool {
        self.paths(path).iter().any(|candidate| {
            candidate == "opentelemetry::global::meter"
                || candidate == "global::meter" && self.glob_imports("opentelemetry")
                || candidate == "meter" && self.glob_imports("opentelemetry::global")
        })
    }
}

impl<'ast> syn::visit::Visit<'ast> for TelemetryImports {
    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        self.collect_tree(&node.tree, &mut Vec::new());
        syn::visit::visit_item_use(self, node);
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        self.collect_macro_imports(node.tokens.clone());
        syn::visit::visit_macro(self, node);
    }
}

#[derive(Clone, Default)]
pub(super) struct SpawnTypeResolver {
    aliases: BTreeMap<String, String>,
    crate_names: BTreeSet<String>,
    module: Vec<String>,
}

impl SpawnTypeResolver {
    fn resolves_path(&self, path: &syn::Path) -> bool {
        let Some(raw) = type_path_name(path) else {
            return false;
        };
        let mut name = canonical_path(&raw, &self.module, &self.crate_names);
        let mut visited = BTreeSet::new();
        while visited.insert(name.clone()) {
            if matches!(name.rsplit("::").next(), Some("Handle" | "JoinSet")) {
                return true;
            }
            let Some(target) = self.aliases.get(&name) else {
                break;
            };
            name.clone_from(target);
        }
        false
    }
}

pub(super) fn spawn_receiver_type(ty: &syn::Type, resolver: &SpawnTypeResolver) -> bool {
    match ty {
        syn::Type::Path(path) => resolver.resolves_path(&path.path),
        syn::Type::Reference(reference) => spawn_receiver_type(&reference.elem, resolver),
        syn::Type::Paren(paren) => spawn_receiver_type(&paren.elem, resolver),
        syn::Type::Group(group) => spawn_receiver_type(&group.elem, resolver),
        _ => false,
    }
}

#[derive(Default)]
pub(super) struct WorkspaceSpawnTypes {
    aliases: BTreeMap<String, String>,
    crate_names: BTreeSet<String>,
}

impl WorkspaceSpawnTypes {
    pub(super) fn collect(files: &[(&str, &syn::File)]) -> Self {
        let crate_names = files
            .iter()
            .filter_map(|(path, _)| source_module(path).and_then(|module| module.first().cloned()))
            .collect::<BTreeSet<_>>();
        let mut aliases = BTreeMap::new();
        for (path, syntax) in files {
            let Some(module) = source_module(path) else {
                continue;
            };
            let mut collector = SpawnTypeAliases {
                aliases: &mut aliases,
                crate_names: &crate_names,
                module,
            };
            collector.visit_file(syntax);
        }
        Self {
            aliases,
            crate_names,
        }
    }

    pub(super) fn resolver(&self, path: &str) -> SpawnTypeResolver {
        SpawnTypeResolver {
            aliases: self.aliases.clone(),
            crate_names: self.crate_names.clone(),
            module: source_module(path).unwrap_or_default(),
        }
    }
}

struct SpawnTypeAliases<'a> {
    aliases: &'a mut BTreeMap<String, String>,
    crate_names: &'a BTreeSet<String>,
    module: Vec<String>,
}

impl SpawnTypeAliases<'_> {
    fn collect_imports(&mut self, tree: &syn::UseTree, prefix: &mut Vec<String>) {
        match tree {
            syn::UseTree::Path(path) => {
                prefix.push(path.ident.to_string());
                self.collect_imports(&path.tree, prefix);
                prefix.pop();
            }
            syn::UseTree::Name(name) => {
                prefix.push(name.ident.to_string());
                self.record_alias(name.ident.to_string(), prefix);
                prefix.pop();
            }
            syn::UseTree::Rename(rename) => {
                prefix.push(rename.ident.to_string());
                self.record_alias(rename.rename.to_string(), prefix);
                prefix.pop();
            }
            syn::UseTree::Group(group) => {
                for item in &group.items {
                    self.collect_imports(item, prefix);
                }
            }
            syn::UseTree::Glob(_) => {}
        }
    }

    fn record_alias(&mut self, local: String, target: &[String]) {
        let source = target.join("::").trim_end_matches("::self").to_owned();
        let target = canonical_path(&source, &self.module, self.crate_names);
        let local = canonical_path(&local, &self.module, self.crate_names);
        self.aliases.insert(local, target);
    }
}

impl<'ast> syn::visit::Visit<'ast> for SpawnTypeAliases<'_> {
    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        self.collect_imports(&node.tree, &mut Vec::new());
        syn::visit::visit_item_use(self, node);
    }

    fn visit_item_type(&mut self, node: &'ast syn::ItemType) {
        if let Some(target) = type_name(&node.ty) {
            let target = canonical_path(&target, &self.module, self.crate_names);
            let local = canonical_path(&node.ident.to_string(), &self.module, self.crate_names);
            self.aliases.insert(local, target);
        }
        syn::visit::visit_item_type(self, node);
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        let Some((_, items)) = &node.content else {
            return;
        };
        self.module.push(node.ident.to_string());
        for item in items {
            self.visit_item(item);
        }
        self.module.pop();
    }
}

fn type_path_name(path: &syn::Path) -> Option<String> {
    (!path.segments.is_empty()).then(|| {
        path.segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>()
            .join("::")
    })
}

fn type_name(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Path(path) => type_path_name(&path.path),
        syn::Type::Reference(reference) => type_name(&reference.elem),
        syn::Type::Paren(paren) => type_name(&paren.elem),
        syn::Type::Group(group) => type_name(&group.elem),
        _ => None,
    }
}

fn source_module(path: &str) -> Option<Vec<String>> {
    let parts = path.split('/').collect::<Vec<_>>();
    let crates = parts.iter().position(|part| *part == "crates")?;
    let src = parts[crates + 2..].iter().position(|part| *part == "src")? + crates + 2;
    // crates/<group>/<package>/src/... — the package dir is two below crates/.
    let mut module = vec![parts.get(crates + 2)?.replace('-', "_")];
    let relative = &parts[src + 1..];
    for (index, part) in relative.iter().enumerate() {
        let last = index + 1 == relative.len();
        let stem = part.strip_suffix(".rs").unwrap_or(part);
        if !last || !matches!(stem, "lib" | "main" | "mod") {
            module.push(stem.to_owned());
        }
    }
    Some(module)
}

fn canonical_path(raw: &str, module: &[String], crate_names: &BTreeSet<String>) -> String {
    let parts = raw
        .split("::")
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let Some(first) = parts.first() else {
        return String::new();
    };
    let (mut base, mut skip) = match *first {
        "crate" => (module.first().cloned().into_iter().collect::<Vec<_>>(), 1),
        "self" => (module.to_vec(), 1),
        "super" => (module.to_vec(), 0),
        _ if parts.len() == 1 => (module.to_vec(), 0),
        _ if crate_names.contains(*first)
            || matches!(*first, "tokio" | "std" | "core" | "alloc") =>
        {
            (Vec::new(), 0)
        }
        _ => (module.to_vec(), 0),
    };
    while parts.get(skip) == Some(&"super") {
        if base.len() > 1 {
            base.pop();
        }
        skip += 1;
    }
    base.extend(parts.into_iter().skip(skip).map(str::to_owned));
    base.join("::")
}

#[derive(Default)]
pub(super) struct SpawnDeclarations {
    pub(super) resolver: SpawnTypeResolver,
    pub(super) fields: BTreeSet<String>,
    pub(super) factories: BTreeSet<String>,
}

impl SpawnDeclarations {
    pub(super) fn collect(path: &str, syntax: &syn::File, workspace: &WorkspaceSpawnTypes) -> Self {
        let mut declarations = Self {
            resolver: workspace.resolver(path),
            ..Self::default()
        };
        declarations.visit_file(syntax);
        declarations
    }
}

impl<'ast> syn::visit::Visit<'ast> for SpawnDeclarations {
    fn visit_field(&mut self, node: &'ast syn::Field) {
        if spawn_receiver_type(&node.ty, &self.resolver)
            && let Some(name) = &node.ident
        {
            self.fields.insert(name.to_string());
        }
        syn::visit::visit_field(self, node);
    }

    fn visit_signature(&mut self, node: &'ast syn::Signature) {
        if matches!(&node.output, syn::ReturnType::Type(_, ty) if spawn_receiver_type(ty, &self.resolver))
        {
            self.factories.insert(node.ident.to_string());
        }
        syn::visit::visit_signature(self, node);
    }
}

#[derive(Default)]
pub(super) struct AsyncScopeGuardScanner {
    pub(super) violations: Vec<(proc_macro2::Span, &'static str)>,
    runtime_receivers: BTreeSet<String>,
}

impl AsyncScopeGuardScanner {
    pub(super) fn for_signature(signature: &syn::Signature) -> Self {
        let mut scanner = Self::default();
        for input in &signature.inputs {
            if let syn::FnArg::Typed(typed) = input
                && Self::runtime_type(&typed.ty)
                && let syn::Pat::Ident(binding) = typed.pat.as_ref()
            {
                scanner.runtime_receivers.insert(binding.ident.to_string());
            }
        }
        scanner
    }

    fn path_name(path: &syn::Path) -> String {
        path.segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>()
            .join("::")
    }

    fn runtime_type(ty: &syn::Type) -> bool {
        match ty {
            syn::Type::Path(path) => matches!(
                Self::path_name(&path.path).as_str(),
                "tokio::runtime::Runtime" | "tokio::runtime::Handle"
            ),
            syn::Type::Reference(reference) => Self::runtime_type(&reference.elem),
            syn::Type::Paren(paren) => Self::runtime_type(&paren.elem),
            syn::Type::Group(group) => Self::runtime_type(&group.elem),
            _ => false,
        }
    }

    fn runtime_constructor(expr: &syn::Expr) -> bool {
        match expr {
            syn::Expr::Call(call) => {
                matches!(call.func.as_ref(), syn::Expr::Path(path) if matches!(
                    Self::path_name(&path.path).as_str(),
                    "tokio::runtime::Handle::current" | "tokio::runtime::Handle::try_current"
                ))
            }
            syn::Expr::MethodCall(call) => {
                call.method == "build" && Self::runtime_builder(&call.receiver)
                    || matches!(
                        call.method.to_string().as_str(),
                        "expect" | "unwrap" | "as_ref"
                    ) && Self::runtime_constructor(&call.receiver)
            }
            syn::Expr::Try(try_expr) => Self::runtime_constructor(&try_expr.expr),
            syn::Expr::Paren(paren) => Self::runtime_constructor(&paren.expr),
            syn::Expr::Group(group) => Self::runtime_constructor(&group.expr),
            _ => false,
        }
    }

    fn runtime_builder(expr: &syn::Expr) -> bool {
        match expr {
            syn::Expr::Call(call) => {
                matches!(call.func.as_ref(), syn::Expr::Path(path) if matches!(
                    Self::path_name(&path.path).as_str(),
                    "tokio::runtime::Builder::new_current_thread" | "tokio::runtime::Builder::new_multi_thread"
                ))
            }
            syn::Expr::MethodCall(call) => Self::runtime_builder(&call.receiver),
            syn::Expr::Paren(paren) => Self::runtime_builder(&paren.expr),
            syn::Expr::Group(group) => Self::runtime_builder(&group.expr),
            _ => false,
        }
    }

    fn runtime_receiver(&self, receiver: &syn::Expr) -> bool {
        matches!(receiver, syn::Expr::Path(path) if path.path.segments.last().is_some_and(|segment| {
            self.runtime_receivers.contains(&segment.ident.to_string())
        }))
    }

    fn guard_type(ty: &syn::Type) -> Option<&'static str> {
        let syn::Type::Path(path) = ty else {
            return None;
        };
        match path.path.segments.last()?.ident.to_string().as_str() {
            "ContextGuard" => Some("OpenTelemetry context guard created inside async scope"),
            "Entered" | "EnteredSpan" => Some("span guard created inside async scope"),
            _ => None,
        }
    }
}

impl<'ast> syn::visit::Visit<'ast> for AsyncScopeGuardScanner {
    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if matches!(node.method.to_string().as_str(), "enter" | "entered")
            && !self.runtime_receiver(&node.receiver)
        {
            self.violations
                .push((node.span(), "span guard created inside async scope"));
        }
        if node.method == "attach" {
            self.violations.push((
                node.span(),
                "OpenTelemetry context guard created inside async scope",
            ));
        }
        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_local(&mut self, node: &'ast syn::Local) {
        if let syn::Pat::Type(typed) = &node.pat {
            if let Some(violation) = Self::guard_type(&typed.ty) {
                self.violations.push((node.span(), violation));
            }
            if Self::runtime_type(&typed.ty)
                && let syn::Pat::Ident(binding) = typed.pat.as_ref()
            {
                self.runtime_receivers.insert(binding.ident.to_string());
            }
        } else if let syn::Pat::Ident(binding) = &node.pat
            && let Some(initializer) = &node.init
            && Self::runtime_constructor(&initializer.expr)
        {
            self.runtime_receivers.insert(binding.ident.to_string());
        }
        syn::visit::visit_local(self, node);
    }
}

pub(super) struct SourcePolicyScanner<'a> {
    path: &'a str,
    pub(super) violations: BTreeSet<(usize, &'static str)>,
    telemetry_imports: TelemetryImports,
    spawn_aliases: BTreeSet<String>,
    spawn_module_aliases: BTreeMap<String, String>,
    spawn_receivers: BTreeSet<String>,
    spawn_type_resolver: SpawnTypeResolver,
    spawn_fields: BTreeSet<String>,
    spawn_factories: BTreeSet<String>,
}

impl<'a> SourcePolicyScanner<'a> {
    pub(super) fn new(path: &'a str, syntax: &syn::File, workspace: &WorkspaceSpawnTypes) -> Self {
        let declarations = SpawnDeclarations::collect(path, syntax, workspace);
        Self {
            path,
            violations: BTreeSet::new(),
            telemetry_imports: TelemetryImports::collect(syntax),
            spawn_aliases: BTreeSet::new(),
            spawn_module_aliases: BTreeMap::new(),
            spawn_receivers: BTreeSet::new(),
            spawn_type_resolver: declarations.resolver,
            spawn_fields: declarations.fields,
            spawn_factories: declarations.factories,
        }
    }

    fn allows_spawn(&self) -> bool {
        self.path == "crates/services/jackin-telemetry/src/spawn.rs"
            || RAW_SPAWN_ALLOWLIST.contains(&self.path)
    }

    fn allows_telemetry_apis(&self) -> bool {
        self.path.starts_with("crates/services/jackin-telemetry/")
            || self.path.starts_with("crates/services/jackin-diagnostics/")
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

    fn allows_raw_tracing(&self) -> bool {
        self.allows_telemetry_apis() || RAW_TRACING_ALLOWLIST.contains(&self.path)
    }

    fn reject_macro_path(&mut self, path: &str, span: proc_macro2::Span) {
        if self.telemetry_imports.paths(path).iter().any(|candidate| {
            PROHIBITED_TELEMETRY_MACROS.contains(&candidate.rsplit("::").next().unwrap_or_default())
        }) {
            self.reject(span, "prohibited legacy/generic telemetry macro");
        }
        if !self.allows_raw_tracing() && self.telemetry_imports.is_tracing_macro(path) {
            self.reject(span, "raw tracing call outside governed facade");
        }
    }

    fn reject_attribute_path(&mut self, path: &str, span: proc_macro2::Span) {
        if !self.allows_raw_tracing()
            && (path == "instrument" || self.telemetry_imports.is_tracing_instrument(path))
        {
            self.reject(span, "tracing instrument outside governed facade");
        }
    }

    fn reject_meter_path(&mut self, path: &str, span: proc_macro2::Span) {
        if !self.allows_telemetry_apis() && self.telemetry_imports.is_raw_meter(path) {
            self.reject(span, "raw OpenTelemetry meter construction");
        }
    }

    fn token_path(tokens: &[TokenTree], start: usize) -> Option<(String, usize)> {
        let first_index = if matches!(tokens.get(start), Some(TokenTree::Punct(punct)) if punct.as_char() == ':')
            && matches!(tokens.get(start + 1), Some(TokenTree::Punct(punct)) if punct.as_char() == ':')
        {
            start + 2
        } else {
            start
        };
        let TokenTree::Ident(first) = tokens.get(first_index)? else {
            return None;
        };
        let mut segments = vec![first.to_string()];
        let mut cursor = first_index + 1;
        while matches!(tokens.get(cursor), Some(TokenTree::Punct(punct)) if punct.as_char() == ':')
            && matches!(tokens.get(cursor + 1), Some(TokenTree::Punct(punct)) if punct.as_char() == ':')
            && matches!(tokens.get(cursor + 2), Some(TokenTree::Ident(_)))
        {
            let TokenTree::Ident(segment) = &tokens[cursor + 2] else {
                unreachable!();
            };
            segments.push(segment.to_string());
            cursor += 3;
        }
        Some((segments.join("::"), cursor))
    }

    fn scan_macro_tokens(&mut self, tokens: TokenStream) {
        let trees = tokens.into_iter().collect::<Vec<_>>();
        for (index, token) in trees.iter().enumerate() {
            let TokenTree::Group(group) = token else {
                continue;
            };
            let attribute_prefix = matches!(trees.get(index.wrapping_sub(1)), Some(TokenTree::Punct(punct)) if punct.as_char() == '#')
                || index >= 2
                    && matches!(trees.get(index - 2), Some(TokenTree::Punct(punct)) if punct.as_char() == '#')
                    && matches!(trees.get(index - 1), Some(TokenTree::Punct(punct)) if punct.as_char() == '!');
            if group.delimiter() == Delimiter::Bracket && attribute_prefix {
                self.scan_attribute_tokens(group.stream(), group.span());
            }
            self.scan_macro_tokens(group.stream());
        }

        for (index, token) in trees.iter().enumerate() {
            if let TokenTree::Punct(punct) = token
                && punct.as_char() == '.'
                && matches!(trees.get(index + 1), Some(TokenTree::Ident(name)) if name == "meter")
                && matches!(trees.get(index + 2), Some(TokenTree::Group(group)) if group.delimiter() == Delimiter::Parenthesis)
                && !self.allows_telemetry_apis()
            {
                self.reject(token.span(), "raw OpenTelemetry meter construction");
            }

            let Some((path, end)) = Self::token_path(&trees, index) else {
                continue;
            };
            let span = token.span();
            self.reject_meter_path(&path, span);
            if matches!(trees.get(end), Some(TokenTree::Punct(punct)) if punct.as_char() == '!') {
                self.reject_macro_path(&path, span);
            }
        }
    }

    fn scan_attribute_tokens(&mut self, tokens: TokenStream, span: proc_macro2::Span) {
        let Ok(meta) = syn::parse2::<syn::Meta>(tokens) else {
            return;
        };
        self.scan_attribute_meta(&meta, span);
    }

    fn scan_attribute_meta(&mut self, meta: &syn::Meta, span: proc_macro2::Span) {
        let path = Self::path_name(meta.path());
        self.reject_attribute_path(&path, span);
        if path != "cfg_attr" {
            return;
        }
        let syn::Meta::List(list) = meta else {
            return;
        };
        use syn::parse::Parser as _;
        let parser = syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated;
        let Ok(attributes) = parser.parse2(list.tokens.clone()) else {
            return;
        };
        for attribute in attributes.iter().skip(1) {
            self.scan_attribute_meta(attribute, span);
        }
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
            self.reject_meter_path(&name, node.span());
        }
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_expr_path(&mut self, node: &'ast syn::ExprPath) {
        self.reject_meter_path(&Self::path_name(&node.path), node.span());
        syn::visit::visit_expr_path(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if node.method == "with_callback" {
            for argument in &node.args {
                if !matches!(argument, syn::Expr::Closure(_)) {
                    self.reject(
                        argument.span(),
                        "observable callback must be an inline snapshot-only closure",
                    );
                    continue;
                }
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
                if self.telemetry_imports.is_raw_meter(&source) {
                    self.telemetry_imports
                        .record_alias_path(&binding.ident.to_string(), &source);
                }
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

    fn visit_item_const(&mut self, node: &'ast syn::ItemConst) {
        if let syn::Expr::Path(path) = node.expr.as_ref() {
            let source = Self::path_name(&path.path);
            if self.telemetry_imports.is_raw_meter(&source) {
                self.telemetry_imports
                    .record_alias_path(&node.ident.to_string(), &source);
            }
        }
        syn::visit::visit_item_const(self, node);
    }

    fn visit_item_static(&mut self, node: &'ast syn::ItemStatic) {
        if let syn::Expr::Path(path) = node.expr.as_ref() {
            let source = Self::path_name(&path.path);
            if self.telemetry_imports.is_raw_meter(&source) {
                self.telemetry_imports
                    .record_alias_path(&node.ident.to_string(), &source);
            }
        }
        syn::visit::visit_item_static(self, node);
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
        self.reject_macro_path(&name, node.span());
        self.scan_macro_tokens(node.tokens.clone());
        syn::visit::visit_macro(self, node);
    }

    fn visit_attribute(&mut self, node: &'ast syn::Attribute) {
        let name = Self::path_name(node.path());
        self.reject_attribute_path(&name, node.span());
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
        let allowed = match node.func.as_ref() {
            syn::Expr::Path(function) => matches!(
                SourcePolicyScanner::path_name(&function.path).as_str(),
                "f64::from_bits" | "health::count"
            ),
            _ => false,
        };
        if !allowed {
            self.reject(node.span());
        }
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if !matches!(
            node.method.to_string().as_str(),
            "observe"
                | "load"
                | "metrics"
                | "num_workers"
                | "num_alive_tasks"
                | "global_queue_depth"
        ) {
            self.reject(node.span());
        }
        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_expr_await(&mut self, node: &'ast syn::ExprAwait) {
        self.reject(node.span());
        syn::visit::visit_expr_await(self, node);
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        self.reject(node.span());
        syn::visit::visit_macro(self, node);
    }
}
