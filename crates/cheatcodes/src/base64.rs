use crate::{Cheatcode, Cheatcodes, Result, Vm::*};
use alloy_network::Network;
use alloy_sol_types::SolValue;
use base64::prelude::*;
use foundry_evm_core::EthCheatCtx;

impl<CTX: EthCheatCtx, N: Network> Cheatcode<CTX, N> for toBase64_0Call {
    fn apply(&self, _state: &mut Cheatcodes<CTX, N>) -> Result {
        let Self { data } = self;
        Ok(BASE64_STANDARD.encode(data).abi_encode())
    }
}

impl<CTX: EthCheatCtx, N: Network> Cheatcode<CTX, N> for toBase64_1Call {
    fn apply(&self, _state: &mut Cheatcodes<CTX, N>) -> Result {
        let Self { data } = self;
        Ok(BASE64_STANDARD.encode(data).abi_encode())
    }
}

impl<CTX: EthCheatCtx, N: Network> Cheatcode<CTX, N> for toBase64URL_0Call {
    fn apply(&self, _state: &mut Cheatcodes<CTX, N>) -> Result {
        let Self { data } = self;
        Ok(BASE64_URL_SAFE.encode(data).abi_encode())
    }
}

impl<CTX: EthCheatCtx, N: Network> Cheatcode<CTX, N> for toBase64URL_1Call {
    fn apply(&self, _state: &mut Cheatcodes<CTX, N>) -> Result {
        let Self { data } = self;
        Ok(BASE64_URL_SAFE.encode(data).abi_encode())
    }
}
