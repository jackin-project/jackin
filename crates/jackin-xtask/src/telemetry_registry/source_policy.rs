// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use syn::spanned::Spanned as _;
use syn::visit::Visit as _;

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
    let mut module = vec![parts.get(crates + 1)?.replace('-', "_")];
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
