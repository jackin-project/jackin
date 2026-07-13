//! jackin❯ project lints (dylint).
//!
//! Isolated from the main workspace (own nightly pin). Never make this a
//! workspace member — dylint compiles against rustc-private APIs.

#![feature(rustc_private)]
#![warn(unused_extern_crates)]

extern crate rustc_errors;
extern crate rustc_hir;
extern crate rustc_middle;
extern crate rustc_span;

use rustc_errors::DiagDecorator;
use rustc_hir::def::Res;
use rustc_hir::def_id::{DefId, LocalDefId};
use rustc_hir::{Body, Expr, ExprKind, FnDecl, HirId, ImplItemKind, ItemKind, StructTailExpr};
use rustc_lint::{LateContext, LateLintPass, LintContext};
use rustc_span::{Span, Symbol};
use std::collections::{HashSet, VecDeque};

dylint_linting::declare_late_lint! {
    /// ### What it does
    ///
    /// Flags blocking I/O / process / `std::sync` lock calls reachable from a
    /// render-path function (named `render`, or a compositor frame helper
    /// named `compose_pending_frame` / `compose_ratatui_frame`).
    ///
    /// ### Why is this bad?
    ///
    /// The terminal draw path must not block on filesystem, network, sleep,
    /// process spawn, or `std::sync` locks. Clippy method lists cannot see
    /// one call deep; this lint walks a bounded local call graph.
    ///
    /// ### Example
    ///
    /// ```rust,ignore
    /// fn render(&self) {
    ///     let _ = std::fs::read("/tmp/x"); // ~RENDER_THREAD_PURITY
    /// }
    /// ```
    pub RENDER_THREAD_PURITY,
    Warn,
    "blocking I/O or std::sync locks reachable from a render-path function"
}

const MAX_DEPTH: usize = 5;

const RENDER_ROOT_NAMES: &[&str] = &[
    "render",
    "compose_pending_frame",
    "compose_ratatui_frame",
];

impl<'tcx> LateLintPass<'tcx> for RenderThreadPurity {
    fn check_fn(
        &mut self,
        cx: &LateContext<'tcx>,
        kind: rustc_hir::intravisit::FnKind<'tcx>,
        _decl: &'tcx FnDecl<'tcx>,
        body: &'tcx Body<'tcx>,
        _span: Span,
        def_id: LocalDefId,
    ) {
        // Closures have no item_name — skip (also intentional graph boundary).
        match kind {
            rustc_hir::intravisit::FnKind::ItemFn(ident, _, _)
            | rustc_hir::intravisit::FnKind::Method(ident, _) => {
                let name = ident.name.as_str();
                if !RENDER_ROOT_NAMES.contains(&name) {
                    return;
                }
                walk_from_body(cx, body, def_id, name);
            }
            rustc_hir::intravisit::FnKind::Closure => {}
        }
    }
}

fn walk_from_body<'tcx>(
    cx: &LateContext<'tcx>,
    body: &'tcx Body<'tcx>,
    root: LocalDefId,
    root_name: &str,
) {
    let mut queue: VecDeque<(LocalDefId, usize, Vec<String>, Option<&'tcx Body<'tcx>>)> =
        VecDeque::new();
    queue.push_back((root, 0, vec![root_name.to_owned()], Some(body)));
    let mut seen: HashSet<LocalDefId> = HashSet::new();
    let mut reported: HashSet<(DefId, Span)> = HashSet::new();

    while let Some((def_id, depth, chain, body_opt)) = queue.pop_front() {
        if !seen.insert(def_id) || depth > MAX_DEPTH {
            continue;
        }
        let Some(body) = body_opt.or_else(|| cx.tcx.hir_maybe_body_owned_by(def_id)) else {
            continue;
        };
        // typeck_results is valid only for the body currently under analysis.
        // For callees we re-enter via their own body id through nested typeck.
        find_calls(cx, body.value, &chain, depth, def_id, &mut queue, &mut reported);
    }
}

fn find_calls<'tcx>(
    cx: &LateContext<'tcx>,
    expr: &'tcx Expr<'tcx>,
    chain: &[String],
    depth: usize,
    current_fn: LocalDefId,
    queue: &mut VecDeque<(LocalDefId, usize, Vec<String>, Option<&'tcx Body<'tcx>>)>,
    reported: &mut HashSet<(DefId, Span)>,
) {
    match expr.kind {
        ExprKind::Call(callee, args) => {
            check_callee(
                cx, callee, expr.span, chain, depth, current_fn, queue, reported,
            );
            for arg in args {
                find_calls(cx, arg, chain, depth, current_fn, queue, reported);
            }
            find_calls(cx, callee, chain, depth, current_fn, queue, reported);
        }
        ExprKind::MethodCall(_path, receiver, args, _span) => {
            check_method(cx, expr, chain, current_fn, reported);
            find_calls(cx, receiver, chain, depth, current_fn, queue, reported);
            for arg in args {
                find_calls(cx, arg, chain, depth, current_fn, queue, reported);
            }
        }
        // Closure bodies are graph boundaries (spawn_blocking / tokio::spawn).
        ExprKind::Closure(_) => {}
        ExprKind::Block(block, _) => {
            for stmt in block.stmts {
                match stmt.kind {
                    rustc_hir::StmtKind::Expr(e) | rustc_hir::StmtKind::Semi(e) => {
                        find_calls(cx, e, chain, depth, current_fn, queue, reported);
                    }
                    rustc_hir::StmtKind::Let(local) => {
                        if let Some(init) = local.init {
                            find_calls(cx, init, chain, depth, current_fn, queue, reported);
                        }
                    }
                    _ => {}
                }
            }
            if let Some(e) = block.expr {
                find_calls(cx, e, chain, depth, current_fn, queue, reported);
            }
        }
        ExprKind::If(cond, then, else_) => {
            find_calls(cx, cond, chain, depth, current_fn, queue, reported);
            find_calls(cx, then, chain, depth, current_fn, queue, reported);
            if let Some(e) = else_ {
                find_calls(cx, e, chain, depth, current_fn, queue, reported);
            }
        }
        ExprKind::Match(scrut, arms, _) => {
            find_calls(cx, scrut, chain, depth, current_fn, queue, reported);
            for arm in arms {
                find_calls(cx, arm.body, chain, depth, current_fn, queue, reported);
            }
        }
        ExprKind::DropTemps(e)
        | ExprKind::Ret(Some(e))
        | ExprKind::Break(_, Some(e))
        | ExprKind::Unary(_, e)
        | ExprKind::Cast(e, _)
        | ExprKind::Type(e, _)
        | ExprKind::Become(e)
        | ExprKind::AddrOf(_, _, e) => {
            find_calls(cx, e, chain, depth, current_fn, queue, reported);
        }
        ExprKind::Binary(_, a, b)
        | ExprKind::Assign(a, b, _)
        | ExprKind::AssignOp(_, a, b)
        | ExprKind::Index(a, b, _) => {
            find_calls(cx, a, chain, depth, current_fn, queue, reported);
            find_calls(cx, b, chain, depth, current_fn, queue, reported);
        }
        ExprKind::Field(e, _) => find_calls(cx, e, chain, depth, current_fn, queue, reported),
        ExprKind::Tup(exprs) | ExprKind::Array(exprs) => {
            for e in exprs {
                find_calls(cx, e, chain, depth, current_fn, queue, reported);
            }
        }
        ExprKind::Struct(_, fields, base) => {
            for f in fields {
                find_calls(cx, f.expr, chain, depth, current_fn, queue, reported);
            }
            if let StructTailExpr::Base(e) = base {
                find_calls(cx, e, chain, depth, current_fn, queue, reported);
            }
        }
        ExprKind::Loop(block, _, _, _) => {
            for stmt in block.stmts {
                if let rustc_hir::StmtKind::Expr(e) | rustc_hir::StmtKind::Semi(e) = stmt.kind {
                    find_calls(cx, e, chain, depth, current_fn, queue, reported);
                }
            }
            if let Some(e) = block.expr {
                find_calls(cx, e, chain, depth, current_fn, queue, reported);
            }
        }
        _ => {}
    }
}

fn typeck_for_fn<'tcx>(
    cx: &LateContext<'tcx>,
    fn_def: LocalDefId,
) -> Option<&'tcx rustc_middle::ty::TypeckResults<'tcx>> {
    // Prefer the active body when it matches; otherwise query typeck for the callee.
    if cx.enclosing_body.is_some() {
        let owner = cx.tcx.hir_body_owner_def_id(cx.enclosing_body?);
        if owner == fn_def {
            return Some(cx.typeck_results());
        }
    }
    Some(cx.tcx.typeck(fn_def))
}

fn check_callee<'tcx>(
    cx: &LateContext<'tcx>,
    callee: &'tcx Expr<'tcx>,
    span: Span,
    chain: &[String],
    depth: usize,
    current_fn: LocalDefId,
    queue: &mut VecDeque<(LocalDefId, usize, Vec<String>, Option<&'tcx Body<'tcx>>)>,
    reported: &mut HashSet<(DefId, Span)>,
) {
    let Some(typeck) = typeck_for_fn(cx, current_fn) else {
        return;
    };
    if let ExprKind::Path(qpath) = callee.kind {
        let res = typeck.qpath_res(&qpath, callee.hir_id);
        if let Res::Def(_, def_id) = res {
            report_if_denied(cx, def_id, span, chain, reported);
            if let Some(local) = def_id.as_local() {
                let mut next = chain.to_vec();
                next.push(cx.tcx.item_name(def_id).to_string());
                queue.push_back((local, depth + 1, next, None));
            }
        }
    }
}

fn check_method<'tcx>(
    cx: &LateContext<'tcx>,
    expr: &'tcx Expr<'tcx>,
    chain: &[String],
    current_fn: LocalDefId,
    reported: &mut HashSet<(DefId, Span)>,
) {
    let Some(typeck) = typeck_for_fn(cx, current_fn) else {
        return;
    };
    if let Some(def_id) = typeck.type_dependent_def_id(expr.hir_id) {
        report_if_denied(cx, def_id, expr.span, chain, reported);
    }
}

fn is_denied_path(path: &str) -> bool {
    path.starts_with("std::fs")
        || path.starts_with("std::net")
        || path == "std::thread::sleep"
        || path.starts_with("std::process::Command")
        || path.ends_with("Mutex::lock")
        || path.ends_with("RwLock::read")
        || path.ends_with("RwLock::write")
}

fn report_if_denied(
    cx: &LateContext<'_>,
    def_id: DefId,
    span: Span,
    chain: &[String],
    reported: &mut HashSet<(DefId, Span)>,
) {
    let path = cx.tcx.def_path_str(def_id);
    if !is_denied_path(&path) {
        return;
    }
    if !reported.insert((def_id, span)) {
        return;
    }
    let chain_s = chain.join(" → ");
    let msg = format!("blocking call `{path}` reachable from render path: {chain_s}");
    cx.emit_span_lint(
        RENDER_THREAD_PURITY,
        span,
        DiagDecorator(move |diag| {
            diag.primary_message(msg);
            diag.note(
                "render/draw path must not perform blocking I/O, process spawn, sleep, or std::sync locks",
            );
        }),
    );
}

// Keep unused imports intentional for future trait-path matching.
#[allow(dead_code, reason = "documented residual allow; prefer expect when site is lint-true")]
fn _keep(symbol: Symbol, id: HirId, kind: ItemKind<'_>, impl_kind: ImplItemKind<'_>) {
    let _ = (symbol, id, kind, impl_kind);
}

#[cfg(test)]
mod tests;
