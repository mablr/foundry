//! Engine-independent Forge test selection.

use crate::{
    TestFilter,
    result::{SymbolicCounterexampleArtifact, SymbolicCounterexampleArtifactKind},
    symbolic_regression::SYMBOLIC_REGRESSION_MARKER,
};
use alloy_json_abi::{Function, JsonAbi};
use foundry_common::TestFunctionKind;
use foundry_compilers::ArtifactId;
use foundry_config::{Config, InlineConfig};
use std::path::PathBuf;

/// A symbolic counterexample selected for replay.
#[derive(Clone, Debug)]
pub struct SymbolicArtifactReplayConfig {
    /// Artifact payload to replay.
    pub artifact: SymbolicCounterexampleArtifact,
    /// Path the artifact was loaded from, used in diagnostics.
    pub path: PathBuf,
}

/// Classifies and filters Forge tests without an execution engine.
#[derive(Clone, Copy)]
pub(crate) struct TestFunctionMatcher<'a> {
    pub(crate) config: &'a Config,
    pub(crate) inline_config: &'a InlineConfig,
    symbolic_artifact_replay: Option<&'a SymbolicArtifactReplayConfig>,
}

impl<'a> TestFunctionMatcher<'a> {
    pub(crate) const fn new(
        config: &'a Config,
        inline_config: &'a InlineConfig,
        symbolic_artifact_replay: Option<&'a SymbolicArtifactReplayConfig>,
    ) -> Self {
        Self { config, inline_config, symbolic_artifact_replay }
    }

    fn symbolic_tests_enabled(&self, contract_id: &str) -> bool {
        self.symbolic_artifact_replay.is_some_and(|artifact| {
            artifact.artifact.kind == SymbolicCounterexampleArtifactKind::SingleCall
        }) || self.inline_config.contract_symbolic_enabled(
            &self.config.profile,
            contract_id,
            self.config.symbolic.enabled,
        )
    }

    pub(crate) fn test_function_kind(
        &self,
        contract_id: &str,
        func: &Function,
        generated_symbolic_regression: bool,
    ) -> TestFunctionKind {
        if generated_symbolic_regression && !func.name.starts_with("test_regression_") {
            return TestFunctionKind::Unknown;
        }

        TestFunctionKind::classify(
            func.name.as_str(),
            !func.inputs.is_empty(),
            self.symbolic_tests_enabled(contract_id),
        )
    }

    /// Returns the functions of `abi` accepted by `keep`, which is given the contract identifier,
    /// the function and its classification.
    pub(crate) fn test_functions(
        self,
        contract_id: String,
        abi: &JsonAbi,
        mut keep: impl FnMut(&str, &Function, TestFunctionKind) -> bool,
    ) -> impl Iterator<Item = &Function> {
        let generated_symbolic_regression = is_generated_symbolic_regression_contract(abi);
        abi.functions().filter(move |func| {
            let kind = self.test_function_kind(&contract_id, func, generated_symbolic_regression);
            keep(&contract_id, func, kind)
        })
    }

    /// Returns the test functions of `abi` that match `filter`.
    pub(crate) fn matching_test_functions<'b>(
        self,
        filter: &dyn TestFilter,
        id: &ArtifactId,
        abi: &'b JsonAbi,
    ) -> impl Iterator<Item = &'b Function> {
        self.test_functions(id.identifier(), abi, move |contract_id, func, kind| {
            filter.matches_test_function_kind_in_contract(contract_id, func, kind)
        })
    }

    pub(crate) fn matches_contract(
        &self,
        filter: &dyn TestFilter,
        id: &ArtifactId,
        abi: &JsonAbi,
    ) -> bool {
        filter.matches_path(&id.source)
            && filter.matches_contract(&id.name)
            && self.matching_test_functions(filter, id, abi).next().is_some()
    }
}

pub(crate) fn is_generated_symbolic_regression_contract(abi: &JsonAbi) -> bool {
    abi.functions().any(|func| func.name == SYMBOLIC_REGRESSION_MARKER && func.inputs.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_symbolic_regression_detection_uses_marker() {
        let mut abi = JsonAbi::new();
        let ordinary = Function::parse("test_fails()").unwrap();
        abi.functions.entry(ordinary.name.clone()).or_default().push(ordinary);
        assert!(!is_generated_symbolic_regression_contract(&abi));

        let marker = Function::parse(&format!("{SYMBOLIC_REGRESSION_MARKER}()")).unwrap();
        abi.functions.entry(marker.name.clone()).or_default().push(marker);
        assert!(is_generated_symbolic_regression_contract(&abi));
    }
}
