use crate::{
    Cast, SimpleCast,
    opts::{Cast as CastArgs, CastSubcommand, ToBaseArgs},
    traces::identifier::SignaturesIdentifier,
};
use alloy_consensus::transaction::{Recovered, SignerRecoverable};
use alloy_dyn_abi::{DynSolValue, ErrorExt, EventExt};
use alloy_eips::eip7702::SignedAuthorization;
use alloy_ens::{ProviderEnsExt, namehash};
use alloy_primitives::{Address, B256, eip191_hash_message, hex, keccak256};
use alloy_provider::Provider;
use alloy_rpc_types::{BlockId, BlockNumberOrTag::Latest};
use clap::{CommandFactory, Parser};
use clap_complete::generate;
use eyre::Result;
use foundry_cli::{handler, utils, utils::LoadConfig};
use foundry_common::{
    abi::{get_error, get_event},
    fmt::{format_tokens, format_tokens_raw, format_uint_exp},
    fs,
    selectors::{
        ParsedSignatures, SelectorImportData, SelectorKind, decode_calldata, decode_event_topic,
        decode_function_selector, decode_selectors, import_selectors, parse_signatures,
        pretty_calldata,
    },
    shell, stdin,
};
use std::time::Instant;

/// Run the `cast` command-line interface.
pub fn run() -> Result<()> {
    setup()?;

    let args = CastArgs::parse();
    args.global.init()?;
    args.global.tokio_runtime().block_on(run_command(args))
}

/// Setup the global logger and other utilities.
pub fn setup() -> Result<()> {
    utils::install_crypto_provider();
    handler::install();
    utils::load_dotenv();
    utils::subscriber();
    utils::enable_paint();

    Ok(())
}

/// Run the subcommand.
pub async fn run_command(args: CastArgs) -> Result<()> {
    match args.cmd {
        // Constants
        CastSubcommand::MaxInt { r#type } => {
            sh_println!("{}", SimpleCast::max_int(&r#type)?)?;
        }
        CastSubcommand::MinInt { r#type } => {
            sh_println!("{}", SimpleCast::min_int(&r#type)?)?;
        }
        CastSubcommand::MaxUint { r#type } => {
            sh_println!("{}", SimpleCast::max_int(&r#type)?)?;
        }
        CastSubcommand::AddressZero => {
            sh_println!("{:?}", Address::ZERO)?;
        }
        CastSubcommand::HashZero => {
            sh_println!("{:?}", B256::ZERO)?;
        }

        // Conversions & transformations
        CastSubcommand::FromUtf8 { text } => {
            let value = stdin::unwrap(text, false)?;
            sh_println!("{}", SimpleCast::from_utf8(&value))?
        }
        CastSubcommand::ToAscii { hexdata } => {
            let value = stdin::unwrap(hexdata, false)?;
            sh_println!("{}", SimpleCast::to_ascii(value.trim())?)?
        }
        CastSubcommand::ToUtf8 { hexdata } => {
            let value = stdin::unwrap(hexdata, false)?;
            sh_println!("{}", SimpleCast::to_utf8(&value)?)?
        }
        CastSubcommand::FromFixedPoint { value, decimals } => {
            let (value, decimals) = stdin::unwrap2(value, decimals)?;
            sh_println!("{}", SimpleCast::from_fixed_point(&value, &decimals)?)?
        }
        CastSubcommand::ToFixedPoint { value, decimals } => {
            let (value, decimals) = stdin::unwrap2(value, decimals)?;
            sh_println!("{}", SimpleCast::to_fixed_point(&value, &decimals)?)?
        }
        CastSubcommand::ConcatHex { data } => {
            if data.is_empty() {
                let s = stdin::read(true)?;
                sh_println!("{}", SimpleCast::concat_hex(s.split_whitespace()))?
            } else {
                sh_println!("{}", SimpleCast::concat_hex(data))?
            }
        }
        CastSubcommand::FromBin => {
            let hex = stdin::read_bytes(false)?;
            sh_println!("{}", hex::encode_prefixed(hex))?
        }
        CastSubcommand::ToHexdata { input } => {
            let value = stdin::unwrap_line(input)?;
            let output = match value {
                s if s.starts_with('@') => hex::encode(std::env::var(&s[1..])?),
                s if s.starts_with('/') => hex::encode(fs::read(s)?),
                s => s.split(':').map(|s| s.trim_start_matches("0x").to_lowercase()).collect(),
            };
            sh_println!("0x{output}")?
        }
        CastSubcommand::ToCheckSumAddress { address, chain_id } => {
            let value = stdin::unwrap_line(address)?;
            sh_println!("{}", value.to_checksum(chain_id))?
        }
        CastSubcommand::ToUint256 { value } => {
            let value = stdin::unwrap_line(value)?;
            sh_println!("{}", SimpleCast::to_uint256(&value)?)?
        }
        CastSubcommand::ToInt256 { value } => {
            let value = stdin::unwrap_line(value)?;
            sh_println!("{}", SimpleCast::to_int256(&value)?)?
        }
        CastSubcommand::ToUnit { value, unit } => {
            let value = stdin::unwrap_line(value)?;
            sh_println!("{}", SimpleCast::to_unit(&value, &unit)?)?
        }
        CastSubcommand::ParseUnits { value, unit } => {
            let value = stdin::unwrap_line(value)?;
            sh_println!("{}", SimpleCast::parse_units(&value, unit)?)?;
        }
        CastSubcommand::FormatUnits { value, unit } => {
            let value = stdin::unwrap_line(value)?;
            sh_println!("{}", SimpleCast::format_units(&value, unit)?)?;
        }
        CastSubcommand::FromWei { value, unit } => {
            let value = stdin::unwrap_line(value)?;
            sh_println!("{}", SimpleCast::from_wei(&value, &unit)?)?
        }
        CastSubcommand::ToWei { value, unit } => {
            let value = stdin::unwrap_line(value)?;
            sh_println!("{}", SimpleCast::to_wei(&value, &unit)?)?
        }
        CastSubcommand::FromRlp { value, as_int } => {
            let value = stdin::unwrap_line(value)?;
            sh_println!("{}", SimpleCast::from_rlp(value, as_int)?)?
        }
        CastSubcommand::ToRlp { value } => {
            let value = stdin::unwrap_line(value)?;
            sh_println!("{}", SimpleCast::to_rlp(&value)?)?
        }
        CastSubcommand::ToHex(ToBaseArgs { value, base_in }) => {
            let value = stdin::unwrap_line(value)?;
            sh_println!("{}", SimpleCast::to_base(&value, base_in.as_deref(), "hex")?)?
        }
        CastSubcommand::ToDec(ToBaseArgs { value, base_in }) => {
            let value = stdin::unwrap_line(value)?;
            sh_println!("{}", SimpleCast::to_base(&value, base_in.as_deref(), "dec")?)?
        }
        CastSubcommand::ToBase { base: ToBaseArgs { value, base_in }, base_out } => {
            let (value, base_out) = stdin::unwrap2(value, base_out)?;
            sh_println!("{}", SimpleCast::to_base(&value, base_in.as_deref(), &base_out)?)?
        }
        CastSubcommand::ToBytes32 { bytes } => {
            let value = stdin::unwrap_line(bytes)?;
            sh_println!("{}", SimpleCast::to_bytes32(&value)?)?
        }
        CastSubcommand::Pad { data, right, left: _, len } => {
            let value = stdin::unwrap_line(data)?;
            sh_println!("{}", SimpleCast::pad(&value, right, len)?)?
        }
        CastSubcommand::FormatBytes32String { string } => {
            let value = stdin::unwrap_line(string)?;
            sh_println!("{}", SimpleCast::format_bytes32_string(&value)?)?
        }
        CastSubcommand::ParseBytes32String { bytes } => {
            let value = stdin::unwrap_line(bytes)?;
            sh_println!("{}", SimpleCast::parse_bytes32_string(&value)?)?
        }
        CastSubcommand::ParseBytes32Address { bytes } => {
            let value = stdin::unwrap_line(bytes)?;
            sh_println!("{}", SimpleCast::parse_bytes32_address(&value)?)?
        }

        // ABI encoding & decoding
        CastSubcommand::DecodeAbi { sig, calldata, input } => {
            let tokens = SimpleCast::abi_decode(&sig, &calldata, input)?;
            print_tokens(&tokens);
        }
        CastSubcommand::AbiEncode { sig, packed, args } => {
            if !packed {
                sh_println!("{}", SimpleCast::abi_encode(&sig, &args)?)?
            } else {
                sh_println!("{}", SimpleCast::abi_encode_packed(&sig, &args)?)?
            }
        }
        CastSubcommand::DecodeCalldata { sig, calldata } => {
            let tokens = SimpleCast::calldata_decode(&sig, &calldata, true)?;
            print_tokens(&tokens);
        }
        CastSubcommand::CalldataEncode { sig, args, file } => {
            let final_args = if let Some(file_path) = file {
                let contents = fs::read_to_string(file_path)?;
                contents
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(String::from)
                    .collect()
            } else {
                args
            };
            sh_println!("{}", SimpleCast::calldata_encode(sig, &final_args)?)?;
        }
        CastSubcommand::DecodeString { data } => {
            let tokens = SimpleCast::calldata_decode("Any(string)", &data, true)?;
            print_tokens(&tokens);
        }
        CastSubcommand::DecodeEvent { sig, data } => {
            let decoded_event = if let Some(event_sig) = sig {
                let event = get_event(event_sig.as_str())?;
                event.decode_log_parts(core::iter::once(event.selector()), &hex::decode(data)?)?
            } else {
                let data = data.strip_prefix("0x").unwrap_or(data.as_str());
                let selector = data.get(..64).unwrap_or_default();
                let selector = selector.parse()?;
                let identified_event =
                    SignaturesIdentifier::new(false)?.identify_event(selector).await;
                if let Some(event) = identified_event {
                    let _ = sh_println!("{}", event.signature());
                    let data = data.get(64..).unwrap_or_default();
                    get_event(event.signature().as_str())?
                        .decode_log_parts(core::iter::once(selector), &hex::decode(data)?)?
                } else {
                    eyre::bail!("No matching event signature found for selector `{selector}`")
                }
            };
            print_tokens(&decoded_event.body);
        }
        CastSubcommand::DecodeError { sig, data } => {
            let error = if let Some(err_sig) = sig {
                get_error(err_sig.as_str())?
            } else {
                let data = data.strip_prefix("0x").unwrap_or(data.as_str());
                let selector = data.get(..8).unwrap_or_default();
                let identified_error =
                    SignaturesIdentifier::new(false)?.identify_error(selector.parse()?).await;
                if let Some(error) = identified_error {
                    let _ = sh_println!("{}", error.signature());
                    error
                } else {
                    eyre::bail!("No matching error signature found for selector `{selector}`")
                }
            };
            let decoded_error = error.decode_error(&hex::decode(data)?)?;
            print_tokens(&decoded_error.body);
        }
        CastSubcommand::Interface(cmd) => cmd.run().await?,
        CastSubcommand::CreationCode(cmd) => cmd.run().await?,
        CastSubcommand::ConstructorArgs(cmd) => cmd.run().await?,
        CastSubcommand::Artifact(cmd) => cmd.run().await?,
        CastSubcommand::Bind(cmd) => cmd.run().await?,
        CastSubcommand::PrettyCalldata { calldata, offline } => {
            let calldata = stdin::unwrap_line(calldata)?;
            sh_println!("{}", pretty_calldata(&calldata, offline).await?)?;
        }
        CastSubcommand::Sig { sig, optimize } => {
            let sig = stdin::unwrap_line(sig)?;
            match optimize {
                Some(opt) => {
                    sh_println!("Starting to optimize signature...")?;
                    let start_time = Instant::now();
                    let (selector, signature) = SimpleCast::get_selector(&sig, opt)?;
                    sh_println!("Successfully generated in {:?}", start_time.elapsed())?;
                    sh_println!("Selector: {selector}")?;
                    sh_println!("Optimized signature: {signature}")?;
                }
                None => sh_println!("{}", SimpleCast::get_selector(&sig, 0)?.0)?,
            }
        }

        // Blockchain & RPC queries
        CastSubcommand::AccessList(cmd) => cmd.run().await?,
        CastSubcommand::Age { block, rpc } => {
            let config = rpc.load_config()?;
            let provider = utils::get_provider(&config)?;
            sh_println!(
                "{} UTC",
                Cast::new(provider).age(block.unwrap_or(BlockId::Number(Latest))).await?
            )?
        }
        CastSubcommand::Balance { block, who, ether, rpc, erc20 } => {
            let config = rpc.load_config()?;
            let provider = utils::get_provider(&config)?;
            let account_addr = who.resolve(&provider).await?;

            match erc20 {
                Some(token) => {
                    let balance =
                        Cast::new(&provider).erc20_balance(token, account_addr, block).await?;
                    sh_println!("{}", format_uint_exp(balance))?
                }
                None => {
                    let value = Cast::new(&provider).balance(account_addr, block).await?;
                    if ether {
                        sh_println!("{}", SimpleCast::from_wei(&value.to_string(), "eth")?)?
                    } else {
                        sh_println!("{value}")?
                    }
                }
            }
        }
        CastSubcommand::BaseFee { block, rpc } => {
            let config = rpc.load_config()?;
            let provider = utils::get_provider(&config)?;
            sh_println!(
                "{}",
                Cast::new(provider).base_fee(block.unwrap_or(BlockId::Number(Latest))).await?
            )?
        }
        CastSubcommand::Block { block, full, field, raw, rpc } => {
            let config = rpc.load_config()?;
            let provider = utils::get_provider(&config)?;

            // Can use either --raw or specify raw as a field
            let raw = raw || field.as_ref().is_some_and(|f| f == "raw");

            sh_println!(
                "{}",
                Cast::new(provider)
                    .block(block.unwrap_or(BlockId::Number(Latest)), full, field, raw)
                    .await?
            )?
        }
        CastSubcommand::BlockNumber { rpc, block } => {
            let config = rpc.load_config()?;
            let provider = utils::get_provider(&config)?;
            let number = match block {
                Some(id) => {
                    provider
                        .get_block(id)
                        .await?
                        .ok_or_else(|| eyre::eyre!("block {id:?} not found"))?
                        .header
                        .number
                }
                None => Cast::new(provider).block_number().await?,
            };
            sh_println!("{number}")?
        }
        CastSubcommand::Chain { rpc } => {
            let config = rpc.load_config()?;
            let provider = utils::get_provider(&config)?;
            sh_println!("{}", Cast::new(provider).chain().await?)?
        }
        CastSubcommand::ChainId { rpc } => {
            let config = rpc.load_config()?;
            let provider = utils::get_provider(&config)?;
            sh_println!("{}", Cast::new(provider).chain_id().await?)?
        }
        CastSubcommand::Client { rpc } => {
            let config = rpc.load_config()?;
            let provider = utils::get_provider(&config)?;
            sh_println!("{}", provider.get_client_version().await?)?
        }
        CastSubcommand::Code { block, who, disassemble, rpc } => {
            let config = rpc.load_config()?;
            let provider = utils::get_provider(&config)?;
            let who = who.resolve(&provider).await?;
            sh_println!("{}", Cast::new(provider).code(who, block, disassemble).await?)?
        }
        CastSubcommand::Codesize { block, who, rpc } => {
            let config = rpc.load_config()?;
            let provider = utils::get_provider(&config)?;
            let who = who.resolve(&provider).await?;
            sh_println!("{}", Cast::new(provider).codesize(who, block).await?)?
        }
        CastSubcommand::ComputeAddress { address, nonce, salt, init_code, init_code_hash, rpc } => {
            let address = stdin::unwrap_line(address)?;
            let computed = {
                // For CREATE2, init_code_hash is needed to compute the address
                if let Some(init_code_hash) = init_code_hash {
                    address.create2(salt.unwrap_or(B256::ZERO), init_code_hash)
                } else if let Some(init_code) = init_code {
                    address.create2(salt.unwrap_or(B256::ZERO), keccak256(hex::decode(init_code)?))
                } else {
                    // For CREATE, rpc is needed to compute the address
                    let config = rpc.load_config()?;
                    let provider = utils::get_provider(&config)?;
                    Cast::new(provider).compute_address(address, nonce).await?
                }
            };
            sh_println!("Computed Address: {}", computed.to_checksum(None))?
        }
        CastSubcommand::Disassemble { bytecode } => {
            let bytecode = stdin::unwrap_line(bytecode)?;
            sh_println!("{}", SimpleCast::disassemble(&hex::decode(bytecode)?)?)?
        }
        CastSubcommand::Selectors { bytecode, resolve } => {
            let bytecode = stdin::unwrap_line(bytecode)?;
            let functions = SimpleCast::extract_functions(&bytecode)?;
            let max_args_len = functions.iter().map(|r| r.1.len()).max().unwrap_or(0);
            let max_mutability_len = functions.iter().map(|r| r.2.len()).max().unwrap_or(0);

            let resolve_results = if resolve {
                let selectors = functions
                    .iter()
                    .map(|&(selector, ..)| SelectorKind::Function(selector))
                    .collect::<Vec<_>>();
                let ds = decode_selectors(&selectors).await?;
                ds.into_iter().map(|v| v.join("|")).collect()
            } else {
                vec![]
            };
            for (pos, (selector, arguments, state_mutability)) in functions.into_iter().enumerate()
            {
                if resolve {
                    let resolved = &resolve_results[pos];
                    sh_println!(
                        "{selector}\t{arguments:max_args_len$}\t{state_mutability:max_mutability_len$}\t{resolved}"
                    )?
                } else {
                    sh_println!("{selector}\t{arguments:max_args_len$}\t{state_mutability}")?
                }
            }
        }
        CastSubcommand::FindBlock(cmd) => cmd.run().await?,
        CastSubcommand::GasPrice { rpc } => {
            let config = rpc.load_config()?;
            let provider = utils::get_provider(&config)?;
            sh_println!("{}", Cast::new(provider).gas_price().await?)?;
        }
        CastSubcommand::Index { key_type, key, slot_number } => {
            sh_println!("{}", SimpleCast::index(&key_type, &key, &slot_number)?)?;
        }
        CastSubcommand::IndexErc7201 { id, formula_id } => {
            eyre::ensure!(formula_id == "erc7201", "unsupported formula ID: {formula_id}");
            let id = stdin::unwrap_line(id)?;
            sh_println!("{}", foundry_common::erc7201(&id))?;
        }
        CastSubcommand::Implementation { block, beacon, who, rpc } => {
            let config = rpc.load_config()?;
            let provider = utils::get_provider(&config)?;
            let who = who.resolve(&provider).await?;
            sh_println!("{}", Cast::new(provider).implementation(who, beacon, block).await?)?;
        }
        CastSubcommand::Admin { block, who, rpc } => {
            let config = rpc.load_config()?;
            let provider = utils::get_provider(&config)?;
            let who = who.resolve(&provider).await?;
            sh_println!("{}", Cast::new(provider).admin(who, block).await?)?;
        }
        CastSubcommand::Nonce { block, who, rpc } => {
            let config = rpc.load_config()?;
            let provider = utils::get_provider(&config)?;
            let who = who.resolve(&provider).await?;
            sh_println!("{}", Cast::new(provider).nonce(who, block).await?)?;
        }
        CastSubcommand::Codehash { block, who, slots, rpc } => {
            let config = rpc.load_config()?;
            let provider = utils::get_provider(&config)?;
            let who = who.resolve(&provider).await?;
            sh_println!("{}", Cast::new(provider).codehash(who, slots, block).await?)?;
        }
        CastSubcommand::StorageRoot { block, who, slots, rpc } => {
            let config = rpc.load_config()?;
            let provider = utils::get_provider(&config)?;
            let who = who.resolve(&provider).await?;
            sh_println!("{}", Cast::new(provider).storage_root(who, slots, block).await?)?;
        }
        CastSubcommand::Proof { address, slots, rpc, block } => {
            let config = rpc.load_config()?;
            let provider = utils::get_provider(&config)?;
            let address = address.resolve(&provider).await?;
            let value = provider
                .get_proof(address, slots.into_iter().collect())
                .block_id(block.unwrap_or_default())
                .await?;
            sh_println!("{}", serde_json::to_string(&value)?)?;
        }
        CastSubcommand::Rpc(cmd) => cmd.run().await?,
        CastSubcommand::Storage(cmd) => cmd.run().await?,

        // Calls & transactions
        CastSubcommand::Call(cmd) => cmd.run().await?,
        CastSubcommand::Estimate(cmd) => cmd.run().await?,
        CastSubcommand::MakeTx(cmd) => cmd.run().await?,
        CastSubcommand::PublishTx { raw_tx, cast_async, rpc } => {
            let config = rpc.load_config()?;
            let provider = utils::get_provider(&config)?;
            let cast = Cast::new(&provider);
            let pending_tx = cast.publish(raw_tx).await?;
            let tx_hash = pending_tx.inner().tx_hash();

            if cast_async {
                sh_println!("{tx_hash:#x}")?;
            } else {
                let receipt = pending_tx.get_receipt().await?;
                sh_println!("{}", serde_json::json!(receipt))?;
            }
        }
        CastSubcommand::Receipt { tx_hash, field, cast_async, confirmations, rpc } => {
            let config = rpc.load_config()?;
            let provider = utils::get_provider(&config)?;
            sh_println!(
                "{}",
                Cast::new(provider)
                    .receipt(tx_hash, field, confirmations, None, cast_async)
                    .await?
            )?
        }
        CastSubcommand::Run(cmd) => cmd.run().await?,
        CastSubcommand::SendTx(cmd) => cmd.run().await?,
        CastSubcommand::Tx { tx_hash, from, nonce, field, raw, rpc, to_request } => {
            let config = rpc.load_config()?;
            let provider = utils::get_provider(&config)?;

            // Can use either --raw or specify raw as a field
            let raw = raw || field.as_ref().is_some_and(|f| f == "raw");

            sh_println!(
                "{}",
                Cast::new(&provider)
                    .transaction(tx_hash, from, nonce, field, raw, to_request)
                    .await?
            )?
        }

        // 4Byte
        CastSubcommand::FourByte { selector } => {
            let selector = stdin::unwrap_line(selector)?;
            let sigs = decode_function_selector(selector).await?;
            if sigs.is_empty() {
                eyre::bail!("No matching function signatures found for selector `{selector}`");
            }
            for sig in sigs {
                sh_println!("{sig}")?
            }
        }

        CastSubcommand::FourByteCalldata { calldata } => {
            let calldata = stdin::unwrap_line(calldata)?;

            if calldata.len() == 10 {
                let sigs = decode_function_selector(calldata.parse()?).await?;
                if sigs.is_empty() {
                    eyre::bail!("No matching function signatures found for calldata `{calldata}`");
                }
                for sig in sigs {
                    sh_println!("{sig}")?
                }
                return Ok(());
            }

            let sigs = decode_calldata(&calldata).await?;
            sigs.iter().enumerate().for_each(|(i, sig)| {
                let _ = sh_println!("{}) \"{sig}\"", i + 1);
            });

            let sig = match sigs.len() {
                0 => eyre::bail!("No signatures found"),
                1 => sigs.first().unwrap(),
                _ => {
                    let i: usize = prompt!("Select a function signature by number: ")?;
                    sigs.get(i - 1).ok_or_else(|| eyre::eyre!("Invalid signature index"))?
                }
            };

            let tokens = SimpleCast::calldata_decode(sig, &calldata, true)?;
            print_tokens(&tokens);
        }

        CastSubcommand::FourByteEvent { topic } => {
            let topic = stdin::unwrap_line(topic)?;
            let sigs = decode_event_topic(topic).await?;
            if sigs.is_empty() {
                eyre::bail!("No matching event signatures found for topic `{topic}`");
            }
            for sig in sigs {
                sh_println!("{sig}")?
            }
        }
        CastSubcommand::UploadSignature { signatures } => {
            let signatures = stdin::unwrap_vec(signatures)?;
            let ParsedSignatures { signatures, abis } = parse_signatures(signatures);
            if !abis.is_empty() {
                import_selectors(SelectorImportData::Abi(abis)).await?.describe();
            }
            if !signatures.is_empty() {
                import_selectors(SelectorImportData::Raw(signatures)).await?.describe();
            }
        }

        // ENS
        CastSubcommand::Namehash { name } => {
            let name = stdin::unwrap_line(name)?;
            sh_println!("{}", namehash(&name))?
        }
        CastSubcommand::LookupAddress { who, rpc, verify } => {
            let config = rpc.load_config()?;
            let provider = utils::get_provider(&config)?;

            let who = stdin::unwrap_line(who)?;
            let name = provider.lookup_address(&who).await?;
            if verify {
                let address = provider.resolve_name(&name).await?;
                eyre::ensure!(
                    address == who,
                    "Reverse lookup verification failed: got `{address}`, expected `{who}`"
                );
            }
            sh_println!("{name}")?
        }
        CastSubcommand::ResolveName { who, rpc, verify } => {
            let config = rpc.load_config()?;
            let provider = utils::get_provider(&config)?;

            let who = stdin::unwrap_line(who)?;
            let address = provider.resolve_name(&who).await?;
            if verify {
                let name = provider.lookup_address(&address).await?;
                eyre::ensure!(
                    name == who,
                    "Forward lookup verification failed: got `{name}`, expected `{who}`"
                );
            }
            sh_println!("{address}")?
        }

        // Misc
        CastSubcommand::Keccak { data } => {
            let bytes = match data {
                Some(data) => data.into_bytes(),
                None => stdin::read_bytes(false)?,
            };
            match String::from_utf8(bytes) {
                Ok(s) => {
                    let s = SimpleCast::keccak(&s)?;
                    sh_println!("{s}")?
                }
                Err(e) => {
                    let hash = keccak256(e.as_bytes());
                    let s = hex::encode(hash);
                    sh_println!("0x{s}")?
                }
            };
        }
        CastSubcommand::HashMessage { message } => {
            let message = stdin::unwrap(message, false)?;
            sh_println!("{}", eip191_hash_message(message))?
        }
        CastSubcommand::SigEvent { event_string } => {
            let event_string = stdin::unwrap_line(event_string)?;
            let parsed_event = get_event(&event_string)?;
            sh_println!("{:?}", parsed_event.selector())?
        }
        CastSubcommand::LeftShift { value, bits, base_in, base_out } => sh_println!(
            "{}",
            SimpleCast::left_shift(&value, &bits, base_in.as_deref(), &base_out)?
        )?,
        CastSubcommand::RightShift { value, bits, base_in, base_out } => sh_println!(
            "{}",
            SimpleCast::right_shift(&value, &bits, base_in.as_deref(), &base_out)?
        )?,
        CastSubcommand::Source {
            address,
            directory,
            explorer_api_url,
            explorer_url,
            etherscan,
            flatten,
        } => {
            let config = etherscan.load_config()?;
            let chain = config.chain.unwrap_or_default();
            let api_key = config.get_etherscan_api_key(Some(chain));
            match (directory, flatten) {
                (Some(dir), false) => {
                    SimpleCast::expand_etherscan_source_to_directory(
                        chain,
                        address,
                        api_key,
                        dir,
                        explorer_api_url,
                        explorer_url,
                    )
                    .await?
                }
                (None, false) => sh_println!(
                    "{}",
                    SimpleCast::etherscan_source(
                        chain,
                        address,
                        api_key,
                        explorer_api_url,
                        explorer_url
                    )
                    .await?
                )?,
                (dir, true) => {
                    SimpleCast::etherscan_source_flatten(
                        chain,
                        address,
                        api_key,
                        dir,
                        explorer_api_url,
                        explorer_url,
                    )
                    .await?;
                }
            }
        }
        CastSubcommand::Create2(cmd) => {
            cmd.run()?;
        }
        CastSubcommand::Wallet { command } => command.run().await?,
        CastSubcommand::Completions { shell } => {
            generate(shell, &mut CastArgs::command(), "cast", &mut std::io::stdout())
        }
        CastSubcommand::GenerateFigSpec => clap_complete::generate(
            clap_complete_fig::Fig,
            &mut CastArgs::command(),
            "cast",
            &mut std::io::stdout(),
        ),
        CastSubcommand::Logs(cmd) => cmd.run().await?,
        CastSubcommand::DecodeTransaction { tx } => {
            let tx = stdin::unwrap_line(tx)?;
            let tx = SimpleCast::decode_raw_transaction(&tx)?;

            if let Ok(signer) = tx.recover_signer() {
                let recovered = Recovered::new_unchecked(tx, signer);
                sh_println!("{}", serde_json::to_string_pretty(&recovered)?)?;
            } else {
                sh_println!("{}", serde_json::to_string_pretty(&tx)?)?;
            }
        }
        CastSubcommand::RecoverAuthority { auth } => {
            let auth: SignedAuthorization = serde_json::from_str(&auth).unwrap();
            sh_println!("{}", auth.recover_authority()?)?;
        }
        CastSubcommand::TxPool { command } => command.run().await?,
        CastSubcommand::DAEstimate(cmd) => {
            cmd.run().await?;
        }
    };

    /// Prints slice of tokens using [`format_tokens`] or [`format_tokens_raw`] depending whether
    /// the shell is in JSON mode.
    ///
    /// This is included here to avoid a cyclic dependency between `fmt` and `common`.
    fn print_tokens(tokens: &[DynSolValue]) {
        if shell::is_json() {
            let tokens: Vec<String> = format_tokens_raw(tokens).collect();
            let _ = sh_println!("{}", serde_json::to_string_pretty(&tokens).unwrap());
        } else {
            let tokens = format_tokens(tokens);
            tokens.for_each(|t| {
                let _ = sh_println!("{t}");
            });
        }
    }

    Ok(())
}
