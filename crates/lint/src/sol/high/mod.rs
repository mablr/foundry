use crate::sol::{EarlyLintPass, LateLintPass, SolLint};

mod incorrect_shift;
mod unchecked_calls;
mod unsafe_typecast;

use incorrect_shift::INCORRECT_SHIFT;
use unchecked_calls::{ERC20_UNCHECKED_TRANSFER, UNCHECKED_CALL};
use unsafe_typecast::UNSAFE_TYPECAST;

register_lints!(
    (IncorrectShift, early, (INCORRECT_SHIFT)),
    (UncheckedCall, early, (UNCHECKED_CALL)),
    (UncheckedTransferERC20, early, (ERC20_UNCHECKED_TRANSFER)),
    (UnsafeTypecast, late, (UNSAFE_TYPECAST))
);
