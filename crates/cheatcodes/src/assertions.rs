//! Engine-independent assertion evaluation and formatting.

use crate::{Result, Vm::*};
use alloy_primitives::{I256, U256, U512, uint};
use foundry_evm_core::{
    abi::console::{format_units_int, format_units_uint},
    decode::ASSERTION_FAILED_PREFIX,
};
use itertools::Itertools;
use std::{borrow::Cow, fmt};

const EQ_REL_DELTA_RESOLUTION: U256 = uint!(18_U256);

struct ComparisonAssertionError<'a, T> {
    kind: AssertionKind,
    left: &'a T,
    right: &'a T,
}

#[derive(Clone, Copy)]
enum AssertionKind {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
}

impl AssertionKind {
    const fn inverse(self) -> Self {
        match self {
            Self::Eq => Self::Ne,
            Self::Ne => Self::Eq,
            Self::Gt => Self::Le,
            Self::Ge => Self::Lt,
            Self::Lt => Self::Ge,
            Self::Le => Self::Gt,
        }
    }

    const fn to_str(self) -> &'static str {
        match self {
            Self::Eq => "==",
            Self::Ne => "!=",
            Self::Gt => ">",
            Self::Ge => ">=",
            Self::Lt => "<",
            Self::Le => "<=",
        }
    }
}

impl<T> ComparisonAssertionError<'_, T> {
    fn format_values<D: fmt::Display>(&self, f: impl Fn(&T) -> D) -> String {
        format!("{} {} {}", f(self.left), self.kind.inverse().to_str(), f(self.right))
    }
}

impl<T: fmt::Display> ComparisonAssertionError<'_, T> {
    fn format_for_values(&self) -> String {
        self.format_values(T::to_string)
    }
}

impl<T: fmt::Display> ComparisonAssertionError<'_, Vec<T>> {
    fn format_for_arrays(&self) -> String {
        self.format_values(|v| format!("[{}]", v.iter().format(", ")))
    }
}

impl ComparisonAssertionError<'_, U256> {
    fn format_with_decimals(&self, decimals: &U256) -> String {
        self.format_values(|v| format_units_uint(v, decimals))
    }
}

impl ComparisonAssertionError<'_, I256> {
    fn format_with_decimals(&self, decimals: &U256) -> String {
        self.format_values(|v| format_units_int(v, decimals))
    }
}

#[derive(thiserror::Error, Debug)]
#[error("{left} !~= {right} (max delta: {max_delta}, real delta: {real_delta})")]
struct EqAbsAssertionError<T, D> {
    left: T,
    right: T,
    max_delta: D,
    real_delta: D,
}

impl EqAbsAssertionError<U256, U256> {
    fn format_with_decimals(&self, decimals: &U256) -> String {
        format!(
            "{} !~= {} (max delta: {}, real delta: {})",
            format_units_uint(&self.left, decimals),
            format_units_uint(&self.right, decimals),
            format_units_uint(&self.max_delta, decimals),
            format_units_uint(&self.real_delta, decimals),
        )
    }
}

impl EqAbsAssertionError<I256, U256> {
    fn format_with_decimals(&self, decimals: &U256) -> String {
        format!(
            "{} !~= {} (max delta: {}, real delta: {})",
            format_units_int(&self.left, decimals),
            format_units_int(&self.right, decimals),
            format_units_uint(&self.max_delta, decimals),
            format_units_uint(&self.real_delta, decimals),
        )
    }
}

fn format_delta_percent(delta: &U256) -> String {
    format!("{}%", format_units_uint(delta, &(EQ_REL_DELTA_RESOLUTION - U256::from(2))))
}

#[derive(Debug)]
enum EqRelDelta {
    Defined(U256),
    Undefined,
}

impl fmt::Display for EqRelDelta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Defined(delta) => write!(f, "{}", format_delta_percent(delta)),
            Self::Undefined => write!(f, "undefined"),
        }
    }
}

#[derive(thiserror::Error, Debug)]
#[error(
    "{left} !~= {right} (max delta: {}, real delta: {})",
    format_delta_percent(max_delta),
    real_delta
)]
struct EqRelAssertionFailure<T> {
    left: T,
    right: T,
    max_delta: U256,
    real_delta: EqRelDelta,
}

#[derive(thiserror::Error, Debug)]
enum EqRelAssertionError<T> {
    #[error(transparent)]
    Failure(Box<EqRelAssertionFailure<T>>),
    #[error("overflow in delta calculation")]
    Overflow,
}

impl EqRelAssertionError<U256> {
    fn format_with_decimals(&self, decimals: &U256) -> String {
        match self {
            Self::Failure(f) => format!(
                "{} !~= {} (max delta: {}, real delta: {})",
                format_units_uint(&f.left, decimals),
                format_units_uint(&f.right, decimals),
                format_delta_percent(&f.max_delta),
                f.real_delta,
            ),
            Self::Overflow => self.to_string(),
        }
    }
}

impl EqRelAssertionError<I256> {
    fn format_with_decimals(&self, decimals: &U256) -> String {
        match self {
            Self::Failure(f) => format!(
                "{} !~= {} (max delta: {}, real delta: {})",
                format_units_int(&f.left, decimals),
                format_units_int(&f.right, decimals),
                format_delta_percent(&f.max_delta),
                f.real_delta,
            ),
            Self::Overflow => self.to_string(),
        }
    }
}

type ComparisonResult<'a, T> = Result<(), ComparisonAssertionError<'a, T>>;

fn format_assertion_error<'a, E>(
    err: E,
    error_formatter: Option<&dyn Fn(&E) -> String>,
    error_msg: Option<&'a str>,
) -> Cow<'a, str> {
    let error_msg = error_msg.unwrap_or(ASSERTION_FAILED_PREFIX);
    if let Some(formatter) = error_formatter {
        Cow::Owned(format!("{error_msg}: {}", formatter(&err)))
    } else {
        Cow::Borrowed(error_msg)
    }
}

// Keep each assertion's argument binding, comparison and formatting together.
macro_rules! assertions {
    ($( $args:tt, $body:expr, $formatter:expr, [$(($plain:ident, $plain_call:ident, $custom:ident, $custom_call:ident)),* $(,)?]; )*) => {
        /// Evaluates assertion cheatcodes without an execution engine or journal.
        pub(crate) fn evaluate(call: &VmCalls) -> Option<Result> {
            Some(match call {
                $($(
                    VmCalls::$plain(value) => assertions!(@plain value, $plain_call, $args, $body, $formatter),
                    VmCalls::$custom(value) => assertions!(@custom value, $custom_call, $args, $body, $formatter),
                )*)*
                _ => return None,
            })
        }
    };
    (@plain $value:ident, $call:ident, ($($arg:ident),*), $body:expr, $formatter:expr) => {{
        let $call { $($arg),* } = $value;
        $body.map(|()| Vec::new()).map_err(|error| {
            format_assertion_error(error, $formatter, None).into_owned().into()
        })
    }};
    (@custom $value:ident, $call:ident, ($($arg:ident),*), $body:expr, $formatter:expr) => {{
        let $call { $($arg,)* err } = $value;
        $body.map(|()| Vec::new()).map_err(|error| {
            format_assertion_error(error, $formatter, Some(err)).into_owned().into()
        })
    }};
}

assertions! {
    (condition), assert_true(*condition), None, [
        (assertTrue_0, assertTrue_0Call, assertTrue_1, assertTrue_1Call),
    ];
    (condition), assert_false(*condition), None, [
        (assertFalse_0, assertFalse_0Call, assertFalse_1, assertFalse_1Call),
    ];
    (left, right), assert_eq(left, right), Some(&ComparisonAssertionError::format_for_values), [
        (assertEq_0, assertEq_0Call, assertEq_1, assertEq_1Call),
        (assertEq_2, assertEq_2Call, assertEq_3, assertEq_3Call),
        (assertEq_4, assertEq_4Call, assertEq_5, assertEq_5Call),
        (assertEq_6, assertEq_6Call, assertEq_7, assertEq_7Call),
        (assertEq_8, assertEq_8Call, assertEq_9, assertEq_9Call),
        (assertEq_10, assertEq_10Call, assertEq_11, assertEq_11Call),
        (assertEq_12, assertEq_12Call, assertEq_13, assertEq_13Call),
    ];
    (left, right), assert_eq(left, right), Some(&ComparisonAssertionError::format_for_arrays), [
        (assertEq_14, assertEq_14Call, assertEq_15, assertEq_15Call),
        (assertEq_16, assertEq_16Call, assertEq_17, assertEq_17Call),
        (assertEq_18, assertEq_18Call, assertEq_19, assertEq_19Call),
        (assertEq_20, assertEq_20Call, assertEq_21, assertEq_21Call),
        (assertEq_22, assertEq_22Call, assertEq_23, assertEq_23Call),
        (assertEq_24, assertEq_24Call, assertEq_25, assertEq_25Call),
        (assertEq_26, assertEq_26Call, assertEq_27, assertEq_27Call),
    ];
    (left, right, decimals), assert_eq(left, right), Some(&|e| e.format_with_decimals(decimals)), [
        (assertEqDecimal_0, assertEqDecimal_0Call, assertEqDecimal_1, assertEqDecimal_1Call),
        (assertEqDecimal_2, assertEqDecimal_2Call, assertEqDecimal_3, assertEqDecimal_3Call),
    ];
    (left, right), assert_not_eq(left, right), Some(&ComparisonAssertionError::format_for_values), [
        (assertNotEq_0, assertNotEq_0Call, assertNotEq_1, assertNotEq_1Call),
        (assertNotEq_2, assertNotEq_2Call, assertNotEq_3, assertNotEq_3Call),
        (assertNotEq_4, assertNotEq_4Call, assertNotEq_5, assertNotEq_5Call),
        (assertNotEq_6, assertNotEq_6Call, assertNotEq_7, assertNotEq_7Call),
        (assertNotEq_8, assertNotEq_8Call, assertNotEq_9, assertNotEq_9Call),
        (assertNotEq_10, assertNotEq_10Call, assertNotEq_11, assertNotEq_11Call),
        (assertNotEq_12, assertNotEq_12Call, assertNotEq_13, assertNotEq_13Call),
    ];
    (left, right), assert_not_eq(left, right), Some(&ComparisonAssertionError::format_for_arrays), [
        (assertNotEq_14, assertNotEq_14Call, assertNotEq_15, assertNotEq_15Call),
        (assertNotEq_16, assertNotEq_16Call, assertNotEq_17, assertNotEq_17Call),
        (assertNotEq_18, assertNotEq_18Call, assertNotEq_19, assertNotEq_19Call),
        (assertNotEq_20, assertNotEq_20Call, assertNotEq_21, assertNotEq_21Call),
        (assertNotEq_22, assertNotEq_22Call, assertNotEq_23, assertNotEq_23Call),
        (assertNotEq_24, assertNotEq_24Call, assertNotEq_25, assertNotEq_25Call),
        (assertNotEq_26, assertNotEq_26Call, assertNotEq_27, assertNotEq_27Call),
    ];
    (left, right, decimals), assert_not_eq(left, right), Some(&|e| e.format_with_decimals(decimals)), [
        (assertNotEqDecimal_0, assertNotEqDecimal_0Call, assertNotEqDecimal_1, assertNotEqDecimal_1Call),
        (assertNotEqDecimal_2, assertNotEqDecimal_2Call, assertNotEqDecimal_3, assertNotEqDecimal_3Call),
    ];
    (left, right), assert_gt(left, right), Some(&ComparisonAssertionError::format_for_values), [
        (assertGt_0, assertGt_0Call, assertGt_1, assertGt_1Call),
        (assertGt_2, assertGt_2Call, assertGt_3, assertGt_3Call),
    ];
    (left, right, decimals), assert_gt(left, right), Some(&|e| e.format_with_decimals(decimals)), [
        (assertGtDecimal_0, assertGtDecimal_0Call, assertGtDecimal_1, assertGtDecimal_1Call),
        (assertGtDecimal_2, assertGtDecimal_2Call, assertGtDecimal_3, assertGtDecimal_3Call),
    ];
    (left, right), assert_ge(left, right), Some(&ComparisonAssertionError::format_for_values), [
        (assertGe_0, assertGe_0Call, assertGe_1, assertGe_1Call),
        (assertGe_2, assertGe_2Call, assertGe_3, assertGe_3Call),
    ];
    (left, right, decimals), assert_ge(left, right), Some(&|e| e.format_with_decimals(decimals)), [
        (assertGeDecimal_0, assertGeDecimal_0Call, assertGeDecimal_1, assertGeDecimal_1Call),
        (assertGeDecimal_2, assertGeDecimal_2Call, assertGeDecimal_3, assertGeDecimal_3Call),
    ];
    (left, right), assert_lt(left, right), Some(&ComparisonAssertionError::format_for_values), [
        (assertLt_0, assertLt_0Call, assertLt_1, assertLt_1Call),
        (assertLt_2, assertLt_2Call, assertLt_3, assertLt_3Call),
    ];
    (left, right, decimals), assert_lt(left, right), Some(&|e| e.format_with_decimals(decimals)), [
        (assertLtDecimal_0, assertLtDecimal_0Call, assertLtDecimal_1, assertLtDecimal_1Call),
        (assertLtDecimal_2, assertLtDecimal_2Call, assertLtDecimal_3, assertLtDecimal_3Call),
    ];
    (left, right), assert_le(left, right), Some(&ComparisonAssertionError::format_for_values), [
        (assertLe_0, assertLe_0Call, assertLe_1, assertLe_1Call),
        (assertLe_2, assertLe_2Call, assertLe_3, assertLe_3Call),
    ];
    (left, right, decimals), assert_le(left, right), Some(&|e| e.format_with_decimals(decimals)), [
        (assertLeDecimal_0, assertLeDecimal_0Call, assertLeDecimal_1, assertLeDecimal_1Call),
        (assertLeDecimal_2, assertLeDecimal_2Call, assertLeDecimal_3, assertLeDecimal_3Call),
    ];
    (left, right, maxDelta), uint_assert_approx_eq_abs(*left, *right, *maxDelta), Some(&ToString::to_string), [
        (assertApproxEqAbs_0, assertApproxEqAbs_0Call, assertApproxEqAbs_1, assertApproxEqAbs_1Call),
    ];
    (left, right, maxDelta), int_assert_approx_eq_abs(*left, *right, *maxDelta), Some(&ToString::to_string), [
        (assertApproxEqAbs_2, assertApproxEqAbs_2Call, assertApproxEqAbs_3, assertApproxEqAbs_3Call),
    ];
    (left, right, decimals, maxDelta), uint_assert_approx_eq_abs(*left, *right, *maxDelta), Some(&|e| e.format_with_decimals(decimals)), [
        (assertApproxEqAbsDecimal_0, assertApproxEqAbsDecimal_0Call, assertApproxEqAbsDecimal_1, assertApproxEqAbsDecimal_1Call),
    ];
    (left, right, decimals, maxDelta), int_assert_approx_eq_abs(*left, *right, *maxDelta), Some(&|e| e.format_with_decimals(decimals)), [
        (assertApproxEqAbsDecimal_2, assertApproxEqAbsDecimal_2Call, assertApproxEqAbsDecimal_3, assertApproxEqAbsDecimal_3Call),
    ];
    (left, right, maxPercentDelta), uint_assert_approx_eq_rel(*left, *right, *maxPercentDelta), Some(&ToString::to_string), [
        (assertApproxEqRel_0, assertApproxEqRel_0Call, assertApproxEqRel_1, assertApproxEqRel_1Call),
    ];
    (left, right, maxPercentDelta), int_assert_approx_eq_rel(*left, *right, *maxPercentDelta), Some(&ToString::to_string), [
        (assertApproxEqRel_2, assertApproxEqRel_2Call, assertApproxEqRel_3, assertApproxEqRel_3Call),
    ];
    (left, right, decimals, maxPercentDelta), uint_assert_approx_eq_rel(*left, *right, *maxPercentDelta), Some(&|e| e.format_with_decimals(decimals)), [
        (assertApproxEqRelDecimal_0, assertApproxEqRelDecimal_0Call, assertApproxEqRelDecimal_1, assertApproxEqRelDecimal_1Call),
    ];
    (left, right, decimals, maxPercentDelta), int_assert_approx_eq_rel(*left, *right, *maxPercentDelta), Some(&|e| e.format_with_decimals(decimals)), [
        (assertApproxEqRelDecimal_2, assertApproxEqRelDecimal_2Call, assertApproxEqRelDecimal_3, assertApproxEqRelDecimal_3Call),
    ];
}

const fn assert_true(condition: bool) -> Result<(), ()> {
    if condition { Ok(()) } else { Err(()) }
}

const fn assert_false(condition: bool) -> Result<(), ()> {
    assert_true(!condition)
}

fn assert_eq<'a, T: PartialEq>(left: &'a T, right: &'a T) -> ComparisonResult<'a, T> {
    if left == right {
        Ok(())
    } else {
        Err(ComparisonAssertionError { kind: AssertionKind::Eq, left, right })
    }
}

fn assert_not_eq<'a, T: PartialEq>(left: &'a T, right: &'a T) -> ComparisonResult<'a, T> {
    if left == right {
        Err(ComparisonAssertionError { kind: AssertionKind::Ne, left, right })
    } else {
        Ok(())
    }
}

fn assert_gt<'a, T: PartialOrd>(left: &'a T, right: &'a T) -> ComparisonResult<'a, T> {
    if left > right {
        Ok(())
    } else {
        Err(ComparisonAssertionError { kind: AssertionKind::Gt, left, right })
    }
}

fn assert_ge<'a, T: PartialOrd>(left: &'a T, right: &'a T) -> ComparisonResult<'a, T> {
    if left >= right {
        Ok(())
    } else {
        Err(ComparisonAssertionError { kind: AssertionKind::Ge, left, right })
    }
}

fn assert_lt<'a, T: PartialOrd>(left: &'a T, right: &'a T) -> ComparisonResult<'a, T> {
    if left < right {
        Ok(())
    } else {
        Err(ComparisonAssertionError { kind: AssertionKind::Lt, left, right })
    }
}

fn assert_le<'a, T: PartialOrd>(left: &'a T, right: &'a T) -> ComparisonResult<'a, T> {
    if left <= right {
        Ok(())
    } else {
        Err(ComparisonAssertionError { kind: AssertionKind::Le, left, right })
    }
}

fn get_delta_int(left: I256, right: I256) -> U256 {
    let (left_sign, left_abs) = left.into_sign_and_abs();
    let (right_sign, right_abs) = right.into_sign_and_abs();

    if left_sign == right_sign {
        if left_abs > right_abs { left_abs - right_abs } else { right_abs - left_abs }
    } else {
        left_abs.wrapping_add(right_abs)
    }
}

/// Calculates the relative delta for an absolute difference.
///
/// Avoids overflow in the multiplication by using [`U512`] to hold the intermediary result.
fn calc_delta_full<T>(abs_diff: U256, right: U256) -> Result<U256, EqRelAssertionError<T>> {
    let delta = U512::from(abs_diff) * U512::from(10).pow(U512::from(EQ_REL_DELTA_RESOLUTION))
        / U512::from(right);
    U256::checked_from_limbs_slice(delta.as_limbs()).ok_or(EqRelAssertionError::Overflow)
}

fn uint_assert_approx_eq_abs(
    left: U256,
    right: U256,
    max_delta: U256,
) -> Result<(), Box<EqAbsAssertionError<U256, U256>>> {
    let delta = left.abs_diff(right);

    if delta <= max_delta {
        Ok(())
    } else {
        Err(Box::new(EqAbsAssertionError { left, right, max_delta, real_delta: delta }))
    }
}

fn int_assert_approx_eq_abs(
    left: I256,
    right: I256,
    max_delta: U256,
) -> Result<(), Box<EqAbsAssertionError<I256, U256>>> {
    let delta = get_delta_int(left, right);

    if delta <= max_delta {
        Ok(())
    } else {
        Err(Box::new(EqAbsAssertionError { left, right, max_delta, real_delta: delta }))
    }
}

fn uint_assert_approx_eq_rel(
    left: U256,
    right: U256,
    max_delta: U256,
) -> Result<(), EqRelAssertionError<U256>> {
    if right.is_zero() {
        if left.is_zero() {
            return Ok(());
        }
        return Err(EqRelAssertionError::Failure(Box::new(EqRelAssertionFailure {
            left,
            right,
            max_delta,
            real_delta: EqRelDelta::Undefined,
        })));
    }

    let delta = calc_delta_full::<U256>(left.abs_diff(right), right)?;

    if delta <= max_delta {
        Ok(())
    } else {
        Err(EqRelAssertionError::Failure(Box::new(EqRelAssertionFailure {
            left,
            right,
            max_delta,
            real_delta: EqRelDelta::Defined(delta),
        })))
    }
}

fn int_assert_approx_eq_rel(
    left: I256,
    right: I256,
    max_delta: U256,
) -> Result<(), EqRelAssertionError<I256>> {
    if right.is_zero() {
        if left.is_zero() {
            return Ok(());
        }
        return Err(EqRelAssertionError::Failure(Box::new(EqRelAssertionFailure {
            left,
            right,
            max_delta,
            real_delta: EqRelDelta::Undefined,
        })));
    }

    let delta = calc_delta_full::<I256>(get_delta_int(left, right), right.unsigned_abs())?;

    if delta <= max_delta {
        Ok(())
    } else {
        Err(EqRelAssertionError::Failure(Box::new(EqRelAssertionFailure {
            left,
            right,
            max_delta,
            real_delta: EqRelDelta::Defined(delta),
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_preserves_assertion_messages() {
        let call =
            VmCalls::assertEq_2(assertEq_2Call { left: U256::from(1), right: U256::from(2) });
        assert_eq!(evaluate(&call).unwrap().unwrap_err().to_string(), "assertion failed: 1 != 2");
        let call = VmCalls::assertEq_3(assertEq_3Call {
            left: U256::from(1),
            right: U256::from(2),
            err: "balance".into(),
        });
        assert_eq!(evaluate(&call).unwrap().unwrap_err().to_string(), "balance: 1 != 2");
        let call =
            VmCalls::assertTrue_1(assertTrue_1Call { condition: false, err: "condition".into() });
        assert_eq!(evaluate(&call).unwrap().unwrap_err().to_string(), "condition");
    }

    #[test]
    fn dispatch_distinguishes_success_from_unsupported() {
        let call = VmCalls::assertTrue_0(assertTrue_0Call { condition: true });
        assert_eq!(evaluate(&call).unwrap().unwrap(), Vec::<u8>::new());
        let call = VmCalls::getBlockNumber(getBlockNumberCall {});
        assert!(evaluate(&call).is_none());
    }
}
