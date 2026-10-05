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
use rustc_hir::intravisit::{self, Visitor};
use rustc_hir::{Body, Expr, ExprKind, FnDecl};
use rustc_lint::{LateContext, LateLintPass, LintContext};
use rustc_span::Span;
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

const RENDER_ROOT_NAMES: &[&str] = &["render", "compose_pending_frame", "compose_ratatui_frame"];

impl<'tcx> LateLintPass<'tcx> for RenderThreadPurity {
    #[allow(clippy::too_many_arguments, reason = "rustc LateLintPass::check_fn signature is fixed")]
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
    let mut queue: CallQueue<'tcx> = VecDeque::new();
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
        // Every queued body owns its type-check results. Use the compiler's
        // expression walk so new HIR child forms cannot silently evade checks.
        CallVisitor {
            cx,
            chain: &chain,
            depth,
            current_fn: def_id,
            queue: &mut queue,
            reported: &mut reported,
        }
        .visit_expr(body.value);
    }
}

type CallQueue<'tcx> = VecDeque<(LocalDefId, usize, Vec<String>, Option<&'tcx Body<'tcx>>)>;

struct CallVisitor<'a, 'tcx> {
    cx: &'a LateContext<'tcx>,
    chain: &'a [String],
    depth: usize,
    current_fn: LocalDefId,
    queue: &'a mut CallQueue<'tcx>,
    reported: &'a mut HashSet<(DefId, Span)>,
}

impl<'tcx> Visitor<'tcx> for CallVisitor<'_, 'tcx> {
    fn visit_expr(&mut self, expr: &'tcx Expr<'tcx>) {
        // Closure bodies retain the explicit spawn boundary. Nested item
        // bodies are visited only through resolved local call edges.
        if let ExprKind::Closure(_) = expr.kind {
            return;
        }
        let Some(typeck) = typeck_for_fn(self.cx, self.current_fn) else {
            return;
        };
        let def_id = match expr.kind {
            ExprKind::Call(callee, _) => match callee.kind {
                ExprKind::Path(qpath) => match typeck.qpath_res(&qpath, callee.hir_id) {
                    Res::Def(_, def_id) => Some(def_id),
                    _ => None,
                },
                _ => None,
            },
            ExprKind::MethodCall(..) => typeck.type_dependent_def_id(expr.hir_id),
            _ => None,
        };
        if let Some(def_id) = def_id {
            report_if_denied(self.cx, def_id, expr.span, self.chain, self.reported);
            if let Some(local) = def_id.as_local() {
                let mut next = self.chain.to_vec();
                next.push(self.cx.tcx.item_name(def_id).to_string());
                self.queue.push_back((local, self.depth + 1, next, None));
            }
        }
        intravisit::walk_expr(self, expr);
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

#[cfg(test)]
mod tests;
