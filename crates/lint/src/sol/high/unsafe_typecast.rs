use super::UnsafeTypecast;
use crate::{
    linter::{LateLintPass, LintContext},
    sol::{Severity, SolLint},
};
use solar_sema::hir::{self, ExprKind, TypeKind};
use solar_ast::ElementaryType;

declare_forge_lint!(
    UNSAFE_TYPECAST,
    Severity::High,
    "unsafe-typecast",
    "unsafe typecast that may cause data loss or unexpected behavior"
);

impl<'hir> LateLintPass<'hir> for UnsafeTypecast {
    fn check_expr(
        &mut self,
        ctx: &LintContext<'_>,
        hir: &'hir hir::Hir<'hir>,
        expr: &'hir hir::Expr<'hir>,
    ) {
        // Check all calls to understand the HIR structure
        if let ExprKind::Call(call_expr, call_args, _) = &expr.kind {
            if let ExprKind::TypeCall(ty) = &call_expr.kind {
                if let Some(expr) = call_args.exprs().next() {
                    if check_unsafe_typecast(hir, ty, expr) {
                        ctx.emit(&UNSAFE_TYPECAST, expr.span);
                    }
                }
            }
        }
    }
}

/// Check if a typecast is potentially unsafe by examining the target type
/// Returns true if the cast is potentially unsafe, false if it's safe
fn check_unsafe_typecast(
    hir: &hir::Hir<'_>,
    ty: &hir::Type<'_>,
    expr: &hir::Expr<'_>,
) -> bool {
    // infer the type of the expression
    let Some(expr_type_kind) = infer_expr_type(hir, expr) else {
        return false; // Unable to infer type, assume safe
    };

    // Get the type of the cast or return false if it's not an elementary type
    let ty_kind = match ty.kind {
        TypeKind::Elementary(elem_ty) => elem_ty,
        _ => return false,
    };

    // Check if the expression type can be casted safely
    todo!();
}

/// Infer the elementary type of a HIR expression recursively
/// Returns None for types that cannot be casted (strings, complex types, etc.)
fn infer_expr_type(hir: &hir::Hir<'_>, expr: &hir::Expr<'_>) -> Option<ElementaryType> {
    match &expr.kind {
        // Literals
        ExprKind::Lit(lit) => {
            use solar_ast::LitKind;
            match &lit.kind {
                LitKind::Bool(_) => Some(ElementaryType::Bool),
                LitKind::Number(_) => {
                    todo!();
                },
                // String and other literals cannot be casted to elementary types
                _ => None,
            }
        }

        // Type calls (explicit casts) - get the type being cast to
        ExprKind::TypeCall(ty) => {
            match &ty.kind {
                TypeKind::Elementary(elem_ty) => Some(*elem_ty),
                _ => None, // Non-elementary types can't be casted
            }
        }

        // Binary operations - recursively infer from operands
        ExprKind::Binary(left, _op, right) => {
            let left_type = infer_expr_type(hir, left)?;
            let right_type = infer_expr_type(hir, right)?;
            
            todo!();
        }

        // Unary operations - preserve the type of the operand
        ExprKind::Unary(_op, operand) => {
            infer_expr_type(hir, operand)
        }

        // For other expression kinds (identifiers, calls, member access, etc.)
        _ => None,
    }
}