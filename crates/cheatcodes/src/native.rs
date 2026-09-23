//! Native evm2 implementations of basic state and environment cheatcodes.

use crate::{Cheatcode, CheatcodeDef, CheatsConfig, Error, Result, Vm};
use alloy_primitives::{Address, U256, map::HashMap};
use alloy_sol_types::SolValue;
use evm2::{BaseEvmTypes, Evm, SpecId, bytecode::Bytecode};
use foundry_evm_core::{
    backend::GLOBAL_FAIL_SLOT, constants::CHEATCODE_ADDRESS, eip2935::HISTORY_STORAGE_ADDRESS,
};

/// Engine-independent observations retained by a native execution.
#[derive(Default)]
pub struct Session {
    /// Assertion diagnostics, independent of journal rollback.
    pub diagnostics: Vec<String>,
    /// Deprecated selectors encountered during execution.
    pub deprecated: HashMap<&'static str, Option<&'static str>>,
}

/// Dispatches supported cheatcodes and returns `None` for unmigrated capabilities.
pub fn dispatch(
    call: &Vm::VmCalls,
    host: &mut Evm<'_, BaseEvmTypes>,
    config: &CheatsConfig,
    session: &mut Session,
) -> Option<Result> {
    macro_rules! metadata {
        ($($variant:ident),*) => { match call { $(Vm::VmCalls::$variant(c) => definition(c),)* }};
    }
    macro_rules! assertion {
        ($($variant:ident),*) => { match call { $(Vm::VmCalls::$variant(c) => c.assertion_result(),)* }};
    }
    let definition = vm_calls!(metadata);
    if let crate::spec::Status::Deprecated(replacement) = definition.status {
        session.deprecated.insert(definition.func.signature, replacement);
    }
    let name = definition.func.signature.split('(').next().unwrap();
    let mut result = if config.blocked_cheatcodes.contains(&definition.func.selector_bytes) {
        Err(Error::display("disabled during restricted execution"))
    } else if let Some(result) = vm_calls!(assertion) {
        if !config.assertions_revert
            && let Err(error) = result
        {
            session.diagnostics.push(error.to_string());
            mark_assertion_failure(host)
        } else {
            result
        }
    } else {
        apply(call, host)?
    };
    if let Err(error) = &mut result
        && error.is_str()
        && !name.contains("assert")
    {
        *error = Error::display(format_args!("vm.{name}: {error}"));
    }
    Some(result)
}

const fn definition<C: CheatcodeDef>(_: &C) -> &'static crate::spec::Cheatcode<'static> {
    C::CHEATCODE
}

fn apply(call: &Vm::VmCalls, host: &mut Evm<'_, BaseEvmTypes>) -> Option<Result> {
    let supported = match call {
        Vm::VmCalls::warp(_)
        | Vm::VmCalls::coinbase(_)
        | Vm::VmCalls::fee(_)
        | Vm::VmCalls::prevrandao_0(_)
        | Vm::VmCalls::prevrandao_1(_)
        | Vm::VmCalls::getBlockTimestamp(_)
        | Vm::VmCalls::getBlockNumber(_)
        | Vm::VmCalls::getChainId(_)
        | Vm::VmCalls::load(_)
        | Vm::VmCalls::store(_)
        | Vm::VmCalls::getNonce_0(_)
        | Vm::VmCalls::getNonce_1(_) => true,
        Vm::VmCalls::roll(_) => host.spec_id() < SpecId::PRAGUE,
        Vm::VmCalls::etch(c) => c.target != HISTORY_STORAGE_ADDRESS,
        _ => false,
    };
    if !supported {
        return None;
    }
    let mut block = *host.block();
    Some((|| -> Result {
        match call {
            Vm::VmCalls::warp(c) => block.timestamp = c.newTimestamp,
            Vm::VmCalls::coinbase(c) => block.beneficiary = c.newCoinbase,
            Vm::VmCalls::fee(c) => {
                ensure!(c.newBasefee <= U256::from(u64::MAX), "base fee must be less than 2^64");
                block.basefee = c.newBasefee;
            }
            Vm::VmCalls::roll(c) if host.spec_id() < SpecId::PRAGUE => block.number = c.newHeight,
            Vm::VmCalls::prevrandao_0(c) => block.prevrandao = c.newPrevrandao.into(),
            Vm::VmCalls::prevrandao_1(c) => block.prevrandao = c.newPrevrandao,
            Vm::VmCalls::getBlockTimestamp(_) => return Ok(block.timestamp.abi_encode()),
            Vm::VmCalls::getBlockNumber(_) => return Ok(block.number.abi_encode()),
            Vm::VmCalls::getChainId(_) => {
                return Ok(U256::from(host.version().chain_id).abi_encode());
            }
            Vm::VmCalls::load(c) => {
                let mut account = host
                    .state_mut()
                    .account(&c.target, false)
                    .map_err(|error| fmt_err!("native state access failed: {error:?}"))?;
                account.warm();
                drop(account);
                let mut slot = host
                    .state_mut()
                    .storage_slot(&c.target, c.slot.into(), false)
                    .map_err(|error| fmt_err!("native state access failed: {error:?}"))?;
                slot.warm();
                return Ok(slot.current().abi_encode());
            }
            Vm::VmCalls::store(c) => {
                ensure_not_precompile(host, &c.target)?;
                let mut account = host
                    .state_mut()
                    .account(&c.target, false)
                    .map_err(|error| fmt_err!("native state access failed: {error:?}"))?;
                account.warm();
                account.touch();
                drop(account);
                let mut slot = host
                    .state_mut()
                    .storage_slot(&c.target, c.slot.into(), false)
                    .map_err(|error| fmt_err!("native state access failed: {error:?}"))?;
                slot.warm();
                slot.set(c.value.into());
            }
            Vm::VmCalls::etch(c) if c.target != HISTORY_STORAGE_ADDRESS => {
                ensure_not_precompile(host, &c.target)?;
                let mut account = host
                    .state_mut()
                    .account(&c.target, false)
                    .map_err(|error| fmt_err!("native state access failed: {error:?}"))?;
                account.warm();
                let code = Bytecode::new_raw_checked(c.newRuntimeBytecode.clone())
                    .map_err(|error| fmt_err!("failed to create bytecode: {error}"))?;
                account.touch();
                account.set_code_slow(code);
            }
            Vm::VmCalls::getNonce_0(c) => {
                let mut account = host
                    .state_mut()
                    .account(&c.account, false)
                    .map_err(|error| fmt_err!("native state access failed: {error:?}"))?;
                account.warm();
                return Ok(account.nonce().abi_encode());
            }
            Vm::VmCalls::getNonce_1(c) => {
                let mut account = host
                    .state_mut()
                    .account(&c.wallet.addr, false)
                    .map_err(|error| fmt_err!("native state access failed: {error:?}"))?;
                account.warm();
                return Ok(account.nonce().abi_encode());
            }
            _ => unreachable!("capability checked before dispatch"),
        }
        host.set_block(block);
        Ok(Vec::new())
    })())
}

fn ensure_not_precompile(host: &Evm<'_, BaseEvmTypes>, address: &Address) -> Result<()> {
    ensure!(
        !host.precompiles().contains(address),
        "cannot use precompile {address} as an argument"
    );
    Ok(())
}

fn mark_assertion_failure(host: &mut Evm<'_, BaseEvmTypes>) -> Result {
    let mut account = host
        .state_mut()
        .account(&CHEATCODE_ADDRESS, false)
        .map_err(|error| fmt_err!("native state access failed: {error:?}"))?;
    account.warm();
    account.touch();
    drop(account);
    let mut slot = host
        .state_mut()
        .storage_slot(&CHEATCODE_ADDRESS, GLOBAL_FAIL_SLOT, false)
        .map_err(|error| fmt_err!("native state access failed: {error:?}"))?;
    slot.warm();
    slot.set(U256::from(1));
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_sol_types::SolCall;
    use evm2::{Precompiles, ethereum::ethereum_tx_registry, evm::InMemoryDB};

    #[test]
    fn restricted_execution_rejects_before_mutating_native_state() {
        let mut host = Evm::<BaseEvmTypes>::new(
            SpecId::CANCUN,
            Default::default(),
            ethereum_tx_registry(SpecId::CANCUN),
            InMemoryDB::default(),
            Precompiles::base(SpecId::CANCUN),
        );
        let config =
            CheatsConfig { blocked_cheatcodes: vec![Vm::warpCall::SELECTOR], ..Default::default() };
        let before = host.block().timestamp;
        let call = Vm::VmCalls::warp(Vm::warpCall { newTimestamp: U256::from(1234) });
        let error =
            dispatch(&call, &mut host, &config, &mut Session::default()).unwrap().unwrap_err();
        assert_eq!(error.to_string(), "vm.warp: disabled during restricted execution");
        assert_eq!(host.block().timestamp, before);
    }
}
