// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use syn::spanned::Spanned as _;
use syn::visit::Visit as _;

pub(super) fn spawn_receiver_type(ty: &syn::Type, aliases: &BTreeMap<String, String>) -> bool {
    match ty {
        syn::Type::Path(path) => path.path.segments.last().is_some_and(|segment| {
            let mut name = segment.ident.to_string();
            let mut visited = BTreeSet::new();
            while visited.insert(name.clone()) {
                if matches!(name.as_str(), "Handle" | "JoinSet") {
                    return true;
                }
                let Some(target) = aliases.get(&name) else {
                    break;
                };
                name.clone_from(target);
            }
            false
        }),
        syn::Type::Reference(reference) => spawn_receiver_type(&reference.elem, aliases),
        syn::Type::Paren(paren) => spawn_receiver_type(&paren.elem, aliases),
        syn::Type::Group(group) => spawn_receiver_type(&group.elem, aliases),
        _ => false,
    }
}

#[derive(Default)]
struct SpawnTypeAliases {
    aliases: BTreeMap<String, String>,
}

impl SpawnTypeAliases {
    fn collect(syntax: &syn::File) -> BTreeMap<String, String> {
        let mut collector = Self::default();
        collector.visit_file(syntax);
        collector.aliases
    }

    fn collect_imports(&mut self, tree: &syn::UseTree, prefix: &mut Vec<String>) {
        match tree {
            syn::UseTree::Path(path) => {
                prefix.push(path.ident.to_string());
                self.collect_imports(&path.tree, prefix);
                prefix.pop();
            }
            syn::UseTree::Rename(rename) => {
                prefix.push(rename.ident.to_string());
                if let Some(target) = prefix.last() {
                    self.aliases
                        .insert(rename.rename.to_string(), target.clone());
                }
                prefix.pop();
            }
            syn::UseTree::Group(group) => {
                for item in &group.items {
                    self.collect_imports(item, prefix);
                }
            }
            syn::UseTree::Name(_) | syn::UseTree::Glob(_) => {}
        }
    }

    fn type_name(ty: &syn::Type) -> Option<String> {
        match ty {
            syn::Type::Path(path) => path
                .path
                .segments
                .last()
                .map(|segment| segment.ident.to_string()),
            syn::Type::Reference(reference) => Self::type_name(&reference.elem),
            syn::Type::Paren(paren) => Self::type_name(&paren.elem),
            syn::Type::Group(group) => Self::type_name(&group.elem),
            _ => None,
        }
    }
}

impl<'ast> syn::visit::Visit<'ast> for SpawnTypeAliases {
    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        self.collect_imports(&node.tree, &mut Vec::new());
        syn::visit::visit_item_use(self, node);
    }

    fn visit_item_type(&mut self, node: &'ast syn::ItemType) {
        if let Some(target) = Self::type_name(&node.ty) {
            self.aliases.insert(node.ident.to_string(), target);
        }
        syn::visit::visit_item_type(self, node);
    }
}

#[derive(Default)]
pub(super) struct SpawnDeclarations {
    pub(super) aliases: BTreeMap<String, String>,
    pub(super) fields: BTreeSet<String>,
    pub(super) factories: BTreeSet<String>,
}

impl SpawnDeclarations {
    pub(super) fn collect(syntax: &syn::File) -> Self {
        let mut declarations = Self {
            aliases: SpawnTypeAliases::collect(syntax),
            ..Self::default()
        };
        declarations.visit_file(syntax);
        declarations
    }
}

impl<'ast> syn::visit::Visit<'ast> for SpawnDeclarations {
    fn visit_field(&mut self, node: &'ast syn::Field) {
        if spawn_receiver_type(&node.ty, &self.aliases)
            && let Some(name) = &node.ident
        {
            self.fields.insert(name.to_string());
        }
        syn::visit::visit_field(self, node);
    }

    fn visit_signature(&mut self, node: &'ast syn::Signature) {
        if matches!(&node.output, syn::ReturnType::Type(_, ty) if spawn_receiver_type(ty, &self.aliases))
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
