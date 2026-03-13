use crate::{Cheatcode, Cheatcodes, Result, Vm::*};
use alloy_sol_types::SolValue;
use base64::prelude::*;

impl Cheatcode for toBase64_0Call {
    fn apply<BLOCK>(&self, _state: &mut Cheatcodes<BLOCK>) -> Result {
        let Self { data } = self;
        Ok(BASE64_STANDARD.encode(data).abi_encode())
    }
}

impl Cheatcode for toBase64_1Call {
    fn apply<BLOCK>(&self, _state: &mut Cheatcodes<BLOCK>) -> Result {
        let Self { data } = self;
        Ok(BASE64_STANDARD.encode(data).abi_encode())
    }
}

impl Cheatcode for toBase64URL_0Call {
    fn apply<BLOCK>(&self, _state: &mut Cheatcodes<BLOCK>) -> Result {
        let Self { data } = self;
        Ok(BASE64_URL_SAFE.encode(data).abi_encode())
    }
}

impl Cheatcode for toBase64URL_1Call {
    fn apply<BLOCK>(&self, _state: &mut Cheatcodes<BLOCK>) -> Result {
        let Self { data } = self;
        Ok(BASE64_URL_SAFE.encode(data).abi_encode())
    }
}
