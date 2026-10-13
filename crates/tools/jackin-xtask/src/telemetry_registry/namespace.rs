//! Binding-aware validation of governed telemetry namespace literals.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use syn::parse::Parser as _;
use syn::spanned::Spanned as _;
use syn::visit::Visit as _;

pub(super) fn validate_legacy_namespaces(root: &Path) -> Result<()> {
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

fn is_event_metadata_type(ty: &syn::Type, bindings: &NamespaceBindings) -> bool {
    match ty {
        syn::Type::Reference(reference) => is_event_metadata_type(&reference.elem, bindings),
        syn::Type::Paren(parenthesized) => is_event_metadata_type(&parenthesized.elem, bindings),
        syn::Type::Group(group) => is_event_metadata_type(&group.elem, bindings),
        syn::Type::Path(path) if path.qself.is_none() => {
            bindings.expand_path(&path.path).is_some_and(|path| {
                path.last()
                    .is_some_and(|segment| segment == "EventMetadata")
                    && path
                        .windows(2)
                        .any(|pair| pair == ["schema", "EventMetadata"])
                    && bindings.is_telemetry_root(&path)
            })
        }
        _ => false,
    }
}

fn pattern_binding_names(pattern: &syn::Pat) -> BTreeSet<String> {
    #[derive(Default)]
    struct Bindings(BTreeSet<String>);

    impl<'ast> syn::visit::Visit<'ast> for Bindings {
        fn visit_pat_ident(&mut self, pattern: &'ast syn::PatIdent) {
            self.0.insert(pattern.ident.to_string());
            syn::visit::visit_pat_ident(self, pattern);
        }
    }

    let mut bindings = Bindings::default();
    syn::visit::Visit::visit_pat(&mut bindings, pattern);
    bindings.0
}

pub(super) struct NamespaceScanner<'a> {
    path: &'a str,
    context: String,
    bindings: NamespaceBindings,
    event_metadata_bindings: BTreeSet<(String, String)>,
    event_attribute_bindings: BTreeSet<String>,
    schema_key_bindings: BTreeSet<(String, String)>,
    pub(super) violations: BTreeSet<(usize, String)>,
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
    pub(super) fn new(path: &'a str) -> Self {
        Self {
            path,
            context: String::from("file"),
            bindings: NamespaceBindings {
                telemetry_crate: path.starts_with("crates/services/jackin-telemetry/"),
                ..NamespaceBindings::default()
            },
            event_metadata_bindings: BTreeSet::new(),
            event_attribute_bindings: BTreeSet::new(),
            schema_key_bindings: BTreeSet::new(),
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

    fn is_event_metadata_definition(&self, expression: &syn::Expr) -> bool {
        match expression {
            syn::Expr::Call(call) => {
                let syn::Expr::Path(path) = call.func.as_ref() else {
                    return false;
                };
                self.bindings.expand_path(&path.path).is_some_and(|path| {
                    self.bindings.is_telemetry_root(&path)
                        && path
                            .windows(3)
                            .any(|triple| triple == ["schema", "events", "definition"])
                })
            }
            syn::Expr::MethodCall(call) if call.method == "expect" => {
                self.is_event_metadata_definition(&call.receiver)
            }
            syn::Expr::Paren(expression) => self.is_event_metadata_definition(&expression.expr),
            syn::Expr::Group(expression) => self.is_event_metadata_definition(&expression.expr),
            syn::Expr::Reference(expression) => self.is_event_metadata_definition(&expression.expr),
            _ => false,
        }
    }

    fn is_event_metadata_binding(&self, expression: &syn::Expr) -> bool {
        let syn::Expr::Path(path) = expression else {
            return false;
        };
        path.path.segments.len() == 1
            && self.event_metadata_bindings.contains(&(
                self.context.clone(),
                path.path.segments[0].ident.to_string(),
            ))
    }

    fn is_event_metadata_attributes_iter(&self, expression: &syn::Expr) -> bool {
        match expression {
            syn::Expr::Field(field) => {
                matches!(&field.member, syn::Member::Named(member) if member == "attributes")
                    && self.is_event_metadata_binding(&field.base)
            }
            syn::Expr::MethodCall(call)
                if matches!(
                    call.method.to_string().as_str(),
                    "iter" | "iter_mut" | "into_iter" | "filter" | "map" | "copied" | "cloned"
                ) =>
            {
                self.is_event_metadata_attributes_iter(&call.receiver)
            }
            syn::Expr::Paren(expression) => {
                self.is_event_metadata_attributes_iter(&expression.expr)
            }
            syn::Expr::Group(expression) => {
                self.is_event_metadata_attributes_iter(&expression.expr)
            }
            syn::Expr::Reference(expression) => {
                self.is_event_metadata_attributes_iter(&expression.expr)
            }
            _ => false,
        }
    }

    fn is_schema_all_keys_expression(&self, expression: &syn::Expr) -> bool {
        match expression {
            syn::Expr::Path(path) => self
                .bindings
                .expand_path(&path.path)
                .is_some_and(|path| self.bindings.is_schema_all_keys_path(&path)),
            syn::Expr::MethodCall(call)
                if matches!(
                    call.method.to_string().as_str(),
                    "iter"
                        | "iter_mut"
                        | "into_iter"
                        | "filter"
                        | "find"
                        | "copied"
                        | "cloned"
                        | "next"
                        | "nth"
                        | "expect"
                ) =>
            {
                self.is_schema_all_keys_expression(&call.receiver)
            }
            syn::Expr::Paren(expression) => self.is_schema_all_keys_expression(&expression.expr),
            syn::Expr::Group(expression) => self.is_schema_all_keys_expression(&expression.expr),
            syn::Expr::Reference(expression) => {
                self.is_schema_all_keys_expression(&expression.expr)
            }
            _ => false,
        }
    }

    fn is_schema_key_collection(
        &self,
        expression: &syn::Expr,
        seen: &mut BTreeSet<String>,
    ) -> bool {
        if self.is_schema_all_keys_expression(expression) {
            return true;
        }
        match expression {
            syn::Expr::Array(array) => {
                !array.elems.is_empty()
                    && array.elems.iter().all(|element| {
                        matches!(
                            self.resolve_attr_key(element, &mut BTreeSet::new()),
                            AttrKeySource::Schema
                        )
                    })
            }
            syn::Expr::Path(path) if path.path.segments.len() == 1 => {
                let name = path.path.segments[0].ident.to_string();
                let local = (self.context.clone(), name.clone());
                let (binding, expressions) = if let Some(values) = self.bindings.locals.get(&local)
                {
                    (format!("local:{}:{name}", self.context), values)
                } else if let Some(values) = self.bindings.constants.get(&name) {
                    (format!("const:{name}"), values)
                } else {
                    return false;
                };
                if expressions.len() != 1 || !seen.insert(binding.clone()) {
                    return false;
                }
                let result = self.is_schema_key_collection(&expressions[0], seen);
                seen.remove(&binding);
                result
            }
            syn::Expr::MethodCall(call)
                if matches!(
                    call.method.to_string().as_str(),
                    "iter" | "iter_mut" | "into_iter" | "copied" | "cloned"
                ) =>
            {
                self.is_schema_key_collection(&call.receiver, seen)
            }
            syn::Expr::Paren(expression) => self.is_schema_key_collection(&expression.expr, seen),
            syn::Expr::Group(expression) => self.is_schema_key_collection(&expression.expr, seen),
            syn::Expr::Reference(expression) => {
                self.is_schema_key_collection(&expression.expr, seen)
            }
            _ => false,
        }
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
                if self
                    .schema_key_bindings
                    .contains(&(self.context.clone(), name.clone()))
                {
                    return AttrKeySource::Schema;
                }
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
                    (Some(name), "name") if self.event_attribute_bindings.contains(name) => {
                        AttrKeySource::Schema
                    }
                    _ => AttrKeySource::Unknown,
                }
            }
            syn::Expr::MethodCall(_) if self.is_schema_all_keys_expression(expression) => {
                AttrKeySource::Schema
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

    fn inspect_macro_tokens(&mut self, tokens: proc_macro2::TokenStream) {
        if let Ok(expression) = syn::parse2::<syn::Expr>(tokens.clone()) {
            self.visit_expr(&expression);
            return;
        }
        let expressions =
            syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated;
        if let Ok(expressions) = expressions.parse2(tokens.clone()) {
            for expression in expressions {
                self.visit_expr(&expression);
            }
            return;
        }
        if let Ok(block) = syn::parse2::<syn::Block>(tokens.clone()) {
            self.visit_block(&block);
            return;
        }
        for token in tokens {
            if let proc_macro2::TokenTree::Group(group) = token {
                self.inspect_macro_tokens(group.stream());
            }
        }
    }

    fn visit_scoped_closure<'ast>(
        &mut self,
        closure: &'ast syn::ExprClosure,
        event_attribute_iter: bool,
    ) where
        Self: syn::visit::Visit<'ast>,
    {
        let previous_metadata_bindings = self.event_metadata_bindings.clone();
        let previous_attribute_bindings = self.event_attribute_bindings.clone();
        for input in &closure.inputs {
            let (pattern, metadata_type) = match input {
                syn::Pat::Ident(_) => (Some(input), None),
                syn::Pat::Type(pattern) => (Some(pattern.pat.as_ref()), Some(pattern.ty.as_ref())),
                _ => (None, None),
            };
            if let Some(syn::Pat::Ident(pattern)) = pattern {
                let name = pattern.ident.to_string();
                self.event_attribute_bindings.remove(&name);
                self.event_metadata_bindings
                    .remove(&(self.context.clone(), name.clone()));
                if event_attribute_iter {
                    self.event_attribute_bindings.insert(name.clone());
                }
                if metadata_type.is_some_and(|ty| is_event_metadata_type(ty, &self.bindings)) {
                    self.event_metadata_bindings
                        .insert((self.context.clone(), name));
                }
            }
        }
        syn::visit::visit_expr_closure(self, closure);
        self.event_metadata_bindings = previous_metadata_bindings;
        self.event_attribute_bindings = previous_attribute_bindings;
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
        if self.is_schema_all_keys_path(path) {
            return true;
        }
        let normalized = if self.telemetry_crate
            && self.has_telemetry_glob()
            && path.first().is_some_and(|segment| segment == "attrs")
        {
            let mut normalized = vec![String::from("schema")];
            normalized.extend(path.iter().cloned());
            normalized
        } else {
            path.to_vec()
        };
        if !self.is_telemetry_root(&normalized) || !is_schema_constant_name(normalized.last()) {
            return false;
        }
        if let Some(index) = normalized
            .windows(3)
            .position(|triple| triple == ["schema", "attrs", "std_attrs"])
        {
            return normalized.len() == index + 4;
        }
        normalized
            .windows(2)
            .position(|pair| pair == ["schema", "attrs"])
            .is_some_and(|index| normalized.len() == index + 3)
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
        if let (syn::Pat::Ident(pattern), Some(initializer)) = (&local.pat, local.init.as_ref()) {
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

    fn visit_block(&mut self, block: &'ast syn::Block) {
        let event_metadata_bindings = self.event_metadata_bindings.clone();
        let event_attribute_bindings = self.event_attribute_bindings.clone();
        let schema_key_bindings = self.schema_key_bindings.clone();
        syn::visit::visit_block(self, block);
        self.event_metadata_bindings = event_metadata_bindings;
        self.event_attribute_bindings = event_attribute_bindings;
        self.schema_key_bindings = schema_key_bindings;
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
        let event_metadata_bindings = self.event_metadata_bindings.clone();
        let previous = std::mem::replace(&mut self.context, format!("fn:{}", item.sig.ident));
        for input in &item.sig.inputs {
            let syn::FnArg::Typed(argument) = input else {
                continue;
            };
            let syn::Pat::Ident(pattern) = argument.pat.as_ref() else {
                continue;
            };
            if is_event_metadata_type(&argument.ty, &self.bindings) {
                self.event_metadata_bindings
                    .insert((self.context.clone(), pattern.ident.to_string()));
            }
        }
        syn::visit::visit_item_fn(self, item);
        self.context = previous;
        self.event_metadata_bindings = event_metadata_bindings;
    }

    fn visit_local(&mut self, local: &'ast syn::Local) {
        let bound_names = pattern_binding_names(&local.pat);
        let direct_binding = match &local.pat {
            syn::Pat::Ident(pattern) => Some((&pattern.ident, None)),
            syn::Pat::Type(pattern) => match pattern.pat.as_ref() {
                syn::Pat::Ident(binding) => Some((&binding.ident, Some(pattern.ty.as_ref()))),
                _ => None,
            },
            _ => None,
        };
        let metadata_binding = direct_binding.and_then(|(ident, declared_type)| {
            let is_metadata = match declared_type {
                Some(ty) => is_event_metadata_type(ty, &self.bindings),
                None => local.init.as_ref().is_some_and(|initializer| {
                    self.is_event_metadata_definition(&initializer.expr)
                }),
            };
            is_metadata.then(|| ident.to_string())
        });
        syn::visit::visit_local(self, local);
        for name in bound_names {
            self.event_attribute_bindings.remove(&name);
            self.event_metadata_bindings
                .remove(&(self.context.clone(), name));
        }
        if let Some(name) = metadata_binding {
            self.event_metadata_bindings
                .insert((self.context.clone(), name));
        }
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        let is_event_attribute_iter = self.is_event_metadata_attributes_iter(&call.receiver);
        self.visit_expr(&call.receiver);
        for argument in &call.args {
            if let syn::Expr::Closure(closure) = argument {
                self.visit_scoped_closure(closure, is_event_attribute_iter);
            } else {
                self.visit_expr(argument);
            }
        }
    }

    fn visit_expr_closure(&mut self, closure: &'ast syn::ExprClosure) {
        self.visit_scoped_closure(closure, false);
    }

    fn visit_expr_for_loop(&mut self, expression: &'ast syn::ExprForLoop) {
        for attribute in &expression.attrs {
            self.visit_attribute(attribute);
        }
        self.visit_expr(&expression.expr);

        let previous_metadata_bindings = self.event_metadata_bindings.clone();
        let previous_attribute_bindings = self.event_attribute_bindings.clone();
        let previous_schema_key_bindings = self.schema_key_bindings.clone();
        let is_schema_key_collection =
            self.is_schema_key_collection(&expression.expr, &mut BTreeSet::new());
        for name in pattern_binding_names(&expression.pat) {
            self.event_metadata_bindings
                .remove(&(self.context.clone(), name.clone()));
            self.event_attribute_bindings.remove(&name);
            self.schema_key_bindings
                .remove(&(self.context.clone(), name.clone()));
            if is_schema_key_collection {
                self.schema_key_bindings
                    .insert((self.context.clone(), name));
            }
        }
        self.visit_block(&expression.body);
        self.event_metadata_bindings = previous_metadata_bindings;
        self.event_attribute_bindings = previous_attribute_bindings;
        self.schema_key_bindings = previous_schema_key_bindings;
    }

    fn visit_macro(&mut self, invocation: &'ast syn::Macro) {
        self.inspect_macro_tokens(invocation.tokens.clone());
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
