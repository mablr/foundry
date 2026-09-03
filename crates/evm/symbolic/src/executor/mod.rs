use super::{abi::*, runtime::*, *};

mod calls;
mod cheatcodes;
mod constraints;
mod create;
mod invariant;
mod opcodes;
mod run;

#[derive(Debug)]
struct CallOutcome {
    status: CallStatus,
    state: PathState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CallStatus {
    Success,
    Revert,
    Failure,
}

#[derive(Debug)]
struct SequencePath {
    state: PathState,
    steps: Vec<SequenceStepTemplate>,
}

#[derive(Clone, Debug)]
struct SequenceStepTemplate {
    sender: Address,
    address: Address,
    contract_name: Option<String>,
    function: Function,
    calldata: SymbolicCalldata,
}

#[derive(Debug)]
struct InvariantCheckOutcome {
    failed: bool,
    state: PathState,
}

impl SymbolicExecutor {
    pub(super) fn pop_next_path(&self, paths: &mut VecDeque<PathState>) -> Option<PathState> {
        match self.config.exploration_order {
            SymbolicExplorationOrder::Bfs => paths.pop_front(),
            SymbolicExplorationOrder::Dfs => paths.pop_back(),
        }
    }

    pub(super) fn pop_next_feasible_path(
        &mut self,
        paths: &mut VecDeque<PathState>,
        deferred_hard_arithmetic: &mut VecDeque<PathState>,
        solver_phase: &mut bool,
    ) -> Result<Option<PathState>, SymbolicError> {
        loop {
            // Keep expensive solver work off the hot path while any locally decidable state
            // remains. A hard-arithmetic miss is preserved for the complete second phase.
            while let Some(mut state) = self.pop_next_path(paths) {
                if state.take_deferred_feasibility_check() {
                    match self.branch_feasibility(&state, &state.constraints, *solver_phase)? {
                        Some(true) => {}
                        Some(false) => continue,
                        None => {
                            state.defer_feasibility_check();
                            deferred_hard_arithmetic.push_back(state);
                            continue;
                        }
                    }
                }
                return Ok(Some(state));
            }

            if *solver_phase || deferred_hard_arithmetic.is_empty() {
                return Ok(None);
            }

            // Switch once per execution worklist so descendants of retried paths also receive
            // complete branch-feasibility checks.
            *solver_phase = true;
            paths.append(deferred_hard_arithmetic);
        }
    }
}
