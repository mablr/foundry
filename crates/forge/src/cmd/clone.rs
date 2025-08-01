use super::{init::InitArgs, install::DependencyInstallOpts};
use alloy_primitives::{Address, Bytes, ChainId, TxHash};
use clap::{Parser, ValueHint};
use eyre::Result;
use foundry_block_explorers::{
    Client,
    contract::{ContractCreationData, ContractMetadata, Metadata},
    errors::EtherscanError,
};
use foundry_cli::{
    opts::EtherscanOpts,
    utils::{Git, LoadConfig},
};
use foundry_common::{compile::ProjectCompiler, fs};
use foundry_compilers::{
    ProjectCompileOutput, ProjectPathsConfig,
    artifacts::{
        ConfigurableContractArtifact, Settings, StorageLayout,
        output_selection::ContractOutputSelection,
        remappings::{RelativeRemapping, Remapping},
    },
    compilers::solc::Solc,
};
use foundry_config::{Chain, Config};
use std::{
    fs::read_dir,
    path::{Path, PathBuf},
    time::Duration,
};

/// CloneMetadata stores the metadata that are not included by `foundry.toml` but necessary for a
/// cloned contract. The metadata can be serialized to a metadata file in the cloned project root.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneMetadata {
    /// The path to the source file that contains the contract declaration.
    /// The path is relative to the root directory of the project.
    pub path: PathBuf,
    /// The name of the contract in the file.
    pub target_contract: String,
    /// The address of the contract on the blockchain.
    pub address: Address,
    /// The chain id.
    pub chain_id: ChainId,
    /// The transaction hash of the creation transaction.
    pub creation_transaction: TxHash,
    /// The address of the deployer, i.e., sender of the creation transaction.
    pub deployer: Address,
    /// The constructor arguments of the contract on chain.
    pub constructor_arguments: Bytes,
    /// The storage layout of the contract on chain.
    pub storage_layout: StorageLayout,
}

/// CLI arguments for `forge clone`.
///
/// `forge clone` clones an on-chain contract from block explorers (e.g., Etherscan) in the
/// following steps:
/// 1. Fetch the contract source code from the block explorer.
/// 2. Initialize a empty foundry project at the `root` directory specified in `CloneArgs`.
/// 3. Dump the contract sources to the source directory.
/// 4. Update the `foundry.toml` configuration file with the compiler settings from Etherscan.
/// 5. Try compile the cloned contract, so that we can get the original storage layout. This
///    original storage layout is preserved in the `CloneMetadata` so that if the user later
///    modifies the contract, it is possible to quickly check the storage layout compatibility with
///    the original on-chain contract.
/// 6. Dump the `CloneMetadata` to the root directory of the cloned project as `.clone.meta` file.
#[derive(Clone, Debug, Parser)]
pub struct CloneArgs {
    /// The contract address to clone.
    pub address: Address,

    /// The root directory of the cloned project.
    #[arg(value_hint = ValueHint::DirPath, default_value = ".", value_name = "PATH")]
    pub root: PathBuf,

    /// Do not generate the remappings.txt file. Instead, keep the remappings in the configuration.
    #[arg(long)]
    pub no_remappings_txt: bool,

    /// Keep the original directory structure collected from Etherscan.
    ///
    /// If this flag is set, the directory structure of the cloned project will be kept as is.
    /// By default, the directory structure is re-orgnized to increase the readability, but may
    /// risk some compilation failures.
    #[arg(long)]
    pub keep_directory_structure: bool,

    #[command(flatten)]
    pub etherscan: EtherscanOpts,

    #[command(flatten)]
    pub install: DependencyInstallOpts,
}

impl CloneArgs {
    pub async fn run(self) -> Result<()> {
        let Self { address, root, install, etherscan, no_remappings_txt, keep_directory_structure } =
            self;

        // step 0. get the chain and api key from the config
        let config = etherscan.load_config()?;
        let chain = config.chain.unwrap_or_default();
        let etherscan_api_version = config.get_etherscan_api_version(Some(chain));
        let etherscan_api_key = config.get_etherscan_api_key(Some(chain)).unwrap_or_default();
        let client =
            Client::new_with_api_version(chain, etherscan_api_key.clone(), etherscan_api_version)?;

        // step 1. get the metadata from client
        sh_println!("Downloading the source code of {address} from Etherscan...")?;

        let meta = Self::collect_metadata_from_client(address, &client).await?;

        // step 2. initialize an empty project
        Self::init_an_empty_project(&root, install)?;
        // canonicalize the root path
        // note that at this point, the root directory must have been created
        let root = dunce::canonicalize(&root)?;

        // step 3. parse the metadata
        Self::parse_metadata(&meta, chain, &root, no_remappings_txt, keep_directory_structure)
            .await?;

        // step 4. collect the compilation metadata
        // if the etherscan api key is not set, we need to wait for 3 seconds between calls
        sh_println!("Collecting the creation information of {address} from Etherscan...")?;

        if etherscan_api_key.is_empty() {
            sh_warn!("Waiting for 5 seconds to avoid rate limit...")?;
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
        Self::collect_compilation_metadata(&meta, chain, address, &root, &client).await?;

        // step 5. git add and commit the changes if needed
        if install.commit {
            let git = Git::new(&root);
            git.add(Some("--all"))?;
            let msg = format!("chore: forge clone {address}");
            git.commit(&msg)?;
        }

        Ok(())
    }

    /// Collect the metadata of the contract from the block explorer.
    ///
    /// * `address` - the address of the contract to be cloned.
    /// * `client` - the client of the block explorer.
    pub(crate) async fn collect_metadata_from_client<C: EtherscanClient>(
        address: Address,
        client: &C,
    ) -> Result<Metadata> {
        let mut meta = client.contract_source_code(address).await?;
        eyre::ensure!(meta.items.len() == 1, "contract not found or ill-formed");
        let meta = meta.items.remove(0);
        eyre::ensure!(!meta.is_vyper(), "Vyper contracts are not supported");
        Ok(meta)
    }

    /// Initialize an empty project at the root directory.
    ///
    /// * `root` - the root directory of the project.
    /// * `enable_git` - whether to enable git for the project.
    /// * `quiet` - whether to print messages.
    pub(crate) fn init_an_empty_project(root: &Path, install: DependencyInstallOpts) -> Result<()> {
        // let's try to init the project with default init args
        let init_args = InitArgs { root: root.to_path_buf(), install, ..Default::default() };
        init_args.run().map_err(|e| eyre::eyre!("Project init error: {:?}", e))?;

        // remove the unnecessary example contracts
        // XXX (ZZ): this is a temporary solution until we have a proper way to remove contracts,
        // e.g., add a field in the InitArgs to control the example contract generation
        fs::remove_file(root.join("src/Counter.sol"))?;
        fs::remove_file(root.join("test/Counter.t.sol"))?;
        fs::remove_file(root.join("script/Counter.s.sol"))?;

        Ok(())
    }

    /// Collect the compilation metadata of the cloned contract.
    /// This function compiles the cloned contract and collects the compilation metadata.
    ///
    /// * `meta` - the metadata of the contract (from Etherscan).
    /// * `chain` - the chain where the contract to be cloned locates.
    /// * `address` - the address of the contract to be cloned.
    /// * `root` - the root directory of the cloned project.
    /// * `client` - the client of the block explorer.
    pub(crate) async fn collect_compilation_metadata<C: EtherscanClient>(
        meta: &Metadata,
        chain: Chain,
        address: Address,
        root: &PathBuf,
        client: &C,
    ) -> Result<()> {
        // compile the cloned contract
        let compile_output = compile_project(root)?;
        let (main_file, main_artifact) = find_main_contract(&compile_output, &meta.contract_name)?;
        let main_file = main_file.strip_prefix(root)?.to_path_buf();
        let storage_layout =
            main_artifact.storage_layout.to_owned().expect("storage layout not found");

        // dump the metadata to the root directory
        let creation_tx = client.contract_creation_data(address).await?;
        let clone_meta = CloneMetadata {
            path: main_file,
            target_contract: meta.contract_name.clone(),
            address,
            chain_id: chain.id(),
            creation_transaction: creation_tx.transaction_hash,
            deployer: creation_tx.contract_creator,
            constructor_arguments: meta.constructor_arguments.clone(),
            storage_layout,
        };
        let metadata_content = serde_json::to_string(&clone_meta)?;
        let metadata_file = root.join(".clone.meta");
        fs::write(&metadata_file, metadata_content)?;
        let mut perms = std::fs::metadata(&metadata_file)?.permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&metadata_file, perms)?;

        Ok(())
    }

    /// Download and parse the source code from Etherscan.
    ///
    /// * `chain` - the chain where the contract to be cloned locates.
    /// * `address` - the address of the contract to be cloned.
    /// * `root` - the root directory to clone the contract into as a foundry project.
    /// * `client` - the client of the block explorer.
    /// * `no_remappings_txt` - whether to generate the remappings.txt file.
    pub(crate) async fn parse_metadata(
        meta: &Metadata,
        chain: Chain,
        root: &PathBuf,
        no_remappings_txt: bool,
        keep_directory_structure: bool,
    ) -> Result<()> {
        // dump sources and update the remapping in configuration
        let remappings = dump_sources(meta, root, keep_directory_structure)?;
        Config::update_at(root, |config, doc| {
            let profile = config.profile.as_str().as_str();

            // update the remappings in the configuration
            let mut remapping_array = toml_edit::Array::new();
            for r in remappings {
                remapping_array.push(r.to_string());
            }
            doc[Config::PROFILE_SECTION][profile]["remappings"] = toml_edit::value(remapping_array);

            // make sure auto_detect_remappings is false (it is very important because cloned
            // project may not follow the common remappings)
            doc[Config::PROFILE_SECTION][profile]["auto_detect_remappings"] =
                toml_edit::value(false);
            true
        })?;

        // update configuration
        Config::update_at(root, |config, doc| {
            update_config_by_metadata(config, doc, meta, chain).is_ok()
        })?;

        // write remappings to remappings.txt if necessary
        if !no_remappings_txt {
            let remappings_txt = root.join("remappings.txt");
            eyre::ensure!(
                !remappings_txt.exists(),
                "remappings.txt already exists, please remove it first"
            );

            Config::update_at(root, |config, doc| {
                let remappings_txt_content =
                    config.remappings.iter().map(|r| r.to_string()).collect::<Vec<_>>().join("\n");
                if fs::write(&remappings_txt, remappings_txt_content).is_err() {
                    return false;
                }

                let profile = config.profile.as_str().as_str();
                if let Some(elem) = doc[Config::PROFILE_SECTION][profile].as_table_mut() {
                    elem.remove_entry("remappings");
                    true
                } else {
                    false
                }
            })?;
        }

        Ok(())
    }
}

/// Update the configuration file with the metadata.
/// This function will update the configuration file with the metadata from the contract.
/// It will update the following fields:
/// - `auto_detect_solc` to `false`
/// - `solc_version` to the value from the metadata
/// - `evm_version` to the value from the metadata, if the metadata's evm_version is "Default", then
///   this is derived from the solc version this contract was compiled with.
/// - `via_ir` to the value from the metadata
/// - `libraries` to the value from the metadata
/// - `metadata` to the value from the metadata
///     - `cbor_metadata`, `use_literal_content`, and `bytecode_hash`
/// - `optimizer` to the value from the metadata
/// - `optimizer_runs` to the value from the metadata
/// - `optimizer_details` to the value from the metadata
///     - `yul_details`, `yul`, etc.
///     - `simpleCounterForLoopUncheckedIncrement` is ignored for now
/// - `remappings` and `stop_after` are pre-validated to be empty and None, respectively
/// - `model_checker`, `debug`, and `output_selection` are ignored for now
///
/// Detailed information can be found from the following link:
/// - <https://github.com/foundry-rs/foundry/blob/master/crates/config/README.md#all-options>
/// - <https://docs.soliditylang.org/en/latest/using-the-compiler.html#compiler-input-and-output-json-description>
fn update_config_by_metadata(
    config: &Config,
    doc: &mut toml_edit::DocumentMut,
    meta: &Metadata,
    chain: Chain,
) -> Result<()> {
    let profile = config.profile.as_str().as_str();

    // macro to update the config if the value exists
    macro_rules! update_if_needed {
        ([$($key:expr),+], $value:expr) => {
            {
                if let Some(value) = $value {
                    let mut current = &mut doc[Config::PROFILE_SECTION][profile];
                    $(
                        if let Some(nested_doc) = current.get_mut(&$key) {
                            current = nested_doc;
                        } else {
                            return Err(eyre::eyre!("cannot find the key: {}", $key));
                        }
                    )+
                    *current = toml_edit::value(value);
                }
            }
        };
    }

    // update the chain id
    doc[Config::PROFILE_SECTION][profile]["chain_id"] = toml_edit::value(chain.id() as i64);

    // disable auto detect solc and set the solc version
    doc[Config::PROFILE_SECTION][profile]["auto_detect_solc"] = toml_edit::value(false);
    let version = meta.compiler_version()?;
    doc[Config::PROFILE_SECTION][profile]["solc_version"] =
        toml_edit::value(format!("{}.{}.{}", version.major, version.minor, version.patch));

    // get optimizer settings
    // we ignore `model_checker`, `debug`, and `output_selection` for now,
    // it seems they do not have impacts on the actual compilation
    let Settings { optimizer, libraries, evm_version, via_ir, stop_after, metadata, .. } =
        meta.settings()?;
    eyre::ensure!(stop_after.is_none(), "stop_after should be None");

    update_if_needed!(["evm_version"], evm_version.map(|v| v.to_string()));
    update_if_needed!(["via_ir"], via_ir);

    // update metadata if needed
    if let Some(metadata) = metadata {
        update_if_needed!(["cbor_metadata"], metadata.cbor_metadata);
        update_if_needed!(["use_literal_content"], metadata.use_literal_content);
        update_if_needed!(["bytecode_hash"], metadata.bytecode_hash.map(|v| v.to_string()));
    }

    // update optimizer settings if needed
    update_if_needed!(["optimizer"], optimizer.enabled);
    update_if_needed!(["optimizer_runs"], optimizer.runs.map(|v| v as i64));
    // update optimizer details if needed
    if let Some(detail) = optimizer.details {
        doc[Config::PROFILE_SECTION][profile]["optimizer_details"] = toml_edit::table();

        update_if_needed!(["optimizer_details", "peephole"], detail.peephole);
        update_if_needed!(["optimizer_details", "inliner"], detail.inliner);
        update_if_needed!(["optimizer_details", "jumpdestRemover"], detail.jumpdest_remover);
        update_if_needed!(["optimizer_details", "orderLiterals"], detail.order_literals);
        update_if_needed!(["optimizer_details", "deduplicate"], detail.deduplicate);
        update_if_needed!(["optimizer_details", "cse"], detail.cse);
        update_if_needed!(["optimizer_details", "constantOptimizer"], detail.constant_optimizer);
        update_if_needed!(
            ["optimizer_details", "simpleCounterForLoopUncheckedIncrement"],
            detail.simple_counter_for_loop_unchecked_increment
        );
        update_if_needed!(["optimizer_details", "yul"], detail.yul);

        if let Some(yul_detail) = detail.yul_details {
            doc[Config::PROFILE_SECTION][profile]["optimizer_details"]["yulDetails"] =
                toml_edit::table();
            update_if_needed!(
                ["optimizer_details", "yulDetails", "stackAllocation"],
                yul_detail.stack_allocation
            );
            update_if_needed!(
                ["optimizer_details", "yulDetails", "optimizerSteps"],
                yul_detail.optimizer_steps
            );
        }
    }

    // apply remapping on libraries
    let path_config: ProjectPathsConfig = config.project_paths();
    let libraries = libraries
        .apply(|libs| path_config.apply_lib_remappings(libs))
        .with_stripped_file_prefixes(&path_config.root);

    // update libraries
    let mut lib_array = toml_edit::Array::new();
    for (path_to_lib, info) in libraries.libs {
        for (lib_name, address) in info {
            lib_array.push(format!("{}:{}:{}", path_to_lib.to_str().unwrap(), lib_name, address));
        }
    }
    doc[Config::PROFILE_SECTION][profile]["libraries"] = toml_edit::value(lib_array);

    Ok(())
}

/// Dump the contract sources to the root directory.
/// The sources are dumped to the `src` directory.
/// IO errors may be returned.
/// A list of remappings is returned
fn dump_sources(meta: &Metadata, root: &PathBuf, no_reorg: bool) -> Result<Vec<RelativeRemapping>> {
    // get config
    let path_config = ProjectPathsConfig::builder().build_with_root::<Solc>(root);
    // we will canonicalize the sources directory later
    let src_dir = &path_config.sources;
    let lib_dir = &path_config.libraries[0];
    // Optional dir, if found in src
    let node_modules_dir = &root.join("node_modules");
    let contract_name = &meta.contract_name;
    let source_tree = meta.source_tree();

    // then we move the sources to the correct directories
    // we will first load existing remappings if necessary
    //  make sure this happens before dumping sources
    let mut remappings: Vec<Remapping> = Remapping::find_many(root);

    // first we dump the sources to a temporary directory
    let tmp_dump_dir = root.join("raw_sources");
    source_tree
        .write_to(&tmp_dump_dir)
        .map_err(|e| eyre::eyre!("failed to dump sources: {}", e))?;

    // check whether we need to re-organize directories in the original sources, since we do not
    // want to put all the sources in the `src` directory if the original directory structure is
    // well organized, e.g., a standard foundry project containing `src` and `lib`
    //
    // * if the user wants to keep the original directory structure, we should not re-organize.
    // * if there is any other directory other than `src`, `contracts`, `lib`, `hardhat`,
    //   `forge-std`,
    // or not started with `@`, we should not re-organize.
    let to_reorg = !no_reorg
        && std::fs::read_dir(tmp_dump_dir.join(contract_name))?.all(|e| {
            let Ok(e) = e else { return false };
            let folder_name = e.file_name();
            folder_name == "src"
                || folder_name == "lib"
                || folder_name == "node_modules"
                || folder_name == "contracts"
                || folder_name == "hardhat"
                || folder_name == "forge-std"
                || folder_name.to_string_lossy().starts_with('@')
        });

    // ensure `src` and `lib` directories exist
    eyre::ensure!(Path::exists(&root.join(src_dir)), "`src` directory must exists");
    eyre::ensure!(Path::exists(&root.join(lib_dir)), "`lib` directory must exists");

    // move source files
    for entry in std::fs::read_dir(tmp_dump_dir.join(contract_name))? {
        let entry = entry?;
        let folder_name = entry.file_name();
        // special handling when we need to re-organize the directories: we flatten them.
        if to_reorg {
            if folder_name == "contracts"
                || folder_name == "src"
                || folder_name == "lib"
                || folder_name == "node_modules"
            {
                // move all sub folders in contracts to src or lib
                let new_dir = if folder_name == "lib" {
                    lib_dir
                } else if folder_name == "node_modules" {
                    // Create node_modules dir if it exists in raw sources.
                    std::fs::create_dir(node_modules_dir)?;
                    node_modules_dir
                } else {
                    src_dir
                };
                for e in read_dir(entry.path())? {
                    let e = e?;
                    let dest = new_dir.join(e.file_name());
                    eyre::ensure!(!Path::exists(&dest), "destination already exists: {:?}", dest);
                    std::fs::rename(e.path(), &dest)?;
                    remappings.push(Remapping {
                        context: None,
                        name: format!(
                            "{}/{}",
                            folder_name.to_string_lossy(),
                            e.file_name().to_string_lossy()
                        ),
                        path: dest.to_string_lossy().to_string(),
                    });
                }
            } else {
                assert!(
                    folder_name == "hardhat"
                        || folder_name == "forge-std"
                        || folder_name.to_string_lossy().starts_with('@')
                );
                // move these other folders to lib
                let dest = lib_dir.join(&folder_name);
                if folder_name == "forge-std" {
                    // let's use the provided forge-std directory
                    std::fs::remove_dir_all(&dest)?;
                }
                eyre::ensure!(!Path::exists(&dest), "destination already exists: {:?}", dest);
                std::fs::rename(entry.path(), &dest)?;
                remappings.push(Remapping {
                    context: None,
                    name: folder_name.to_string_lossy().to_string(),
                    path: dest.to_string_lossy().to_string(),
                });
            }
        } else {
            // directly move the all folders into src
            let dest = src_dir.join(&folder_name);
            eyre::ensure!(!Path::exists(&dest), "destination already exists: {:?}", dest);
            std::fs::rename(entry.path(), &dest)?;
            if folder_name != "src" {
                remappings.push(Remapping {
                    context: None,
                    name: folder_name.to_string_lossy().to_string(),
                    path: dest.to_string_lossy().to_string(),
                });
            }
        }
    }

    // remove the temporary directory
    std::fs::remove_dir_all(tmp_dump_dir)?;

    // add remappings in the metadata
    for mut r in meta.settings()?.remappings {
        if to_reorg {
            // we should update its remapped path in the same way as we dump sources
            // i.e., remove prefix `contracts` (if any) and add prefix `src`
            let new_path = if r.path.starts_with("contracts") {
                PathBuf::from("src").join(PathBuf::from(&r.path).strip_prefix("contracts")?)
            } else if r.path.starts_with('@')
                || r.path.starts_with("hardhat/")
                || r.path.starts_with("forge-std/")
            {
                PathBuf::from("lib").join(PathBuf::from(&r.path))
            } else {
                PathBuf::from(&r.path)
            };
            r.path = new_path.to_string_lossy().to_string();
        }
        remappings.push(r);
    }

    Ok(remappings.into_iter().map(|r| r.into_relative(root)).collect())
}

/// Compile the project in the root directory, and return the compilation result.
pub fn compile_project(root: &Path) -> Result<ProjectCompileOutput> {
    let mut config = Config::load_with_root(root)?.sanitized();
    config.extra_output.push(ContractOutputSelection::StorageLayout);
    let project = config.project()?;
    let compiler = ProjectCompiler::new();
    compiler.compile(&project)
}

/// Find the artifact of the contract with the specified name.
/// This function returns the path to the source file and the artifact.
pub fn find_main_contract<'a>(
    compile_output: &'a ProjectCompileOutput,
    contract: &str,
) -> Result<(PathBuf, &'a ConfigurableContractArtifact)> {
    let mut rv = None;
    for (f, c, a) in compile_output.artifacts_with_files() {
        if contract == c {
            // it is possible that we have multiple contracts with the same name
            // in different files
            // instead of throwing an error, we should handle this case in the future
            if rv.is_some() {
                return Err(eyre::eyre!("multiple contracts with the same name found"));
            }
            rv = Some((PathBuf::from(f), a));
        }
    }
    rv.ok_or_else(|| eyre::eyre!("contract not found"))
}

/// EtherscanClient is a trait that defines the methods to interact with Etherscan.
/// It is defined as a wrapper of the `foundry_block_explorers::Client` to allow mocking.
#[cfg_attr(test, mockall::automock)]
pub(crate) trait EtherscanClient {
    async fn contract_source_code(
        &self,
        address: Address,
    ) -> std::result::Result<ContractMetadata, EtherscanError>;
    async fn contract_creation_data(
        &self,
        address: Address,
    ) -> std::result::Result<ContractCreationData, EtherscanError>;
}

impl EtherscanClient for Client {
    #[inline]
    async fn contract_source_code(
        &self,
        address: Address,
    ) -> std::result::Result<ContractMetadata, EtherscanError> {
        self.contract_source_code(address).await
    }

    #[inline]
    async fn contract_creation_data(
        &self,
        address: Address,
    ) -> std::result::Result<ContractCreationData, EtherscanError> {
        self.contract_creation_data(address).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::hex;
    use foundry_compilers::CompilerContract;
    use foundry_test_utils::rpc::next_etherscan_api_key;
    use std::collections::BTreeMap;

    #[expect(clippy::disallowed_macros)]
    fn assert_successful_compilation(root: &PathBuf) -> ProjectCompileOutput {
        println!("project_root: {root:#?}");
        compile_project(root).expect("compilation failure")
    }

    fn assert_compilation_result(
        compiled: ProjectCompileOutput,
        contract_name: &str,
        stripped_creation_code: &str,
    ) {
        compiled.compiled_contracts_by_compiler_version().iter().for_each(|(_, contracts)| {
            contracts.iter().for_each(|(name, contract)| {
                if name == contract_name {
                    let compiled_creation_code =
                        contract.bin_ref().expect("creation code not found");
                    assert!(
                        hex::encode(compiled_creation_code.as_ref())
                            .starts_with(stripped_creation_code),
                        "inconsistent creation code"
                    );
                }
            });
        });
    }

    fn mock_etherscan(address: Address) -> impl super::EtherscanClient {
        // load mock data
        let mut mocked_data = BTreeMap::new();
        let data_folder =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/etherscan");
        // iterate each sub folder
        for entry in std::fs::read_dir(data_folder).expect("failed to read test data folder") {
            let entry = entry.expect("failed to read test data entry");
            let addr: Address = entry.file_name().to_string_lossy().parse().unwrap();
            let contract_data_dir = entry.path();
            // the metadata.json file contains the metadata of the contract
            let metadata_file = contract_data_dir.join("metadata.json");
            let metadata: ContractMetadata =
                serde_json::from_str(&std::fs::read_to_string(metadata_file).unwrap())
                    .expect("failed to parse metadata.json");
            // the creation_data.json file contains the creation data of the contract
            let creation_data_file = contract_data_dir.join("creation_data.json");
            let creation_data: ContractCreationData =
                serde_json::from_str(&std::fs::read_to_string(creation_data_file).unwrap())
                    .expect("failed to parse creation_data.json");
            // insert the data to the map
            mocked_data.insert(addr, (metadata, creation_data));
        }

        let (metadata, creation_data) = mocked_data.get(&address).unwrap();
        let metadata = metadata.clone();
        let creation_data = *creation_data;
        let mut mocked_client = super::MockEtherscanClient::new();
        mocked_client
            .expect_contract_source_code()
            .times(1)
            .returning(move |_| Ok(metadata.clone()));
        mocked_client
            .expect_contract_creation_data()
            .times(1)
            .returning(move |_| Ok(creation_data));
        mocked_client
    }

    /// Fetch the metadata and creation data from Etherscan and dump them to the testdata folder.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "this test is used to dump mock data from Etherscan"]
    async fn test_dump_mock_data() {
        let address: Address = "0x9d27527Ada2CF29fBDAB2973cfa243845a08Bd3F".parse().unwrap();
        let data_folder = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/etherscan")
            .join(address.to_string());
        // create folder if not exists
        std::fs::create_dir_all(&data_folder).unwrap();
        // create metadata.json and creation_data.json
        let client = Client::new(Chain::mainnet(), next_etherscan_api_key()).unwrap();
        let meta = client.contract_source_code(address).await.unwrap();
        // dump json
        let json = serde_json::to_string_pretty(&meta).unwrap();
        // write to metadata.json
        std::fs::write(data_folder.join("metadata.json"), json).unwrap();
        let creation_data = client.contract_creation_data(address).await.unwrap();
        // dump json
        let json = serde_json::to_string_pretty(&creation_data).unwrap();
        // write to creation_data.json
        std::fs::write(data_folder.join("creation_data.json"), json).unwrap();
    }

    /// Run the clone command with the specified contract address and assert the compilation.
    async fn one_test_case(address: Address, check_compilation_result: bool) {
        let mut project_root = tempfile::tempdir().unwrap().path().to_path_buf();
        let client = mock_etherscan(address);
        let meta = CloneArgs::collect_metadata_from_client(address, &client).await.unwrap();
        CloneArgs::init_an_empty_project(&project_root, DependencyInstallOpts::default()).unwrap();
        project_root = dunce::canonicalize(&project_root).unwrap();
        CloneArgs::parse_metadata(&meta, Chain::mainnet(), &project_root, false, false)
            .await
            .unwrap();
        CloneArgs::collect_compilation_metadata(
            &meta,
            Chain::mainnet(),
            address,
            &project_root,
            &client,
        )
        .await
        .unwrap();
        let rv = assert_successful_compilation(&project_root);
        if check_compilation_result {
            let (contract_name, stripped_creation_code) =
                pick_creation_info(&address.to_string()).expect("creation code not found");
            assert_compilation_result(rv, contract_name, stripped_creation_code);
        }
        std::fs::remove_dir_all(project_root).unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_clone_single_file_contract() {
        let address = "0x35Fb958109b70799a8f9Bc2a8b1Ee4cC62034193".parse().unwrap();
        one_test_case(address, true).await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_clone_contract_with_optimization_details() {
        let address = "0x8B3D32cf2bb4d0D16656f4c0b04Fa546274f1545".parse().unwrap();
        one_test_case(address, true).await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_clone_contract_with_libraries() {
        let address = "0xDb53f47aC61FE54F456A4eb3E09832D08Dd7BEec".parse().unwrap();
        one_test_case(address, true).await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_clone_contract_with_metadata() {
        let address = "0x71356E37e0368Bd10bFDbF41dC052fE5FA24cD05".parse().unwrap();
        one_test_case(address, true).await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_clone_contract_with_relative_import() {
        let address = "0x3a23F943181408EAC424116Af7b7790c94Cb97a5".parse().unwrap();
        one_test_case(address, false).await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_clone_contract_with_original_remappings() {
        let address = "0x9ab6b21cdf116f611110b048987e58894786c244".parse().unwrap();
        one_test_case(address, false).await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_clone_contract_with_relative_import2() {
        let address = "0x044b75f554b886A065b9567891e45c79542d7357".parse().unwrap();
        one_test_case(address, false).await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_clone_contract_with_nested_src() {
        let address = "0x9d27527Ada2CF29fBDAB2973cfa243845a08Bd3F".parse().unwrap();
        one_test_case(address, false).await
    }

    fn pick_creation_info(address: &str) -> Option<(&'static str, &'static str)> {
        for (addr, contract_name, creation_code) in &CREATION_ARRAY {
            if address == *addr {
                return Some((contract_name, creation_code));
            }
        }

        None
    }

    // remember to remove CBOR metadata from the creation code
    const CREATION_ARRAY: [(&str, &str, &str); 4] = [
        (
            "0x35Fb958109b70799a8f9Bc2a8b1Ee4cC62034193",
            "BearXNFTStaking",
            "608060405234801561001057600080fd5b50613000806100206000396000f3fe608060405234801561001057600080fd5b50600436106102265760003560e01c80638129fc1c11610130578063bca35a71116100b8578063dada55011161007c578063dada550114610458578063f2fde38b1461046b578063f83d08ba1461047e578063fbb0022714610486578063fccd7f721461048e57600080fd5b8063bca35a71146103fa578063bf9befb11461040d578063c89d5b8b14610416578063d5d423001461041e578063d976e09f1461042657600080fd5b8063b1c92f95116100ff578063b1c92f95146103c5578063b549445c146103ce578063b81f8e89146103d6578063b9ade5b7146103de578063ba0848db146103e757600080fd5b80638129fc1c146103905780638da5cb5b14610398578063aaed083b146103a9578063b10dcc93146103b257600080fd5b8063367c164e116101b35780635923489b116101825780635923489b146103245780636e2751211461034f578063706ce3e114610362578063715018a614610375578063760a2e8a1461037d57600080fd5b8063367c164e146102bd57806338ff8a85146102d05780633a17f4f0146102f1578063426233601461030457600080fd5b8063206635e7116101fa578063206635e71461026d5780632afe761a146102805780632bd30f1114610289578063305f839a146102ab57806333ddacd1146102b457600080fd5b8062944f621461022b5780630d00368b146102405780630e8feed41461025c578063120957fd14610264575b600080fd5b61023e610239366004612aa4565b6104bc565b005b61024960735481565b6040519081526020015b60405180910390f35b61023e61053a565b610249606d5481565b61023e61027b366004612b2c565b61057e565b610249606f5481565b60785461029b90610100900460ff1681565b6040519015158152602001610253565b61024960715481565b61024960765481565b61023e6102cb366004612bc2565b6105d1565b6102e36102de366004612aa4565b610829565b604051610253929190612c16565b61023e6102ff366004612aa4565b6109e1565b610317610312366004612aa4565b610a56565b6040516102539190612c2f565b606a54610337906001600160a01b031681565b6040516001600160a01b039091168152602001610253565b6102e361035d366004612aa4565b610b4c565b606b54610337906001600160a01b031681565b61023e610cf8565b61029b61038b366004612aa4565b610d2e565b61023e610dc2565b6033546001600160a01b0316610337565b61024960705481565b61023e6103c0366004612b2c565b610fc0565b610249606e5481565b61023e611236565b61023e6112bb565b61024960725481565b6102e36103f5366004612aa4565b6112ef565b61023e610408366004612aa4565b61149b565b610249606c5481565b610249611510565b606f54610249565b610439610434366004612aa4565b611594565b6040805192151583526001600160a01b03909116602083015201610253565b606954610337906001600160a01b031681565b61023e610479366004612aa4565b6115cf565b61023e611667565b61023e6116a0565b6104a161049c366004612aa4565b6116e1565b60408051938452602084019290925290820152606001610253565b6033546001600160a01b031633146104ef5760405162461bcd60e51b81526004016104e690612c42565b60405180910390fd5b606b546001600160a01b03163b6105185760405162461bcd60e51b81526004016104e690612c77565b606b80546001600160a01b0319166001600160a01b0392909216919091179055565b600061054533611594565b509050806105655760405162461bcd60e51b81526004016104e690612cae565b61056e33610d2e565b61057b5761057b33611c73565b50565b610587336116e1565b505060765560005b81518110156105cd576105bb8282815181106105ad576105ad612cda565b602002602001015133611d7b565b806105c581612d06565b91505061058f565b5050565b606a54604051636eb1769f60e11b815233600482015273871770e3e03bfaefa3597056e540a1a9c9ac7f6b602482015282916001600160a01b03169063dd62ed3e90604401602060405180830381865afa158015610633573d6000803e3d6000fd5b505050506040513d601f19601f820116820180604052508101906106579190612d21565b10156106bb5760405162461bcd60e51b815260206004820152602d60248201527f596f75206861766520746f20617070726f766520726f6f747820746f2073746160448201526c1ada5b99c818dbdb9d1c9858dd609a1b60648201526084016104e6565b606a546040516323b872dd60e01b815233600482015273871770e3e03bfaefa3597056e540a1a9c9ac7f6b6024820152604481018390526001600160a01b03909116906323b872dd906064016020604051808303816000875af1158015610726573d6000803e3d6000fd5b505050506040513d601f19601f8201168201806040525081019061074a9190612d3a565b50606a546040516326c7e79d60e21b8152600481018390526001600160a01b0390911690639b1f9e7490602401600060405180830381600087803b15801561079157600080fd5b505af11580156107a5573d6000803e3d6000fd5b5050606b546001600160a01b031691506379c650689050336107c88460056120a1565b6040516001600160e01b031960e085901b1681526001600160a01b0390921660048301526024820152604401600060405180830381600087803b15801561080e57600080fd5b505af1158015610822573d6000803e3d6000fd5b5050505050565b600060606000805b6001600160a01b0385166000908152607460205260409020548110156108bc576001600160a01b0385166000908152607460205260409020805461089791908390811061088057610880612cda565b906000526020600020906005020160000154612129565b156108aa576108a7600183612d5c565b91505b806108b481612d06565b915050610831565b5060008167ffffffffffffffff8111156108d8576108d8612ac1565b604051908082528060200260200182016040528015610901578160200160208202803683370190505b5090506000805b6001600160a01b0387166000908152607460205260409020548110156109d5576001600160a01b0387166000908152607460205260409020805461095791908390811061088057610880612cda565b156109c3576001600160a01b038716600090815260746020526040902080548290811061098657610986612cda565b9060005260206000209060050201600001548383815181106109aa576109aa612cda565b60209081029190910101526109c0826001612154565b91505b806109cd81612d06565b915050610908565b50919590945092505050565b6033546001600160a01b03163314610a0b5760405162461bcd60e51b81526004016104e690612c42565b6069546001600160a01b03163b610a345760405162461bcd60e51b81526004016104e690612c77565b606980546001600160a01b0319166001600160a01b0392909216919091179055565b6001600160a01b0381166000908152607460205260408120546060919067ffffffffffffffff811115610a8b57610a8b612ac1565b604051908082528060200260200182016040528015610ab4578160200160208202803683370190505b50905060005b6001600160a01b038416600090815260746020526040902054811015610b45576001600160a01b0384166000908152607460205260409020805482908110610b0457610b04612cda565b906000526020600020906005020160000154828281518110610b2857610b28612cda565b602090810291909101015280610b3d81612d06565b915050610aba565b5092915050565b600060606000805b6001600160a01b038516600090815260746020526040902054811015610bdf576001600160a01b03851660009081526074602052604090208054610bba919083908110610ba357610ba3612cda565b9060005260206000209060050201600001546121b3565b15610bcd57610bca826001612154565b91505b80610bd781612d06565b915050610b54565b5060008167ffffffffffffffff811115610bfb57610bfb612ac1565b604051908082528060200260200182016040528015610c24578160200160208202803683370190505b5090506000805b6001600160a01b0387166000908152607460205260409020548110156109d5576001600160a01b03871660009081526074602052604090208054610c7a919083908110610ba357610ba3612cda565b15610ce6576001600160a01b0387166000908152607460205260409020805482908110610ca957610ca9612cda565b906000526020600020906005020160000154838381518110610ccd57610ccd612cda565b6020908102919091010152610ce3826001612154565b91505b80610cf081612d06565b915050610c2b565b6033546001600160a01b03163314610d225760405162461bcd60e51b81526004016104e690612c42565b610d2c60006121d0565b565b60006001815b6001600160a01b038416600090815260746020526040902054811015610b45576001600160a01b03841660009081526074602052604081208054610d9a919084908110610d8357610d83612cda565b906000526020600020906005020160010154612222565b9050603c8111610daa5750610db0565b60009250505b80610dba81612d06565b915050610d34565b600054610100900460ff16610ddd5760005460ff1615610de1565b303b155b610e445760405162461bcd60e51b815260206004820152602e60248201527f496e697469616c697a61626c653a20636f6e747261637420697320616c72656160448201526d191e481a5b9a5d1a585b1a5e995960921b60648201526084016104e6565b600054610100900460ff16158015610e66576000805461ffff19166101011790555b610e6e61223c565b606580546001600160a01b0319908116737a250d5630b4cf539739df2c5dacb4c659f2488d1790915560668054821673c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2179055620151806067556312cc030060685560698054821673e22e1e620dffb03065cd77db0162249c0c91bf01179055606a8054821673d718ad25285d65ef4d79262a6cd3aea6a8e01023179055606b80549091167399cfdf48d0ba4885a73786148a2f89d86c7021701790556000606c5568056bc75e2d63100000606d556802b5e3af16b1880000606e55690257058e269742680000606f819055681b1ae4d6e2ef5000006070819055610bb8607181905591610f709190612d74565b610f7a9190612d8b565b607255607154606e54606d54610f909190612d74565b610f9a9190612d8b565b60735560006076556078805460ff19169055801561057b576000805461ff001916905550565b6000610fcb33611594565b50905080610feb5760405162461bcd60e51b81526004016104e690612cae565b607854610100900460ff161561102c5760405162461bcd60e51b8152602060048201526006602482015265131bd8dad95960d21b60448201526064016104e6565b600061103733610a56565b90508051835111156110775760405162461bcd60e51b81526020600482015260096024820152684964206572726f727360b81b60448201526064016104e6565b6000805b84518110156110fd5760005b83518110156110ea578381815181106110a2576110a2612cda565b60200260200101518683815181106110bc576110bc612cda565b602002602001015114156110d8576110d5836001612154565b92505b806110e281612d06565b915050611087565b50806110f581612d06565b91505061107b565b50835181141561123057835161112761111e82678ac7230489e800006120a1565b606f5490612154565b606f55611132612273565b600060768190555b855181101561122d5760695486516001600160a01b03909116906323b872dd90309033908a908690811061117057611170612cda565b60209081029190910101516040516001600160e01b031960e086901b1681526001600160a01b0393841660048201529290911660248301526044820152606401600060405180830381600087803b1580156111ca57600080fd5b505af11580156111de573d6000803e3d6000fd5b5050606c80549250905060006111f383612dad565b919050555061121b3387838151811061120e5761120e612cda565b602002602001015161229b565b8061122581612d06565b91505061113a565b50505b50505050565b60785460ff166112ac5760005b60755481101561057b576001607760006075848154811061126657611266612cda565b6000918252602080832091909101546001600160a01b031683528201929092526040019020805460ff1916911515919091179055806112a481612d06565b915050611243565b6078805460ff19166001179055565b60006112c633611594565b509050806112e65760405162461bcd60e51b81526004016104e690612cae565b61057b33612470565b600060606000805b6001600160a01b038516600090815260746020526040902054811015611382576001600160a01b0385166000908152607460205260409020805461135d91908390811061134657611346612cda565b906000526020600020906005020160000154612574565b156113705761136d600183612d5c565b91505b8061137a81612d06565b9150506112f7565b5060008167ffffffffffffffff81111561139e5761139e612ac1565b6040519080825280602002602001820160405280156113c7578160200160208202803683370190505b5090506000805b6001600160a01b0387166000908152607460205260409020548110156109d5576001600160a01b0387166000908152607460205260409020805461141d91908390811061134657611346612cda565b15611489576001600160a01b038716600090815260746020526040902080548290811061144c5761144c612cda565b90600052602060002090600502016000015483838151811061147057611470612cda565b6020908102919091010152611486826001612154565b91505b8061149381612d06565b9150506113ce565b6033546001600160a01b031633146114c55760405162461bcd60e51b81526004016104e690612c42565b606a546001600160a01b03163b6114ee5760405162461bcd60e51b81526004016104e690612c77565b606a80546001600160a01b0319166001600160a01b0392909216919091179055565b6000806064606f5460016115249190612dc4565b61152e9190612d8b565b611539906001612d5c565b606d546115469190612dc4565b90506000606d54826115589190612d74565b905060006001606d548361156c9190612d8b565b6115769190612d8b565b611581906001612dc4565b61158c906064612dc4565b949350505050565b6001600160a01b038116600090815260776020526040812054819060ff161515600114156115c457506001929050565b506000928392509050565b6033546001600160a01b031633146115f95760405162461bcd60e51b81526004016104e690612c42565b6001600160a01b03811661165e5760405162461bcd60e51b815260206004820152602660248201527f4f776e61626c653a206e6577206f776e657220697320746865207a65726f206160448201526564647265737360d01b60648201526084016104e6565b61057b816121d0565b73d0d725208fd36be1561050fc1dd6a651d7ea7c89331415610d2c576078805461ff001981166101009182900460ff1615909102179055565b60006116ab33611594565b509050806116cb5760405162461bcd60e51b81526004016104e690612cae565b6116d433610d2e565b61057b5761057b336125aa565b600080808080808087816116f482610829565b509050600061170283610b4c565b50905060058110611a3b57600160005b6001600160a01b038516600090815260746020526040902054811015611a0e576001600160a01b0385166000908152607460205260408120805461177891908490811061176157611761612cda565b906000526020600020906005020160040154612222565b6001600160a01b038716600090815260746020526040902080549192506117a99184908110610ba357610ba3612cda565b806117de57506001600160a01b038616600090815260746020526040902080546117de91908490811061088057610880612cda565b80156117ea5750600181105b156117f457600092505b82801561181857506001600160a01b03861660009081526074602052604090205415155b156118715761186a8561182c83600a612dc4565b6118369190612dc4565b6118648661184585600a612dc4565b61184f9190612dc4565b60765461186490670de0b6b3a7640000612798565b90612154565b995061188e565b8261188e5760765461188b90670de0b6b3a7640000612798565b99505b6001600160a01b038616600090815260746020526040902080546118bd91908490811061088057610880612cda565b15611950576001600160a01b038616600090815260746020526040812080546119089190859081106118f1576118f1612cda565b906000526020600020906005020160020154612222565b90508061192861192182680ad78ebc5ac6200000612dc4565b8c90612154565b9a5061194761194082680ad78ebc5ac6200000612dc4565b8b90612154565b995050506119fb565b6001600160a01b0386166000908152607460205260409020805461197f919084908110610ba357610ba3612cda565b156119fb576001600160a01b038616600090815260746020526040812080546119b39190859081106118f1576118f1612cda565b905060008190506119d76002606d54846119cd9190612dc4565b6119219190612d8b565b9a506119f66002606d54836119ec9190612dc4565b6119409190612d8b565b995050505b5080611a0681612d06565b915050611712565b508515611a3557606b54606654611a32916001600160a01b039081169116886127da565b94505b50611c4f565b60005b6001600160a01b038416600090815260746020526040902054811015611c28576001600160a01b03841660009081526074602052604090208054611a8d91908390811061088057610880612cda565b15611b9f576001600160a01b03841660009081526074602052604081208054611ac191908490811061176157611761612cda565b9050611ad8611ad182600a612dc4565b8a90612154565b98506000611b1560746000886001600160a01b03166001600160a01b0316815260200190815260200160002084815481106118f1576118f1612cda565b9050611b2d611ad182680ad78ebc5ac6200000612dc4565b98506000611b8160746000896001600160a01b03166001600160a01b031681526020019081526020016000208581548110611b6a57611b6a612cda565b906000526020600020906005020160030154612222565b9050611b99611ad182680ad78ebc5ac6200000612dc4565b98505050505b6001600160a01b03841660009081526074602052604090208054611bce919083908110610ba357610ba3612cda565b15611c16576001600160a01b03841660009081526074602052604081208054611c0291908490811061176157611761612cda565b9050611c12611ad182600a612dc4565b9850505b80611c2081612d06565b915050611a3e565b508415611c4f57606b54606654611c4c916001600160a01b039081169116876127da565b93505b611c6187670de0b6b3a7640000612dc4565b9b959a50929850939650505050505050565b6000611c7e826116e1565b5091505080156105cd57606b5460405163a9059cbb60e01b81526001600160a01b038481166004830152602482018490529091169063a9059cbb906044016020604051808303816000875af1158015611cdb573d6000803e3d6000fd5b505050506040513d601f19601f82011682018060405250810190611cff9190612d3a565b5060005b6001600160a01b038316600090815260746020526040902054811015611d76576001600160a01b0383166000908152607460205260409020805442919083908110611d5057611d50612cda565b600091825260209091206002600590920201015580611d6e81612d06565b915050611d03565b505050565b607054606f5410611db857607354606d6000828254611d9a9190612d74565b9091555050606f54611db490678ac7230489e80000612905565b606f555b6069546040516331a9108f60e11b8152600481018490526001600160a01b03838116921690636352211e90602401602060405180830381865afa158015611e03573d6000803e3d6000fd5b505050506040513d601f19601f82011682018060405250810190611e279190612de3565b6001600160a01b031614611e7d5760405162461bcd60e51b815260206004820152601e60248201527f596f7520617265206e6f742061206f776e6572206f6620746865206e6674000060448201526064016104e6565b60695460405163e985e9c560e01b81523360048201523060248201526001600160a01b039091169063e985e9c590604401602060405180830381865afa158015611ecb573d6000803e3d6000fd5b505050506040513d601f19601f82011682018060405250810190611eef9190612d3a565b1515600114611f575760405162461bcd60e51b815260206004820152602e60248201527f596f752073686f756c6420617070726f7665206e667420746f2074686520737460448201526d185ada5b99c818dbdb9d1c9858dd60921b60648201526084016104e6565b6069546040516323b872dd60e01b81526001600160a01b03838116600483015230602483015260448201859052909116906323b872dd90606401600060405180830381600087803b158015611fab57600080fd5b505af1158015611fbf573d6000803e3d6000fd5b505050506000611fce82611594565b50905060006040518060a001604052808581526020014281526020014281526020014281526020014281525090506120126001606c5461215490919063ffffffff16565b606c556001600160a01b03831660009081526074602090815260408083208054600181810183559185529383902085516005909502019384559184015191830191909155820151600282015560608201516003820155608082015160049091015581611230576001600160a01b0383166000908152607760205260409020805460ff1916600117905550505050565b6000826120b057506000612123565b60006120bc8385612dc4565b9050826120c98583612d8b565b146121205760405162461bcd60e51b815260206004820152602160248201527f536166654d6174683a206d756c7469706c69636174696f6e206f766572666c6f6044820152607760f81b60648201526084016104e6565b90505b92915050565b60008064e8d4a510008310158015612146575064e8d4a510058311155b156121235750600192915050565b6000806121618385612d5c565b9050838110156121205760405162461bcd60e51b815260206004820152601b60248201527f536166654d6174683a206164646974696f6e206f766572666c6f77000000000060448201526064016104e6565b600080610e7483116121c757506001612123565b50600092915050565b603380546001600160a01b038381166001600160a01b0319831681179093556040519116919082907f8be0079c531659141344cd1fd0a4f28419497f9722a3daafe3b4186f6b6457e090600090a35050565b6067546000906122328342612d74565b6121239190612d8b565b600054610100900460ff166122635760405162461bcd60e51b81526004016104e690612e00565b61226b612947565b610d2c61296e565b61227c33612470565b61228533610d2e565b610d2c5761229233611c73565b610d2c336125aa565b60005b6001600160a01b038316600090815260746020526040902054811015612428576001600160a01b03831660009081526074602052604090208054839190839081106122eb576122eb612cda565b9060005260206000209060050201600001541415612416576001600160a01b0383166000908152607460205260409020805461232990600190612d74565b8154811061233957612339612cda565b906000526020600020906005020160746000856001600160a01b03166001600160a01b03168152602001908152602001600020828154811061237d5761237d612cda565b60009182526020808320845460059093020191825560018085015490830155600280850154908301556003808501549083015560049384015493909101929092556001600160a01b03851681526074909152604090208054806123e2576123e2612e4b565b6000828152602081206005600019909301928302018181556001810182905560028101829055600381018290556004015590555b8061242081612d06565b91505061229e565b506001600160a01b0382166000908152607460205260409020546105cd576001600160a01b038216600090815260746020526040812061246791612a3f565b6105cd8261299e565b600061247b826116e1565b509091505080156105cd57606a5460405163a9059cbb60e01b81526001600160a01b038481166004830152602482018490529091169063a9059cbb906044016020604051808303816000875af11580156124d9573d6000803e3d6000fd5b505050506040513d601f19601f820116820180604052508101906124fd9190612d3a565b5060005b6001600160a01b038316600090815260746020526040902054811015611d76576001600160a01b038316600090815260746020526040902080544291908390811061254e5761254e612cda565b60009182526020909120600460059092020101558061256c81612d06565b915050612501565b60006509184e72a00682101561258c57506000919050565b6509184e72b4b38211156125a257506000919050565b506001919050565b60006125b5826116e1565b5091505080156105cd57606b5460655460405163095ea7b360e01b81526001600160a01b0391821660048201526024810184905291169063095ea7b3906044016020604051808303816000875af1158015612614573d6000803e3d6000fd5b505050506040513d601f19601f820116820180604052508101906126389190612d3a565b5060408051600280825260608083018452926020830190803683375050606b5482519293506001600160a01b03169183915060009061267957612679612cda565b6001600160a01b0392831660209182029290920101526066548251911690829060019081106126aa576126aa612cda565b6001600160a01b03928316602091820292909201015260655460405163791ac94760e01b815291169063791ac947906126f0908590600090869089904290600401612e9a565b600060405180830381600087803b15801561270a57600080fd5b505af115801561271e573d6000803e3d6000fd5b5050505060005b6001600160a01b038416600090815260746020526040902054811015611230576001600160a01b038416600090815260746020526040902080544291908390811061277257612772612cda565b60009182526020909120600360059092020101558061279081612d06565b915050612725565b600061212083836040518060400160405280601a81526020017f536166654d6174683a206469766973696f6e206279207a65726f0000000000008152506129d7565b6040805160028082526060808301845260009390929190602083019080368337019050509050848160008151811061281457612814612cda565b60200260200101906001600160a01b031690816001600160a01b031681525050838160018151811061284857612848612cda565b6001600160a01b03928316602091820292909201015260655460405163d06ca61f60e01b8152600092919091169063d06ca61f9061288c9087908690600401612ed6565b600060405180830381865afa1580156128a9573d6000803e3d6000fd5b505050506040513d6000823e601f3d908101601f191682016040526128d19190810190612eef565b905080600183516128e29190612d74565b815181106128f2576128f2612cda565b6020026020010151925050509392505050565b600061212083836040518060400160405280601e81526020017f536166654d6174683a207375627472616374696f6e206f766572666c6f770000815250612a0e565b600054610100900460ff16610d2c5760405162461bcd60e51b81526004016104e690612e00565b600054610100900460ff166129955760405162461bcd60e51b81526004016104e690612e00565b610d2c336121d0565b6000806129aa83611594565b915091508115611d76576001600160a01b03166000908152607760205260409020805460ff191690555050565b600081836129f85760405162461bcd60e51b81526004016104e69190612f75565b506000612a058486612d8b565b95945050505050565b60008184841115612a325760405162461bcd60e51b81526004016104e69190612f75565b506000612a058486612d74565b508054600082556005029060005260206000209081019061057b91905b80821115612a8b5760008082556001820181905560028201819055600382018190556004820155600501612a5c565b5090565b6001600160a01b038116811461057b57600080fd5b600060208284031215612ab657600080fd5b813561212081612a8f565b634e487b7160e01b600052604160045260246000fd5b604051601f8201601f1916810167ffffffffffffffff81118282101715612b0057612b00612ac1565b604052919050565b600067ffffffffffffffff821115612b2257612b22612ac1565b5060051b60200190565b60006020808385031215612b3f57600080fd5b823567ffffffffffffffff811115612b5657600080fd5b8301601f81018513612b6757600080fd5b8035612b7a612b7582612b08565b612ad7565b81815260059190911b82018301908381019087831115612b9957600080fd5b928401925b82841015612bb757833582529284019290840190612b9e565b979650505050505050565b600060208284031215612bd457600080fd5b5035919050565b600081518084526020808501945080840160005b83811015612c0b57815187529582019590820190600101612bef565b509495945050505050565b82815260406020820152600061158c6040830184612bdb565b6020815260006121206020830184612bdb565b6020808252818101527f4f776e61626c653a2063616c6c6572206973206e6f7420746865206f776e6572604082015260600190565b60208082526017908201527f41646472657373206973206e6f7420636f6e7472616374000000000000000000604082015260600190565b6020808252601290820152712cb7ba9030b932903737ba1039ba30b5b2b960711b604082015260600190565b634e487b7160e01b600052603260045260246000fd5b634e487b7160e01b600052601160045260246000fd5b6000600019821415612d1a57612d1a612cf0565b5060010190565b600060208284031215612d3357600080fd5b5051919050565b600060208284031215612d4c57600080fd5b8151801515811461212057600080fd5b60008219821115612d6f57612d6f612cf0565b500190565b600082821015612d8657612d86612cf0565b500390565b600082612da857634e487b7160e01b600052601260045260246000fd5b500490565b600081612dbc57612dbc612cf0565b506000190190565b6000816000190483118215151615612dde57612dde612cf0565b500290565b600060208284031215612df557600080fd5b815161212081612a8f565b6020808252602b908201527f496e697469616c697a61626c653a20636f6e7472616374206973206e6f74206960408201526a6e697469616c697a696e6760a81b606082015260800190565b634e487b7160e01b600052603160045260246000fd5b600081518084526020808501945080840160005b83811015612c0b5781516001600160a01b031687529582019590820190600101612e75565b85815284602082015260a060408201526000612eb960a0830186612e61565b6001600160a01b0394909416606083015250608001529392505050565b82815260406020820152600061158c6040830184612e61565b60006020808385031215612f0257600080fd5b825167ffffffffffffffff811115612f1957600080fd5b8301601f81018513612f2a57600080fd5b8051612f38612b7582612b08565b81815260059190911b82018301908381019087831115612f5757600080fd5b928401925b82841015612bb757835182529284019290840190612f5c565b600060208083528351808285015260005b81811015612fa257858101830151858201604001528201612f86565b81811115612fb4576000604083870101525b50601f01601f191692909201604001939250505056fe",
        ),
        (
            "0x8B3D32cf2bb4d0D16656f4c0b04Fa546274f1545",
            "GovernorCharlieDelegate",
            "608060405234801561001057600080fd5b50613e45806100206000396000f3fe60806040526004361061031a5760003560e01c80637b3c71d3116101ab578063d50572ee116100f7578063f0843ba811610095578063fc176c041161006f578063fc176c0414610b82578063fc4eee4214610ba2578063fc66ff1414610bb8578063fe0d94c114610bd857600080fd5b8063f0843ba814610b12578063f2b0653714610b32578063f682e04c14610b6257600080fd5b8063de7bc127116100d1578063de7bc127146109ec578063deaaa7cc14610a02578063e23a9a5214610a36578063e837159c14610afc57600080fd5b8063d50572ee146109a0578063da35c664146109b6578063ddf0b009146109cc57600080fd5b8063a6d8784a11610164578063c1a287e21161013e578063c1a287e214610933578063c4d66de81461094a578063c5a8425d1461096a578063c9fb9e871461098a57600080fd5b8063a6d8784a146108e7578063abaac6a8146108fd578063b58131b01461091d57600080fd5b80637b3c71d31461083c5780637bdbe4d01461085c5780637cae57bb14610871578063806bd5811461088757806386d37e8b146108a757806399533365146108c757600080fd5b80632fedff591161026a5780633e4f49e61161022357806350442098116101fd578063504420981461074657806356781388146107665780635c60da1b1461078657806366176743146107be57600080fd5b80633e4f49e6146106d957806340e58ee5146107065780634d6733d21461072657600080fd5b80632fedff59146105ee578063328dd9821461060e57806338bd0dda1461063e5780633932abb11461066b5780633af32abf146106815780633bccf4fd146106b957600080fd5b8063158ef93e116102d757806318b62629116102b157806318b626291461056e5780631dfb1b5a1461058457806320606b70146105a457806324bc1a64146105d857600080fd5b8063158ef93e146104f757806317977c611461052157806317ba1b8b1461054e57600080fd5b8063013cf08b1461031f57806302a251a31461042857806306fdde031461044c5780630825f38f146104a25780630ea2d98c146104b7578063140499ea146104d7575b600080fd5b34801561032b57600080fd5b506103b361033a3660046132ee565b60096020819052600091825260409091208054600182015460028301546007840154600885015495850154600a860154600b870154600c880154600d890154600e9099015497996001600160a01b0390971698959794969593949293919260ff808316936101008404821693620100009004909116918d565b604080519d8e526001600160a01b03909c1660208e01529a8c019990995260608b019790975260808a019590955260a089019390935260c088019190915260e08701521515610100860152151561012085015215156101408401526101608301526101808201526101a0015b60405180910390f35b34801561043457600080fd5b5061043e60045481565b60405190815260200161041f565b34801561045857600080fd5b506104956040518060400160405280601a81526020017f496e7465726573742050726f746f636f6c20476f7665726e6f7200000000000081525081565b60405161041f9190613363565b6104b56104b0366004613450565b610beb565b005b3480156104c357600080fd5b506104b56104d23660046132ee565b610e61565b3480156104e357600080fd5b506104b56104f23660046134d6565b610ec6565b34801561050357600080fd5b506012546105119060ff1681565b604051901515815260200161041f565b34801561052d57600080fd5b5061043e61053c3660046134d6565b600a6020526000908152604090205481565b34801561055a57600080fd5b506104b56105693660046132ee565b610f07565b34801561057a57600080fd5b5061043e600f5481565b34801561059057600080fd5b506104b561059f3660046132ee565b610f64565b3480156105b057600080fd5b5061043e7f8cad95687ba82c2ce50e74f7b754645e5117c3a5bec8151c0726d5857980a86681565b3480156105e457600080fd5b5061043e60015481565b3480156105fa57600080fd5b506104b56106093660046132ee565b610fc1565b34801561061a57600080fd5b5061062e6106293660046132ee565b61101e565b60405161041f94939291906135ba565b34801561064a57600080fd5b5061043e6106593660046134d6565b600d6020526000908152604090205481565b34801561067757600080fd5b5061043e60035481565b34801561068d57600080fd5b5061051161069c3660046134d6565b6001600160a01b03166000908152600d6020526040902054421090565b3480156106c557600080fd5b506104b56106d4366004613623565b6112af565b3480156106e557600080fd5b506106f96106f43660046132ee565b611516565b60405161041f9190613687565b34801561071257600080fd5b506104b56107213660046132ee565b61169e565b34801561073257600080fd5b506104b56107413660046136af565b611b80565b34801561075257600080fd5b506104b56107613660046132ee565b611c45565b34801561077257600080fd5b506104b56107813660046136d9565b611ca2565b34801561079257600080fd5b506000546107a6906001600160a01b031681565b6040516001600160a01b03909116815260200161041f565b3480156107ca57600080fd5b506108146107d9366004613705565b601160209081526000928352604080842090915290825290205460ff808216916101008104909116906201000090046001600160601b031683565b60408051931515845260ff90921660208401526001600160601b03169082015260600161041f565b34801561084857600080fd5b506104b5610857366004613728565b611d09565b34801561086857600080fd5b5061043e600a81565b34801561087d57600080fd5b5061043e600c5481565b34801561089357600080fd5b506104b56108a23660046132ee565b611d58565b3480156108b357600080fd5b506104b56108c23660046132ee565b611db5565b3480156108d357600080fd5b506104b56108e23660046134d6565b611e12565b3480156108f357600080fd5b5061043e60155481565b34801561090957600080fd5b506104b56109183660046132ee565b611e8b565b34801561092957600080fd5b5061043e60055481565b34801561093f57600080fd5b5061043e6212750081565b34801561095657600080fd5b506104b56109653660046134d6565b611ee8565b34801561097657600080fd5b50600e546107a6906001600160a01b031681565b34801561099657600080fd5b5061043e60135481565b3480156109ac57600080fd5b5061043e60025481565b3480156109c257600080fd5b5061043e60075481565b3480156109d857600080fd5b506104b56109e73660046132ee565b611fd9565b3480156109f857600080fd5b5061043e60105481565b348015610a0e57600080fd5b5061043e7f150214d74d59b7d1e90c73fc22ef3d991dd0a76b046543d4d80ab92d2a50328f81565b348015610a4257600080fd5b50610acc610a51366004613705565b60408051606081018252600080825260208201819052918101919091525060009182526011602090815260408084206001600160a01b03939093168452918152918190208151606081018352905460ff8082161515835261010082041693820193909352620100009092046001600160601b03169082015290565b6040805182511515815260208084015160ff1690820152918101516001600160601b03169082015260600161041f565b348015610b0857600080fd5b5061043e60145481565b348015610b1e57600080fd5b506104b5610b2d3660046132ee565b61238f565b348015610b3e57600080fd5b50610511610b4d3660046132ee565b600b6020526000908152604090205460ff1681565b348015610b6e57600080fd5b5061043e610b7d3660046139b0565b6123ec565b348015610b8e57600080fd5b506104b5610b9d3660046132ee565b612a48565b348015610bae57600080fd5b5061043e60065481565b348015610bc457600080fd5b506008546107a6906001600160a01b031681565b6104b5610be63660046132ee565b612aa5565b60008585858585604051602001610c06959493929190613a91565b60408051601f1981840301815291815281516020928301206000818152600b90935291205490915060ff16610c7b5760405162461bcd60e51b81526020600482015260166024820152753a3c103430b9b713ba103132b2b71038bab2bab2b21760511b60448201526064015b60405180910390fd5b81421015610ccb5760405162461bcd60e51b815260206004820152601d60248201527f7478206861736e2774207375727061737365642074696d656c6f636b2e0000006044820152606401610c72565b610cd86212750083613af3565b421115610d165760405162461bcd60e51b815260206004820152600c60248201526b3a3c1034b99039ba30b6329760a11b6044820152606401610c72565b6000818152600b60205260409020805460ff191690558351606090610d3c575082610d68565b848051906020012084604051602001610d56929190613b0b565b60405160208183030381529060405290505b6000876001600160a01b03168783604051610d839190613b3c565b60006040518083038185875af1925050503d8060008114610dc0576040519150601f19603f3d011682016040523d82523d6000602084013e610dc5565b606091505b5050905080610e0f5760405162461bcd60e51b81526020600482015260166024820152753a3c1032bc32b1baba34b7b7103932bb32b93a32b21760511b6044820152606401610c72565b876001600160a01b0316837fa560e3198060a2f10670c1ec5b403077ea6ae93ca8de1c32b451dc1a943cd6e789898989604051610e4f9493929190613b58565b60405180910390a35050505050505050565b333014610e805760405162461bcd60e51b8152600401610c7290613b95565b600480549082905560408051828152602081018490527f7e3f7f0708a84de9203036abaa450dccc85ad5ff52f78c170f3edb55cf5e882891015b60405180910390a15050565b333014610ee55760405162461bcd60e51b8152600401610c7290613b95565b600880546001600160a01b0319166001600160a01b0392909216919091179055565b333014610f265760405162461bcd60e51b8152600401610c7290613b95565b600580549082905560408051828152602081018490527fccb45da8d5717e6c4544694297c4ba5cf151d455c9bb0ed4fc7a38411bc054619101610eba565b333014610f835760405162461bcd60e51b8152600401610c7290613b95565b600380549082905560408051828152602081018490527fc565b045403dc03c2eea82b81a0465edad9e2e7fc4d97e11421c209da93d7a939101610eba565b333014610fe05760405162461bcd60e51b8152600401610c7290613b95565b601480549082905560408051828152602081018490527f519a192fe8db9e38785eb494c69f530ddb21b9e34322f8d08fe29bd3849749889101610eba565b606080606080600060096000878152602001908152602001600020905080600301816004018260050183600601838054806020026020016040519081016040528092919081815260200182805480156110a057602002820191906000526020600020905b81546001600160a01b03168152600190910190602001808311611082575b50505050509350828054806020026020016040519081016040528092919081815260200182805480156110f257602002820191906000526020600020905b8154815260200190600101908083116110de575b5050505050925081805480602002602001604051908101604052809291908181526020016000905b828210156111c657838290600052602060002001805461113990613bcc565b80601f016020809104026020016040519081016040528092919081815260200182805461116590613bcc565b80156111b25780601f10611187576101008083540402835291602001916111b2565b820191906000526020600020905b81548152906001019060200180831161119557829003601f168201915b50505050508152602001906001019061111a565b50505050915080805480602002602001604051908101604052809291908181526020016000905b8282101561129957838290600052602060002001805461120c90613bcc565b80601f016020809104026020016040519081016040528092919081815260200182805461123890613bcc565b80156112855780601f1061125a57610100808354040283529160200191611285565b820191906000526020600020905b81548152906001019060200180831161126857829003601f168201915b5050505050815260200190600101906111ed565b5050505090509450945094509450509193509193565b604080518082018252601a81527f496e7465726573742050726f746f636f6c20476f7665726e6f7200000000000060209182015281517f8cad95687ba82c2ce50e74f7b754645e5117c3a5bec8151c0726d5857980a866818301527f75a838dcd8ee5903cc7f4a5799344d0080864f57a6e9911f8bdfb4c8ddce9b5481840152466060820152306080808301919091528351808303909101815260a0820184528051908301207f150214d74d59b7d1e90c73fc22ef3d991dd0a76b046543d4d80ab92d2a50328f60c083015260e0820189905260ff8816610100808401919091528451808403909101815261012083019094528351939092019290922061190160f01b6101408401526101428301829052610162830181905290916000906101820160408051601f198184030181528282528051602091820120600080855291840180845281905260ff8a169284019290925260608301889052608083018790529092509060019060a0016020604051602081039080840390855afa15801561143c573d6000803e3d6000fd5b5050604051601f1901519150506001600160a01b03811661149f5760405162461bcd60e51b815260206004820181905260248201527f63617374566f746542795369673a20696e76616c6964207369676e61747572656044820152606401610c72565b88816001600160a01b03167fb8e138887d0aa13bab447e82de9d5c1777041ecd21ca36ba824ff1e6c07ddda48a6114d7858e8e612c90565b6040805160ff90931683526001600160601b039091166020830152606090820181905260009082015260800160405180910390a3505050505050505050565b6000816007541015801561152b575060065482115b6115775760405162461bcd60e51b815260206004820152601a60248201527f73746174653a20696e76616c69642070726f706f73616c2069640000000000006044820152606401610c72565b600082815260096020908152604080832060018101546001600160a01b03168452600d90925290912054600c82015442919091109060ff16156115be575060029392505050565b816007015443116115d3575060009392505050565b816008015443116115e8575060019392505050565b8080156115fc575081600d015482600a0154115b80611618575080158015611618575081600a0154826009015411155b80611633575080158015611633575081600d01548260090154105b15611642575060039392505050565b6002820154611655575060049392505050565b600c820154610100900460ff1615611671575060079392505050565b6212750082600201546116849190613af3565b4210611694575060069392505050565b5060059392505050565b60076116a982611516565b60078111156116ba576116ba613671565b14156117085760405162461bcd60e51b815260206004820152601d60248201527f63616e742063616e63656c2065786563757465642070726f706f73616c0000006044820152606401610c72565b600081815260096020526040902060018101546001600160a01b0316336001600160a01b0316146119755760018101546001600160a01b03166000908152600d6020526040902054421015611878576005546008546001838101546001600160a01b039283169263782d6fe1929116906117829043613c07565b6040516001600160e01b031960e085901b1681526001600160a01b039092166004830152602482015260440160206040518083038186803b1580156117c657600080fd5b505afa1580156117da573d6000803e3d6000fd5b505050506040513d601f19601f820116820180604052508101906117fe9190613c1e565b6001600160601b03161080156118275750600e546001600160a01b0316336001600160a01b0316145b6118735760405162461bcd60e51b815260206004820152601c60248201527f63616e63656c3a2077686974656c69737465642070726f706f736572000000006044820152606401610c72565b611975565b6005546008546001838101546001600160a01b039283169263782d6fe1929116906118a39043613c07565b6040516001600160e01b031960e085901b1681526001600160a01b039092166004830152602482015260440160206040518083038186803b1580156118e757600080fd5b505afa1580156118fb573d6000803e3d6000fd5b505050506040513d601f19601f8201168201806040525081019061191f9190613c1e565b6001600160601b0316106119755760405162461bcd60e51b815260206004820181905260248201527f63616e63656c3a2070726f706f7365722061626f7665207468726573686f6c646044820152606401610c72565b600c8101805460ff1916600117905560005b6003820154811015611b5057611b3e8260030182815481106119ab576119ab613c47565b6000918252602090912001546004840180546001600160a01b0390921691849081106119d9576119d9613c47565b90600052602060002001548460050184815481106119f9576119f9613c47565b906000526020600020018054611a0e90613bcc565b80601f0160208091040260200160405190810160405280929190818152602001828054611a3a90613bcc565b8015611a875780601f10611a5c57610100808354040283529160200191611a87565b820191906000526020600020905b815481529060010190602001808311611a6a57829003601f168201915b5050505050856006018581548110611aa157611aa1613c47565b906000526020600020018054611ab690613bcc565b80601f0160208091040260200160405190810160405280929190818152602001828054611ae290613bcc565b8015611b2f5780601f10611b0457610100808354040283529160200191611b2f565b820191906000526020600020905b815481529060010190602001808311611b1257829003601f168201915b50505050508660020154612f12565b80611b4881613c5d565b915050611987565b5060405182907f789cf55be980739dad1d0699b93b58e806b51c9d96619bfa8fe0a28abaa7b30c90600090a25050565b333014611b9f5760405162461bcd60e51b8152600401610c7290613b95565b42601554611bad9190613af3565b8110611bf45760405162461bcd60e51b81526020600482015260166024820152750caf0e0d2e4c2e8d2dedc40caf0c6cacac8e640dac2f60531b6044820152606401610c72565b6001600160a01b0382166000818152600d6020908152604091829020849055815192835282018390527f4e7b7545bc5744d0e30425959f4687475774b6c7edad77d24cb51c7d967d45159101610eba565b333014611c645760405162461bcd60e51b8152600401610c7290613b95565b601080549082905560408051828152602081018490527f2a61b867418a359864adca8bb250ea65ee8bd41dbfd0279198d8e7552d4a27c29101610eba565b81337fb8e138887d0aa13bab447e82de9d5c1777041ecd21ca36ba824ff1e6c07ddda483611cd1838583612c90565b6040805160ff90931683526001600160601b039091166020830152606090820181905260009082015260800160405180910390a35050565b83337fb8e138887d0aa13bab447e82de9d5c1777041ecd21ca36ba824ff1e6c07ddda485611d38838583612c90565b8686604051611d4a9493929190613c78565b60405180910390a350505050565b333014611d775760405162461bcd60e51b8152600401610c7290613b95565b601380549082905560408051828152602081018490527f8cb5451eee8feb516cec9cd600201bbc31a30886d70c841a085a3fa69a4294d19101610eba565b333014611dd45760405162461bcd60e51b8152600401610c7290613b95565b600180549082905560408051828152602081018490527fa74554b0f53da47d07ec571d712428b3720460f54f81375fbcf78f6b5f72e7ed9101610eba565b333014611e315760405162461bcd60e51b8152600401610c7290613b95565b600e80546001600160a01b038381166001600160a01b031983168117909355604080519190921680825260208201939093527f80a07e73e552148844a9c216d9724212d609cfa54e9c1a2e97203bdd2c4ad3419101610eba565b333014611eaa5760405162461bcd60e51b8152600401610c7290613b95565b600f80549082905560408051828152602081018490527f80a384652af83fc00bfd40ef94edda7ede83e7db39931b2c889821573f314e239101610eba565b60125460ff1615611f3b5760405162461bcd60e51b815260206004820152601860248201527f616c7265616479206265656e20696e697469616c697a656400000000000000006044820152606401610c72565b600880546001600160a01b0319166001600160a01b0392909216919091179055619d8060045561335460035569d3c21bcecceda10000006005556202a300600c5560006007556a084595161401484a00000060019081556a21165458500521280000006002556119aa600f5561a8c06010556a01a784379d99db420000006013556146506014556301e133806015556012805460ff19169091179055565b6004611fe482611516565b6007811115611ff557611ff5613671565b146120425760405162461bcd60e51b815260206004820152601f60248201527f63616e206f6e6c792062652071756575656420696620737563636565646564006044820152606401610c72565b6000818152600960205260408120600e8101549091906120629042613af3565b905060005b600383015481101561234d57600b600084600301838154811061208c5761208c613c47565b6000918252602090912001546004860180546001600160a01b0390921691859081106120ba576120ba613c47565b90600052602060002001548660050185815481106120da576120da613c47565b906000526020600020018760060186815481106120f9576120f9613c47565b9060005260206000200187604051602001612118959493929190613d62565b60408051601f198184030181529181528151602092830120835290820192909252016000205460ff161561218e5760405162461bcd60e51b815260206004820152601760248201527f70726f706f73616c20616c7265616479207175657565640000000000000000006044820152606401610c72565b61233a8360030182815481106121a6576121a6613c47565b6000918252602090912001546004850180546001600160a01b0390921691849081106121d4576121d4613c47565b90600052602060002001548560050184815481106121f4576121f4613c47565b90600052602060002001805461220990613bcc565b80601f016020809104026020016040519081016040528092919081815260200182805461223590613bcc565b80156122825780601f1061225757610100808354040283529160200191612282565b820191906000526020600020905b81548152906001019060200180831161226557829003601f168201915b505050505086600601858154811061229c5761229c613c47565b9060005260206000200180546122b190613bcc565b80601f01602080910402602001604051908101604052809291908181526020018280546122dd90613bcc565b801561232a5780601f106122ff5761010080835404028352916020019161232a565b820191906000526020600020905b81548152906001019060200180831161230d57829003601f168201915b50505050508688600e0154612fac565b508061234581613c5d565b915050612067565b506002820181905560405181815283907f9a2e42fd6722813d69113e7d0079d3d940171428df7373df9c7f7617cfda28929060200160405180910390a2505050565b3330146123ae5760405162461bcd60e51b8152600401610c7290613b95565b600280549082905560408051828152602081018490527fc2adf06da6765dba7faaccde4c0ce3f91c35dd3390e7f0b6bc2844202c9fa9529101610eba565b6000600154600014156124365760405162461bcd60e51b8152602060048201526012602482015271436861726c6965206e6f742061637469766560701b6044820152606401610c72565b6005546008546001600160a01b031663782d6fe133612456600143613c07565b6040516001600160e01b031960e085901b1681526001600160a01b039092166004830152602482015260440160206040518083038186803b15801561249a57600080fd5b505afa1580156124ae573d6000803e3d6000fd5b505050506040513d601f19601f820116820180604052508101906124d29190613c1e565b6001600160601b03161015806124ec57506124ec3361069c565b6125385760405162461bcd60e51b815260206004820152601e60248201527f766f7465732062656c6f772070726f706f73616c207468726573686f6c6400006044820152606401610c72565b8551875114801561254a575084518751145b8015612557575083518751145b6125a35760405162461bcd60e51b815260206004820152601a60248201527f696e666f726d6174696f6e206172697479206d69736d617463680000000000006044820152606401610c72565b86516125e85760405162461bcd60e51b81526020600482015260146024820152736d7573742070726f7669646520616374696f6e7360601b6044820152606401610c72565b600a8751111561262d5760405162461bcd60e51b815260206004820152601060248201526f746f6f206d616e7920616374696f6e7360801b6044820152606401610c72565b336000908152600a6020526040902054801561271657600061264e82611516565b9050600181600781111561266457612664613671565b14156126b25760405162461bcd60e51b815260206004820152601e60248201527f6f6e65206c6976652070726f706f73616c207065722070726f706f73657200006044820152606401610c72565b60008160078111156126c6576126c6613671565b14156127145760405162461bcd60e51b815260206004820152601e60248201527f6f6e65206c6976652070726f706f73616c207065722070726f706f73657200006044820152606401610c72565b505b6007805490600061272683613c5d565b9190505550600060405180610220016040528060075481526020016127483390565b6001600160a01b03168152602001600081526020018a8152602001898152602001888152602001878152602001600354436127839190613af3565b8152602001600454600354436127999190613af3565b6127a39190613af3565b815260200160008152602001600081526020016000815260200160001515815260200160001515815260200185151581526020016001548152602001600c5481525090508380156127fa57506127f83361069c565b155b1561282c574360e08201819052600f5461281391613af3565b6101008201526002546101e08201526010546102008201525b6128353361069c565b15612876576013546101e08201526014546128509043613af3565b60e08201526004546014546128659043613af3565b61286f9190613af3565b6101008201525b805160009081526009602090815260409182902083518155818401516001820180546001600160a01b0319166001600160a01b03909216919091179055918301516002830155606083015180518493926128d792600385019291019061309d565b50608082015180516128f3916004840191602090910190613102565b5060a0820151805161290f91600584019160209091019061313d565b5060c0820151805161292b916006840191602090910190613196565b5060e08281015160078301556101008084015160088401556101208401516009840155610140840151600a80850191909155610160850151600b850155610180850151600c850180546101a08801516101c089015161ffff1990921693151561ff0019169390931792151585029290921762ff0000191662010000921515929092029190911790556101e0850151600d85015561020090940151600e9093019290925583516020808601516001600160a01b0316600090815294905260409384902055830151835191840151925190923392917f7d84a6263ae0d98d3329bd7b46bb4e8d6f98cd35a7adb45c274c8b7fd5ebd5e091612a33918f918f918f918f918f90613d9b565b60405180910390a45198975050505050505050565b333014612a675760405162461bcd60e51b8152600401610c7290613b95565b600c80549082905560408051828152602081018490527fed0229422af39d4d7d33f7a27d31d6f5cb20ec628293da58dd6e8a528ed466be9101610eba565b6005612ab082611516565b6007811115612ac157612ac1613671565b14612b0e5760405162461bcd60e51b815260206004820152601c60248201527f63616e206f6e6c792062652065786563276420696620717565756564000000006044820152606401610c72565b6000818152600960205260408120600c8101805461ff001916610100179055905b6003820154811015612c6057306001600160a01b0316630825f38f836004018381548110612b5f57612b5f613c47565b9060005260206000200154846003018481548110612b7f57612b7f613c47565b6000918252602090912001546004860180546001600160a01b039092169186908110612bad57612bad613c47565b9060005260206000200154866005018681548110612bcd57612bcd613c47565b90600052602060002001876006018781548110612bec57612bec613c47565b9060005260206000200188600201546040518763ffffffff1660e01b8152600401612c1b959493929190613d62565b6000604051808303818588803b158015612c3457600080fd5b505af1158015612c48573d6000803e3d6000fd5b50505050508080612c5890613c5d565b915050612b2f565b5060405182907f712ae1383f79ac853f8d882153778e0260ef8f03b504e2866e0593e04d2b291f90600090a25050565b60006001612c9d84611516565b6007811115612cae57612cae613671565b14612cee5760405162461bcd60e51b815260206004820152601060248201526f1d9bdd1a5b99c81a5cc818db1bdcd95960821b6044820152606401610c72565b60028260ff161115612d365760405162461bcd60e51b8152602060048201526011602482015270696e76616c696420766f7465207479706560781b6044820152606401610c72565b6000838152600960209081526040808320601183528184206001600160a01b0389168552909252909120805460ff1615612da85760405162461bcd60e51b81526020600482015260136024820152721d9bdd195c88185b1c9958591e481d9bdd1959606a1b6044820152606401610c72565b600854600783015460405163782d6fe160e01b81526000926001600160a01b03169163782d6fe191612df2918b916004016001600160a01b03929092168252602082015260400190565b60206040518083038186803b158015612e0a57600080fd5b505afa158015612e1e573d6000803e3d6000fd5b505050506040513d601f19601f82011682018060405250810190612e429190613c1e565b905060ff8516612e6f57806001600160601b031683600a0154612e659190613af3565b600a840155612ec9565b8460ff1660011415612e9e57806001600160601b03168360090154612e949190613af3565b6009840155612ec9565b8460ff1660021415612ec957806001600160601b031683600b0154612ec39190613af3565b600b8401555b81546001600160601b03821662010000026dffffffffffffffffffffffff00001960ff88166101000261ffff199093169290921760011791909116179091559150509392505050565b60008585858585604051602001612f2d959493929190613a91565b60408051601f1981840301815282825280516020918201206000818152600b909252919020805460ff1916905591506001600160a01b0387169082907f2fffc091a501fd91bfbff27141450d3acb40fb8e6d8382b243ec7a812a3aaf8790612f9c908990899089908990613b58565b60405180910390a3505050505050565b6000612fb88242613af3565b831015612ffd5760405162461bcd60e51b815260206004820152601360248201527236bab9ba1039b0ba34b9b33c903232b630bc9760691b6044820152606401610c72565b60008787878787604051602001613018959493929190613a91565b60408051601f1981840301815282825280516020918201206000818152600b909252919020805460ff1916600117905591506001600160a01b0389169082907f76e2796dc3a81d57b0e8504b647febcbeeb5f4af818e164f11eef8131a6a763f9061308a908b908b908b908b90613b58565b60405180910390a3979650505050505050565b8280548282559060005260206000209081019282156130f2579160200282015b828111156130f257825182546001600160a01b0319166001600160a01b039091161782556020909201916001909101906130bd565b506130fe9291506131ef565b5090565b8280548282559060005260206000209081019282156130f2579160200282015b828111156130f2578251825591602001919060010190613122565b82805482825590600052602060002090810192821561318a579160200282015b8281111561318a578251805161317a918491602090910190613204565b509160200191906001019061315d565b506130fe929150613277565b8280548282559060005260206000209081019282156131e3579160200282015b828111156131e357825180516131d3918491602090910190613204565b50916020019190600101906131b6565b506130fe929150613294565b5b808211156130fe57600081556001016131f0565b82805461321090613bcc565b90600052602060002090601f01602090048101928261323257600085556130f2565b82601f1061324b57805160ff19168380011785556130f2565b828001600101855582156130f257918201828111156130f2578251825591602001919060010190613122565b808211156130fe57600061328b82826132b1565b50600101613277565b808211156130fe5760006132a882826132b1565b50600101613294565b5080546132bd90613bcc565b6000825580601f106132cd575050565b601f0160209004906000526020600020908101906132eb91906131ef565b50565b60006020828403121561330057600080fd5b5035919050565b60005b8381101561332257818101518382015260200161330a565b83811115613331576000848401525b50505050565b6000815180845261334f816020860160208601613307565b601f01601f19169290920160200192915050565b6020815260006133766020830184613337565b9392505050565b80356001600160a01b038116811461339457600080fd5b919050565b634e487b7160e01b600052604160045260246000fd5b604051601f8201601f1916810167ffffffffffffffff811182821017156133d8576133d8613399565b604052919050565b600082601f8301126133f157600080fd5b813567ffffffffffffffff81111561340b5761340b613399565b61341e601f8201601f19166020016133af565b81815284602083860101111561343357600080fd5b816020850160208301376000918101602001919091529392505050565b600080600080600060a0868803121561346857600080fd5b6134718661337d565b945060208601359350604086013567ffffffffffffffff8082111561349557600080fd5b6134a189838a016133e0565b945060608801359150808211156134b757600080fd5b506134c4888289016133e0565b95989497509295608001359392505050565b6000602082840312156134e857600080fd5b6133768261337d565b600081518084526020808501945080840160005b8381101561352a5781516001600160a01b031687529582019590820190600101613505565b509495945050505050565b600081518084526020808501945080840160005b8381101561352a57815187529582019590820190600101613549565b600081518084526020808501808196508360051b8101915082860160005b858110156135ad57828403895261359b848351613337565b98850198935090840190600101613583565b5091979650505050505050565b6080815260006135cd60808301876134f1565b82810360208401526135df8187613535565b905082810360408401526135f38186613565565b905082810360608401526136078185613565565b979650505050505050565b803560ff8116811461339457600080fd5b600080600080600060a0868803121561363b57600080fd5b8535945061364b60208701613612565b935061365960408701613612565b94979396509394606081013594506080013592915050565b634e487b7160e01b600052602160045260246000fd5b60208101600883106136a957634e487b7160e01b600052602160045260246000fd5b91905290565b600080604083850312156136c257600080fd5b6136cb8361337d565b946020939093013593505050565b600080604083850312156136ec57600080fd5b823591506136fc60208401613612565b90509250929050565b6000806040838503121561371857600080fd5b823591506136fc6020840161337d565b6000806000806060858703121561373e57600080fd5b8435935061374e60208601613612565b9250604085013567ffffffffffffffff8082111561376b57600080fd5b818701915087601f83011261377f57600080fd5b81358181111561378e57600080fd5b8860208285010111156137a057600080fd5b95989497505060200194505050565b600067ffffffffffffffff8211156137c9576137c9613399565b5060051b60200190565b600082601f8301126137e457600080fd5b813560206137f96137f4836137af565b6133af565b82815260059290921b8401810191818101908684111561381857600080fd5b8286015b8481101561383a5761382d8161337d565b835291830191830161381c565b509695505050505050565b600082601f83011261385657600080fd5b813560206138666137f4836137af565b82815260059290921b8401810191818101908684111561388557600080fd5b8286015b8481101561383a5780358352918301918301613889565b600082601f8301126138b157600080fd5b813560206138c16137f4836137af565b82815260059290921b840181019181810190868411156138e057600080fd5b8286015b8481101561383a57803567ffffffffffffffff8111156139045760008081fd5b6139128986838b01016133e0565b8452509183019183016138e4565b600082601f83011261393157600080fd5b813560206139416137f4836137af565b82815260059290921b8401810191818101908684111561396057600080fd5b8286015b8481101561383a57803567ffffffffffffffff8111156139845760008081fd5b6139928986838b01016133e0565b845250918301918301613964565b8035801515811461339457600080fd5b60008060008060008060c087890312156139c957600080fd5b863567ffffffffffffffff808211156139e157600080fd5b6139ed8a838b016137d3565b97506020890135915080821115613a0357600080fd5b613a0f8a838b01613845565b96506040890135915080821115613a2557600080fd5b613a318a838b016138a0565b95506060890135915080821115613a4757600080fd5b613a538a838b01613920565b94506080890135915080821115613a6957600080fd5b50613a7689828a016133e0565b925050613a8560a088016139a0565b90509295509295509295565b60018060a01b038616815284602082015260a060408201526000613ab860a0830186613337565b8281036060840152613aca8186613337565b9150508260808301529695505050505050565b634e487b7160e01b600052601160045260246000fd5b60008219821115613b0657613b06613add565b500190565b6001600160e01b0319831681528151600090613b2e816004850160208701613307565b919091016004019392505050565b60008251613b4e818460208701613307565b9190910192915050565b848152608060208201526000613b716080830186613337565b8281036040840152613b838186613337565b91505082606083015295945050505050565b60208082526017908201527f6d75737420636f6d652066726f6d2074686520676f762e000000000000000000604082015260600190565b600181811c90821680613be057607f821691505b60208210811415613c0157634e487b7160e01b600052602260045260246000fd5b50919050565b600082821015613c1957613c19613add565b500390565b600060208284031215613c3057600080fd5b81516001600160601b038116811461337657600080fd5b634e487b7160e01b600052603260045260246000fd5b6000600019821415613c7157613c71613add565b5060010190565b60ff851681526001600160601b038416602082015260606040820152816060820152818360808301376000818301608090810191909152601f909201601f191601019392505050565b8054600090600181811c9080831680613cdb57607f831692505b6020808410821415613cfd57634e487b7160e01b600052602260045260246000fd5b838852818015613d145760018114613d2857613d56565b60ff19861689830152604089019650613d56565b876000528160002060005b86811015613d4e5781548b8201850152908501908301613d33565b8a0183019750505b50505050505092915050565b60018060a01b038616815284602082015260a060408201526000613d8960a0830186613cc1565b8281036060840152613aca8186613cc1565b60c081526000613dae60c08301896134f1565b8281036020840152613dc08189613535565b90508281036040840152613dd48188613565565b90508281036060840152613de88187613565565b905084608084015282810360a0840152613e028185613337565b999850505050505050505056fe",
        ),
        (
            "0xDb53f47aC61FE54F456A4eb3E09832D08Dd7BEec",
            "PoolExercise",
            "6101c06040523480156200001257600080fd5b50604051620030713803806200307183398101604081905262000035916200016a565b6001600160a01b038681166101005285811660805284811660a05283811660c052821660e052600f81900b61012052858585858585620000846000808062000101602090811b6200011917901c565b6101408181525050620000a660016000806200010160201b620001191760201c565b6101608181525050620000c860026000806200010160201b620001191760201c565b6101808181525050620000ea60036000806200010160201b620001191760201c565b6101a05250620002319a5050505050505050505050565b600081600f0b6080846001600160401b0316901b60f88660078111156200012c576200012c620001f4565b6200013992911b6200020a565b6200014591906200020a565b949350505050565b80516001600160a01b03811681146200016557600080fd5b919050565b60008060008060008060c087890312156200018457600080fd5b6200018f876200014d565b95506200019f602088016200014d565b9450620001af604088016200014d565b9350620001bf606088016200014d565b9250620001cf608088016200014d565b915060a087015180600f0b8114620001e657600080fd5b809150509295509295509295565b634e487b7160e01b600052602160045260246000fd5b600082198211156200022c57634e487b7160e01b600052601160045260246000fd5b500190565b60805160a05160c05160e05161010051610120516101405161016051610180516101a051612d6f6200030260003960008181610e1c015261137b015260008181610e420152818161135101526113f4015260008181611327015281816114b801526122c90152600081816112fe015281816113cb0152818161148f015281816114e101526122ef0152600081816103a8015281816107ed0152610cda0152600050506000818161176601526117b201526000610485015260008181611964015261212c015260005050612d6f6000f3fe608060405234801561001057600080fd5b50600436106100365760003560e01c8063477130981461003b578063b50e7ee314610050575b600080fd5b61004e610049366004612986565b610063565b005b61004e61005e3660046129c7565b610109565b336001600160a01b038416146100f9576001600160a01b03831660009081527f1799cf914cb0cb442ca7c7ac709ee40d0cb89e87351dc08d517fbda27d50c68c6020908152604080832033845290915290205460ff166100f95760405162461bcd60e51b815260206004820152600c60248201526b1b9bdd08185c1c1c9bdd995960a21b60448201526064015b60405180910390fd5b61010483838361015f565b505050565b6101156000838361015f565b5050565b600081600f0b60808467ffffffffffffffff16901b60f8866007811115610142576101426129e9565b61014d92911b612a15565b6101579190612a15565b949350505050565b608082901c8260006001600160a01b0386161560f883901c600481600781111561018b5761018b6129e9565b14806101a8575060068160078111156101a6576101a66129e9565b145b6101e35760405162461bcd60e51b815260206004820152600c60248201526b696e76616c6964207479706560a01b60448201526064016100f0565b8115806101f95750428567ffffffffffffffff16105b6102335760405162461bcd60e51b815260206004820152600b60248201526a1b9bdd08195e1c1a5c995960aa1b60448201526064016100f0565b6004816007811115610247576102476129e9565b149250506000610262600080516020612d1a83398151915290565b9050600061026f826104c0565b9050428667ffffffffffffffff16101561029a576102978267ffffffffffffffff8816610526565b90505b82806102be5750836102b45784600f0b81600f0b126102be565b84600f0b81600f0b135b6102f45760405162461bcd60e51b81526020600482015260076024820152666e6f742049544d60c81b60448201526064016100f0565b6000841561033a5785600f0b82600f0b1315610335576103328861032984610320600f82900b8b61061e565b600f0b90610659565b600f0b906106b1565b90505b610367565b85600f0b82600f0b12156103675761036461035d89610329600f8a900b8661061e565b8490610719565b90505b6000841561038c5761037b89838c89610754565b6103859082612a15565b9050610454565b6103978b8b8b6108cb565b600082156103ff576103d58c6103d07f0000000000000000000000000000000000000000000000000000000000000000600f0b866106b1565b610a5d565b90506103e18183612a15565b91506103ff8c6103f089610a8c565b6103fa8487612a2d565b610ae1565b604080518c8152602081018c9052908101849052606081018290526001600160a01b038d16907f31939b125e073bbdbf69ac6eb0cb59489894a9bea509d658589af5917b53cca19060800160405180910390a2505b610474898361046e6104678a6000610bb1565b8c8c610119565b89610be4565b61047e9082612a15565b90506104b37f00000000000000000000000000000000000000000000000000000000000000006104ad88610e13565b83610e67565b5050505050505050505050565b60004282600c015414156104de576104d88242610e82565b92915050565b6104e782610eb0565b90506104f38242610e82565b600f0b61050557610505824283610fd3565b42600c830155610516826001611051565b610521826000611051565b919050565b600080610535610e1084612a5a565b600881901c6000818152601287016020526040812054929350909160ff84169190821b821c90610568620e100042612a5a565b90505b811580156105795750808411155b156105a65760128801600061058d86612a7c565b955085815260200190815260200160002054915061056b565b600060805b80156105d25783811c156105ca576105c38183612a15565b93811c9391505b60011c6105ab565b5060118901600060018360086105e88a84612a15565b6105f392911b612a2d565b6105fd9190612a2d565b8152602081019190915260400160002054600f0b9998505050505050505050565b6000600f82810b9084900b0360016001607f1b03198112801590610649575060016001607f1b038113155b61065257600080fd5b9392505050565b600081600f0b6000141561066c57600080fd5b600082600f0b604085600f0b901b8161068757610687612a44565b05905060016001607f1b03198112801590610649575060016001607f1b0381131561065257600080fd5b6000816106c0575060006104d8565b600083600f0b12156106d157600080fd5b600f83900b6001600160801b038316810260401c90608084901c026001600160c01b0381111561070057600080fd5b60401b811981111561071157600080fd5b019392505050565b600080610737838560030160149054906101000a900460ff166110e1565b9050610157818560030160159054906101000a900460ff166110f7565b60008281527fb31c2c74f86ca3ce94d901f5f5bbe66f7161eec2f7b5aa0b75a86371436424eb602052604081205b85156108c25760006107a9600161079884611112565b6107a29190612a2d565b839061111c565b905060006107b78287611128565b9050878111156107c45750865b600080881561084657896107d8848b612a97565b6107e29190612a5a565b9150610815846103d07f0000000000000000000000000000000000000000000000000000000000000000600f0b856106b1565b90506108218187612a15565b955061082d828a612a2d565b98506108468461083c89610a8c565b6103fa8486612a2d565b610850838b612a2d565b99506001600160a01b0384167f31939b125e073bbdbf69ac6eb0cb59489894a9bea509d658589af5917b53cca189856108898587612a2d565b604080519384526020840192909252908201526060810184905260800160405180910390a26108b98489856108cb565b50505050610782565b50949350505050565b6001600160a01b03831661092d5760405162461bcd60e51b815260206004820152602360248201527f455243313135353a206275726e2066726f6d20746865207a65726f206164647260448201526265737360e81b60648201526084016100f0565b61095b3384600061093d866111db565b610946866111db565b60405180602001604052806000815250611226565b60008281527f1799cf914cb0cb442ca7c7ac709ee40d0cb89e87351dc08d517fbda27d50c68b602090815260408083206001600160a01b038716845291829052909120548211156109fc5760405162461bcd60e51b815260206004820152602560248201527f455243313135353a206275726e20616d6f756e7420657863656564732062616c604482015264616e63657360d81b60648201526084016100f0565b6001600160a01b03841660008181526020838152604080832080548790039055805187815291820186905291929133917fc3d58168c5ae7397731d063d5bbf3d657854427343f4c083240f7aacaa2d0f62910160405180910390a450505050565b600080610a6984611762565b9050612710610a788285612a97565b610a829190612a5a565b6101579084612a2d565b600081610ab157600080516020612d1a833981519152546001600160a01b03166104d8565b50507fbbd6af8edd89d04327b00c29df7f272b9b1ae01bf6d9c54a784f935706df52ec546001600160a01b031690565b80610aeb57505050565b60405163a9059cbb60e01b81526001600160a01b0384811660048301526024820183905283169063a9059cbb90604401602060405180830381600087803b158015610b3557600080fd5b505af1158015610b49573d6000803e3d6000fd5b505050506040513d601f19601f82011682018060405250810190610b6d9190612ab6565b6101045760405162461bcd60e51b8152602060048201526015602482015274115490cc8c081d1c985b9cd9995c8819985a5b1959605a1b60448201526064016100f0565b60008215610bcf5781610bc5576005610bc8565b60045b90506104d8565b81610bdb576007610652565b60069392505050565b60008281527fb31c2c74f86ca3ce94d901f5f5bbe66f7161eec2f7b5aa0b75a86371436424eb60205260408120835b8615610e09576000610c3a6001610c2985611112565b610c339190612a2d565b849061111c565b90506000610c488288611128565b905088811115610c555750875b600089610c62838b612a97565b610c6c9190612a5a565b9050610c78818a612a2d565b9850610c84828b612a2d565b9950600087610cc35781610cb4610c9f600f88900b866106b1565b600080516020612d1a83398151915290610719565b610cbe9190612a2d565b610ccd565b610ccd8284612a2d565b90506000610d02856103d07f0000000000000000000000000000000000000000000000000000000000000000600f0b856106b1565b9050610d0e8189612a15565b975082610d2a600080516020612d1a833981519152878c61182c565b15610d5457610d4386610d3d8486612a2d565b8c611869565b610d4d8282612a15565b9050610d7d565b610d7086610d618c610e13565b610d6b8587612a2d565b610e67565b610d7a8382612a15565b90505b610d97600080516020612d1a833981519152878c8461192c565b610da2868c876108cb565b6001600160a01b0386167f69a2ef6bf9e7ff92cbf1b71963ba1751b1abe8f99e3b3aae2ab99e416df614938c610dd88587612a2d565b60408051928352602083019190915281018890526060810185905260800160405180910390a2505050505050610c13565b5050949350505050565b600081610e40577f00000000000000000000000000000000000000000000000000000000000000006104d8565b7f000000000000000000000000000000000000000000000000000000000000000092915050565b61010483838360405180602001604052806000815250611a6c565b60006011830181610e95610e1085612a5a565b8152602081019190915260400160002054600f0b9392505050565b6000808260030160009054906101000a90046001600160a01b03166001600160a01b03166350d25bcd6040518163ffffffff1660e01b815260040160206040518083038186803b158015610f0357600080fd5b505afa158015610f17573d6000803e3d6000fd5b505050506040513d601f19601f82011682018060405250810190610f3b9190612ad8565b905060008360020160009054906101000a90046001600160a01b03166001600160a01b03166350d25bcd6040518163ffffffff1660e01b815260040160206040518083038186803b158015610f8f57600080fd5b505afa158015610fa3573d6000803e3d6000fd5b505050506040513d601f19601f82011682018060405250810190610fc79190612ad8565b90506101578282611b93565b6000610fe1610e1084612a5a565b6000818152601186016020526040902080546001600160801b0319166001600160801b038516179055905061101a60ff80831690612a2d565b6001901b846012016000600884901c815260200190815260200160002060008282546110469190612a15565b909155505050505050565b80151560009081526013830160205260409020805415806110725750805442105b1561107c57505050565b60006110888484611c2e565b90506110c384826110bd6110b286600101546110ad898b611c9890919063ffffffff16565b6110e1565b600f86900b90611cc6565b86611cf9565b50501515600090815260139091016020526040812081815560010155565b6000610652836110f284600a612bd5565b611d76565b600061065261110783600a612bd5565b600f85900b906106b1565b60006104d8825490565b60006106528383611dad565b60006001600160a01b0383166111945760405162461bcd60e51b815260206004820152602b60248201527f455243313135353a2062616c616e636520717565727920666f7220746865207a60448201526a65726f206164647265737360a81b60648201526084016100f0565b7f1799cf914cb0cb442ca7c7ac709ee40d0cb89e87351dc08d517fbda27d50c68b6000928352602090815260408084206001600160a01b0395909516845293905250205490565b6040805160018082528183019092526060916000919060208083019080368337019050509050828160008151811061121557611215612be4565b602090810291909101015292915050565b611234868686868686611e33565b600080516020612d1a83398151915260005b845181101561175857600085828151811061126357611263612be4565b60200260200101519050600085838151811061128157611281612be4565b60200260200101519050806000141561129b575050611746565b6001600160a01b0389166112b8576112b66015850183612011565b505b6001600160a01b0388161580156112e857506000828152600080516020612cfa8339815191526020526040902054155b156112fc576112fa601585018361201d565b505b7f000000000000000000000000000000000000000000000000000000000000000082148061134957507f000000000000000000000000000000000000000000000000000000000000000082145b8061137357507f000000000000000000000000000000000000000000000000000000000000000082145b8061139d57507f000000000000000000000000000000000000000000000000000000000000000082145b1561148d576001600160a01b038916158015906113c257506001600160a01b03881615155b1561148d5760007f000000000000000000000000000000000000000000000000000000000000000083148061141657507f000000000000000000000000000000000000000000000000000000000000000083145b6001600160a01b038b166000908152600d870160209081526040808320841515845290915290205490915042906114509062015180612a15565b1061148b5760405162461bcd60e51b815260206004820152600b60248201526a1b1a5c481b1bd8dac80c5960aa1b60448201526064016100f0565b505b7f00000000000000000000000000000000000000000000000000000000000000008214806114da57507f000000000000000000000000000000000000000000000000000000000000000082145b15611682577f00000000000000000000000000000000000000000000000000000000000000008214600061150e8683612029565b90506001600160a01b038b161561163857600061152b8c86611128565b9050818111801561154557506115418285612a15565b8111155b156115dd576001600160a01b038c166000908152601488016020908152604080832086151580855260138c01845282852054855290835281842090845290915290205484906115949083612a2d565b10156115d25760405162461bcd60e51b815260206004820152600d60248201526c496e7375662062616c616e636560981b60448201526064016100f0565b6115dd878d85612043565b6001600160a01b038b161561163657611611878d858c8a8151811061160457611604612be4565b602002602001015161192c565b611636878c858c8a8151811061162957611629612be4565b60200260200101516120f4565b505b6001600160a01b038a161561167f5760006116538b86611128565b905081811115801561166d57508161166b8583612a15565b115b1561167d5761167d878c85612218565b505b50505b60f882901c826001600160a01b038b16158015906116a857506001600160a01b038a1615155b80156116e0575060058260078111156116c3576116c36129e9565b14806116e0575060078260078111156116de576116de6129e9565b145b1561174157600060058360078111156116fb576116fb6129e9565b1490506000816117225761171d611716600f85900b876106b1565b8990610719565b611724565b845b9050611732888e848461192c565b61173e888d84846120f4565b50505b505050505b8061175081612a7c565b915050611246565b5050505050505050565b60007f00000000000000000000000000000000000000000000000000000000000000006001600160a01b031615610521576040516303793c8d60e11b81526001600160a01b0383811660048301527f000000000000000000000000000000000000000000000000000000000000000016906306f2791a9060240160206040518083038186803b1580156117f457600080fd5b505afa158015611808573d6000803e3d6000fd5b505050506040513d601f19601f820116820180604052508101906104d89190612ad8565b6001600160a01b0382166000908152600e840160209081526040808320841515845290915281205480158061186057504281115b95945050505050565b600080516020612d1a83398151915261188b84611885846122c0565b85610e67565b60006101048061189b8142612a5a565b6118a59190612a97565b6118af9190612a15565b6001600160a01b03861660009081526014840160209081526040808320848452825280832087151584529091528120805492935086929091906118f3908490612a15565b90915550508215156000908152601383016020526040812060018101805491928792611920908490612a15565b90915550505550505050565b6001600160a01b03808416600090815260178601602090815260408083208615158452825280832054601889019092529091205490917f00000000000000000000000000000000000000000000000000000000000000001663edaf7d5b863087866119978982612a2d565b6040516001600160e01b031960e088901b1681526001600160a01b03958616600482015294909316602485015290151560448401526064830152608482015260a4810184905260c401600060405180830381600087803b1580156119fa57600080fd5b505af1158015611a0e573d6000803e3d6000fd5b505050508282611a1e9190612a2d565b6001600160a01b038616600090815260178801602090815260408083208815158452909152902055611a508382612a2d565b9315156000908152601890960160205250506040909320555050565b6001600160a01b038416611acc5760405162461bcd60e51b815260206004820152602160248201527f455243313135353a206d696e7420746f20746865207a65726f206164647265736044820152607360f81b60648201526084016100f0565b611aeb33600086611adc876111db565b611ae5876111db565b86611226565b60008381527f1799cf914cb0cb442ca7c7ac709ee40d0cb89e87351dc08d517fbda27d50c68b602090815260408083206001600160a01b0388168452918290528220805491928592611b3e908490612a15565b909155505060408051858152602081018590526001600160a01b0387169160009133917fc3d58168c5ae7397731d063d5bbf3d657854427343f4c083240f7aacaa2d0f62910160405180910390a45050505050565b600081611b9f57600080fd5b600080841215611bb457836000039350600190505b6000831215611bc65760009290920391155b6000611bd28585612314565b90508115611c00576001607f1b816001600160801b03161115611bf457600080fd5b60000391506104d89050565b60016001607f1b03816001600160801b03161115611c1d57600080fd5b91506104d89050565b505092915050565b600080611c4b83611c40576001611c43565b60005b600080610119565b831515600090815260138601602052604090206001015490915061015790600080516020612cfa83398151915260008481526020919091526040902054611c929190612a2d565b6110ad86865b600081611cb3576003830154600160a81b900460ff16610652565b505060030154600160a01b900460ff1690565b6000600f83810b9083900b0160016001607f1b03198112801590610649575060016001607f1b0381131561065257600080fd5b6000611d058583612476565b90506000611d16868387878761248f565b9050611d23868285612595565b60408051600f83810b825287810b602083015286900b818301529051841515917f4e23621c6f591f14bf9505cb8326b45af9dc6c5569fd608de2a7a2ddd6146b2e919081900360600190a2505050505050565b600081611d8257600080fd5b6000611d8e8484612314565b905060016001607f1b036001600160801b038216111561065257600080fd5b81546000908210611e0b5760405162461bcd60e51b815260206004820152602260248201527f456e756d657261626c655365743a20696e646578206f7574206f6620626f756e604482015261647360f01b60648201526084016100f0565b826000018281548110611e2057611e20612be4565b9060005260206000200154905092915050565b836001600160a01b0316856001600160a01b031614612009576001600160a01b0385811660009081527fb31c2c74f86ca3ce94d901f5f5bbe66f7161eec2f7b5aa0b75a86371436424ec602052604080822092871682528120600080516020612cfa833981519152927fb31c2c74f86ca3ce94d901f5f5bbe66f7161eec2f7b5aa0b75a86371436424eb929091905b87518110156104b3576000878281518110611edf57611edf612be4565b602002602001015190506000811115611ff6576000898381518110611f0657611f06612be4565b6020026020010151905060006001600160a01b03168c6001600160a01b03161415611f545760008181526020889052604081208054849290611f49908490612a15565b90915550611f8a9050565b81611f5f8d83611128565b1415611f8a576000818152602087905260409020611f7d908d6125ec565b50611f88858261201d565b505b6001600160a01b038b16611fc15760008181526020889052604081208054849290611fb6908490612a2d565b90915550611ff49050565b611fcb8b82611128565b611ff4576000818152602087905260409020611fe7908c612601565b50611ff28482612011565b505b505b508061200181612a7c565b915050611ec2565b505050505050565b60006106528383612612565b60006106528383612661565b60008161203a578260040154610652565b50506005015490565b6001600160a01b03821661205657600080fd5b8015156000908152600f8401602090815260408083206010870190925290912061208184838361274c565b61208c575050505050565b6001600160a01b0393841660008181526020838152604080832080549683528184208054978a16808652838620805499909b166001600160a01b0319998a168117909b5599855295909252822080548616909717909655528054821690558254169091555050565b6001600160a01b03808416600090815260178601602090815260408083208615158452825280832054601889019092529091205490917f00000000000000000000000000000000000000000000000000000000000000001663edaf7d5b8630878661215f8982612a15565b6040516001600160e01b031960e088901b1681526001600160a01b03958616600482015294909316602485015290151560448401526064830152608482015260a4810184905260c401600060405180830381600087803b1580156121c257600080fd5b505af11580156121d6573d6000803e3d6000fd5b5050505082826121e69190612a15565b6001600160a01b038616600090815260178801602090815260408083208815158452909152902055611a508382612a15565b6001600160a01b03821661222b57600080fd5b8015156000908152600f8401602090815260408083206010870190925290912061225684838361274c565b15612262575050505050565b60008080526020828152604080832080546001600160a01b0390811680865296845282852080546001600160a01b03199081169a909216998a1790558885529490925282208054841690941790935580528154169092179091555050565b6000816122ed577f00000000000000000000000000000000000000000000000000000000000000006104d8565b7f000000000000000000000000000000000000000000000000000000000000000092915050565b60008161232057600080fd5b60006001600160c01b03841161234b5782604085901b8161234357612343612a44565b049050612462565b60c084811c6401000000008110612364576020918201911c5b620100008110612376576010918201911c5b6101008110612387576008918201911c5b60108110612397576004918201911c5b600481106123a7576002918201911c5b600281106123b6576001820191505b60bf820360018603901c6001018260ff0387901b816123d7576123d7612a44565b0492506001600160801b038311156123ee57600080fd5b608085901c83026001600160801b038616840260c088901c604089901b8281101561241a576001820391505b608084901b92900382811015612431576001820391505b829003608084901c821461244757612447612bfa565b88818161245657612456612a44565b04870196505050505050505b6001600160801b0381111561065257600080fd5b60006124828383612798565b90506106528382846127bf565b600080826124a4576019870154600f0b6124b4565b6019870154600160801b9004600f0b5b905080600f0b600014156124cc57506008860154600f0b5b60405163e101a89b60e01b8152600f87810b600483015286810b602483015285810b604483015282900b6064820152730f6e8ef18fb5bb61d545fee60f779d8aed60408f9063e101a89b9060840160206040518083038186803b15801561253257600080fd5b505af4158015612546573d6000803e3d6000fd5b505050506040513d601f19601f8201168201806040525081019061256a9190612c10565b915067b33333333333333382600f0b121561258b5767b33333333333333391505b5095945050505050565b80156125c5576009830180546001600160801b0384166001600160801b031990911617905542600b840155505050565b6008830180546001600160801b03808516600160801b02911617905542600a840155505050565b6000610652836001600160a01b038416612661565b6000610652836001600160a01b0384165b6000818152600183016020526040812054612659575081546001818101845560008481526020808220909301849055845484825282860190935260409020919091556104d8565b5060006104d8565b60008181526001830160205260408120548015612742576000612685600183612a2d565b8554909150600090869061269b90600190612a2d565b815481106126ab576126ab612be4565b90600052602060002001549050808660000183815481106126ce576126ce612be4565b6000918252602090912001556126e5826001612a15565b6000828152600188016020526040902055855486908061270757612707612c33565b6001900381819060005260206000200160009055905585600101600086815260200190815260200160002060009055600193505050506104d8565b60009150506104d8565b6001600160a01b0383811660009081526020849052604081205490911615158061015757506000808052602083905260409020546001600160a01b039081169085161490509392505050565b6000816127b3576008830154600160801b9004600f0b610652565b505060090154600f0b90565b600080826127d15784600a01546127d7565b84600b01545b6127e19042612a2d565b905061a8c0811115612800576127f961a8c082612a2d565b9050612809565b83915050610652565b600061281782613840611d76565b9050600061282a85611c40576001611c43565b851515600090815260188901602090815260408083205460138c01835281842060010154858552600080516020612cfa83398151915290935290832054939450926128889161287891612a2d565b6128829084612a2d565b83611d76565b6040805161012081018252600f87810b82528b810b602083015283900b8183015267b333333333333333606082015267e666666666666666608082018190526801000000000000000060a0830181905260c083015260e082015268056fc2a2c515da32ea6101008201529051634916d70d60e01b8152919250730f6e8ef18fb5bb61d545fee60f779d8aed60408f91634916d70d9161292991600401612c49565b60206040518083038186803b15801561294157600080fd5b505af4158015612955573d6000803e3d6000fd5b505050506040513d601f19601f820116820180604052508101906129799190612c10565b9998505050505050505050565b60008060006060848603121561299b57600080fd5b83356001600160a01b03811681146129b257600080fd5b95602085013595506040909401359392505050565b600080604083850312156129da57600080fd5b50508035926020909101359150565b634e487b7160e01b600052602160045260246000fd5b634e487b7160e01b600052601160045260246000fd5b60008219821115612a2857612a286129ff565b500190565b600082821015612a3f57612a3f6129ff565b500390565b634e487b7160e01b600052601260045260246000fd5b600082612a7757634e487b7160e01b600052601260045260246000fd5b500490565b6000600019821415612a9057612a906129ff565b5060010190565b6000816000190483118215151615612ab157612ab16129ff565b500290565b600060208284031215612ac857600080fd5b8151801515811461065257600080fd5b600060208284031215612aea57600080fd5b5051919050565b600181815b80851115612b2c578160001904821115612b1257612b126129ff565b80851615612b1f57918102915b93841c9390800290612af6565b509250929050565b600082612b43575060016104d8565b81612b50575060006104d8565b8160018114612b665760028114612b7057612b8c565b60019150506104d8565b60ff841115612b8157612b816129ff565b50506001821b6104d8565b5060208310610133831016604e8410600b8410161715612baf575081810a6104d8565b612bb98383612af1565b8060001904821115612bcd57612bcd6129ff565b029392505050565b600061065260ff841683612b34565b634e487b7160e01b600052603260045260246000fd5b634e487b7160e01b600052600160045260246000fd5b600060208284031215612c2257600080fd5b815180600f0b811461065257600080fd5b634e487b7160e01b600052603160045260246000fd5b6000610120820190508251600f0b82526020830151600f0b60208301526040830151612c7a6040840182600f0b9052565b506060830151612c8f6060840182600f0b9052565b506080830151612ca46080840182600f0b9052565b5060a0830151612cb960a0840182600f0b9052565b5060c0830151612cce60c0840182600f0b9052565b5060e0830151612ce360e0840182600f0b9052565b5061010080840151611c2682850182600f0b905256feb31c2c74f86ca3ce94d901f5f5bbe66f7161eec2f7b5aa0b75a86371436424eabbd6af8edd89d04327b00c29df7f272b9b1ae01bf6d9c54a784f935706df52eb",
        ),
        (
            "0x71356E37e0368Bd10bFDbF41dC052fE5FA24cD05",
            "MainchainGatewayV2",
            "608060405234801561001057600080fd5b506000805460ff1916905561582e806200002b6000396000f3fe60806040526004361061032d5760003560e01c80639157921c116101a5578063b2975794116100ec578063d547741f11610095578063dafae4081161006f578063dafae4081461096e578063dff525e11461098e578063e400327c146109ae578063e75235b8146109ce5761033c565b8063d547741f14610901578063d55ed10314610921578063d64af2a61461094e5761033c565b8063cdb67444116100c6578063cdb674441461089c578063cdf64a76146108b4578063d19773d2146108d45761033c565b8063b29757941461082f578063b9c362091461085c578063ca15c8731461087c5761033c565b8063a3912ec81161014e578063affed0e011610128578063affed0e0146107cc578063b1a2567e146107e2578063b1d08a03146108025761033c565b8063a3912ec81461033a578063ab7965661461077f578063ac78dfe8146107ac5761033c565b8063994390891161017f57806399439089146107155780639dcc4da314610735578063a217fddf1461076a5761033c565b80639157921c1461068f57806391d14854146106af57806393c5678f146106f55761033c565b806336568abe116102745780635c975abb1161021d5780637de5dedd116101f75780637de5dedd146106115780638456cb59146106265780638f34e3471461063b5780639010d07c1461066f5761033c565b80635c975abb146105ac5780636932be98146105c45780636c1ce670146105f15761033c565b80634d0d66731161024e5780634d0d66731461052f5780634d493f4e1461054f57806359122f6b1461057f5761033c565b806336568abe146104e75780633f4ba83a146105075780634b14557e1461051c5761033c565b80631d4a7210116102d65780632f2ff15d116102b05780632f2ff15d1461049b578063302d12db146104bb5780633644e515146104d25761033c565b80631d4a721014610428578063248a9ca3146104555780632dfdf0b5146104855761033c565b8063180ff1e911610307578063180ff1e9146103d55780631a8e55b0146103e85780631b6e7594146104085761033c565b806301ffc9a71461034457806317ce2dd41461037957806317fcb39b1461039d5761033c565b3661033c5761033a6109e6565b005b61033a6109e6565b34801561035057600080fd5b5061036461035f366004614843565b610a69565b60405190151581526020015b60405180910390f35b34801561038557600080fd5b5061038f60755481565b604051908152602001610370565b3480156103a957600080fd5b506074546103bd906001600160a01b031681565b6040516001600160a01b039091168152602001610370565b61033a6103e33660046148f4565b610aad565b3480156103f457600080fd5b5061033a6104033660046149e6565b610dbd565b34801561041457600080fd5b5061033a610423366004614a52565b610e8f565b34801561043457600080fd5b5061038f610443366004614aec565b603e6020526000908152604090205481565b34801561046157600080fd5b5061038f610470366004614b09565b60009081526072602052604090206001015490565b34801561049157600080fd5b5061038f60765481565b3480156104a757600080fd5b5061033a6104b6366004614b22565b610f64565b3480156104c757600080fd5b5061038f620f424081565b3480156104de57600080fd5b5060775461038f565b3480156104f357600080fd5b5061033a610502366004614b22565b610f8f565b34801561051357600080fd5b5061033a61101b565b61033a61052a366004614b52565b611083565b34801561053b57600080fd5b5061036461054a366004614b7d565b6110e1565b34801561055b57600080fd5b5061036461056a366004614b09565b607a6020526000908152604090205460ff1681565b34801561058b57600080fd5b5061038f61059a366004614aec565b603a6020526000908152604090205481565b3480156105b857600080fd5b5060005460ff16610364565b3480156105d057600080fd5b5061038f6105df366004614b09565b60796020526000908152604090205481565b3480156105fd57600080fd5b5061036461060c366004614c06565b61118c565b34801561061d57600080fd5b5061038f61119f565b34801561063257600080fd5b5061033a611234565b34801561064757600080fd5b5061038f7f5e5712e902fff5e704bc4d506ad976718319e019e9d2a872528a01a85db433e481565b34801561067b57600080fd5b506103bd61068a366004614c32565b61129c565b34801561069b57600080fd5b5061033a6106aa366004614c54565b6112b4565b3480156106bb57600080fd5b506103646106ca366004614b22565b60009182526072602090815260408084206001600160a01b0393909316845291905290205460ff1690565b34801561070157600080fd5b5061033a6107103660046149e6565b6115ca565b34801561072157600080fd5b506003546103bd906001600160a01b031681565b34801561074157600080fd5b50610755610750366004614c32565b611696565b60408051928352602083019190915201610370565b34801561077657600080fd5b5061038f600081565b34801561078b57600080fd5b5061038f61079a366004614aec565b603c6020526000908152604090205481565b3480156107b857600080fd5b506103646107c7366004614b09565b61172f565b3480156107d857600080fd5b5061038f60045481565b3480156107ee57600080fd5b5061033a6107fd3660046149e6565b6117ce565b34801561080e57600080fd5b5061038f61081d366004614aec565b60396020526000908152604090205481565b34801561083b57600080fd5b5061084f61084a366004614aec565b61189a565b6040516103709190614ca5565b34801561086857600080fd5b50610755610877366004614c32565b611992565b34801561088857600080fd5b5061038f610897366004614b09565b611a17565b3480156108a857600080fd5b50603754603854610755565b3480156108c057600080fd5b5061033a6108cf366004614aec565b611a2e565b3480156108e057600080fd5b5061038f6108ef366004614aec565b603b6020526000908152604090205481565b34801561090d57600080fd5b5061033a61091c366004614b22565b611a97565b34801561092d57600080fd5b5061038f61093c366004614aec565b603d6020526000908152604090205481565b34801561095a57600080fd5b5061033a610969366004614aec565b611abd565b34801561097a57600080fd5b50610364610989366004614b09565b611b26565b34801561099a57600080fd5b5061033a6109a9366004614cd2565b611bbd565b3480156109ba57600080fd5b5061033a6109c93660046149e6565b611cc7565b3480156109da57600080fd5b50600154600254610755565b60005460ff1615610a315760405162461bcd60e51b815260206004820152601060248201526f14185d5cd8589b194e881c185d5cd95960821b60448201526064015b60405180910390fd5b6074546001600160a01b03163314610a6757610a4b614802565b338152604080820151349101528051610a65908290611d93565b505b565b60006001600160e01b031982167f5a05180f000000000000000000000000000000000000000000000000000000001480610aa75750610aa78261210a565b92915050565b607154610100900460ff16610ac85760715460ff1615610acc565b303b155b610b3e5760405162461bcd60e51b815260206004820152602e60248201527f496e697469616c697a61626c653a20636f6e747261637420697320616c72656160448201527f647920696e697469616c697a65640000000000000000000000000000000000006064820152608401610a28565b607154610100900460ff16158015610b60576071805461ffff19166101011790555b610b6b60008d612171565b6075899055610b798b61217b565b610b828a6121dd565b610c29604080517f8b73c3c69bb8fe3d512ecc4cf759cc79239f7b179b0ffacaa9a75d522b39400f60208201527f159f52c1e3a2b6a6aad3950adf713516211484e0516dad685ea662a094b7c43b918101919091527fad7c5bef027816a800da1736444fb58a807ef4c9603b7848673f7e3a68eb14a560608201524660808201523060a082015260c00160408051601f198184030181529190528051602090910120607755565b610c338887612238565b5050610c3f87876122f8565b5050610c496123d3565b6000610c558680614da6565b90501115610d1657610c7e610c6a8680614da6565b610c776020890189614da6565b8787612467565b610ca4610c8b8680614da6565b8660005b602002810190610c9f9190614da6565b612666565b610cca610cb18680614da6565b8660015b602002810190610cc59190614da6565b612779565b610cf0610cd78680614da6565b8660025b602002810190610ceb9190614da6565b61288c565b610d16610cfd8680614da6565b8660035b602002810190610d119190614da6565b612a30565b60005b610d266040870187614da6565b9050811015610d9c57610d8a7f5e5712e902fff5e704bc4d506ad976718319e019e9d2a872528a01a85db433e4610d606040890189614da6565b84818110610d7057610d70614d90565b9050602002016020810190610d859190614aec565b612b43565b80610d9481614e06565b915050610d19565b508015610daf576071805461ff00191690555b505050505050505050505050565b6000805160206157b9833981519152546001600160a01b03163314610e1d5760405162461bcd60e51b815260206004820152602260248201526000805160206157d983398151915260448201526132b960f11b6064820152608401610a28565b82610e7d5760405162461bcd60e51b815260206004820152602a60248201527f5769746864726177616c4c696d69746174696f6e3a20696e76616c69642061726044820152690e4c2f240d8cadccee8d60b31b6064820152608401610a28565b610e8984848484612779565b50505050565b6000805160206157b9833981519152546001600160a01b03163314610eef5760405162461bcd60e51b815260206004820152602260248201526000805160206157d983398151915260448201526132b960f11b6064820152608401610a28565b84610f4e5760405162461bcd60e51b815260206004820152602960248201527f4d61696e636861696e4761746577617956323a20717565727920666f7220656d60448201526870747920617272617960b81b6064820152608401610a28565b610f5c868686868686612467565b505050505050565b600082815260726020526040902060010154610f808133612b65565b610f8a8383612b43565b505050565b6001600160a01b038116331461100d5760405162461bcd60e51b815260206004820152602f60248201527f416363657373436f6e74726f6c3a2063616e206f6e6c792072656e6f756e636560448201527f20726f6c657320666f722073656c6600000000000000000000000000000000006064820152608401610a28565b6110178282612be5565b5050565b6000805160206157b9833981519152546001600160a01b0316331461107b5760405162461bcd60e51b815260206004820152602260248201526000805160206157d983398151915260448201526132b960f11b6064820152608401610a28565b610a67612c07565b60005460ff16156110c95760405162461bcd60e51b815260206004820152601060248201526f14185d5cd8589b194e881c185d5cd95960821b6044820152606401610a28565b610a656110db36839003830183614ec0565b33611d93565b6000805460ff16156111285760405162461bcd60e51b815260206004820152601060248201526f14185d5cd8589b194e881c185d5cd95960821b6044820152606401610a28565b611184848484808060200260200160405190810160405280939291908181526020016000905b8282101561117a5761116b60608302860136819003810190614f13565b8152602001906001019061114e565b5050505050612ca3565b949350505050565b600061119883836133bc565b9392505050565b600061122f600360009054906101000a90046001600160a01b03166001600160a01b031663926323d56040518163ffffffff1660e01b815260040160206040518083038186803b1580156111f257600080fd5b505afa158015611206573d6000803e3d6000fd5b505050506040513d601f19601f8201168201806040525081019061122a9190614f89565b613480565b905090565b6000805160206157b9833981519152546001600160a01b031633146112945760405162461bcd60e51b815260206004820152602260248201526000805160206157d983398151915260448201526132b960f11b6064820152608401610a28565b610a676134b6565b60008281526073602052604081206111989083613531565b7f5e5712e902fff5e704bc4d506ad976718319e019e9d2a872528a01a85db433e46112df8133612b65565b60006112f86112f336859003850185614ff0565b61353d565b905061130c6112f336859003850185614ff0565b8335600090815260796020526040902054146113765760405162461bcd60e51b815260206004820152602360248201527f4d61696e636861696e4761746577617956323a20696e76616c696420726563656044820152621a5c1d60ea1b6064820152608401610a28565b82356000908152607a602052604090205460ff166113fc5760405162461bcd60e51b815260206004820152603160248201527f4d61696e636861696e4761746577617956323a20717565727920666f7220617060448201527f70726f766564207769746864726177616c0000000000000000000000000000006064820152608401610a28565b82356000908152607a602052604090819020805460ff19169055517fd639511b37b3b002cca6cfe6bca0d833945a5af5a045578a0627fc43b79b26309061144690839086906150c4565b60405180910390a160006114606080850160608601614aec565b9050600061147661012086016101008701615151565b600181111561148757611487614c71565b141561154f5760006114a2368690038601610100870161516e565b6001600160a01b0383166000908152603b60205260409020549091506114ce90610140870135906135c6565b604082015260006114e8368790038701610100880161516e565b60408301519091506114ff9061014088013561518a565b604082015260745461151f908390339086906001600160a01b03166135e0565b6115486115326060880160408901614aec565b60745483919086906001600160a01b03166135e0565b505061158b565b61158b6115626060860160408701614aec565b60745483906001600160a01b03166115833689900389016101008a0161516e565b9291906135e0565b7f21e88e956aa3e086f6388e899965cef814688f99ad8bb29b08d396571016372d82856040516115bc9291906150c4565b60405180910390a150505050565b6000805160206157b9833981519152546001600160a01b0316331461162a5760405162461bcd60e51b815260206004820152602260248201526000805160206157d983398151915260448201526132b960f11b6064820152608401610a28565b8261168a5760405162461bcd60e51b815260206004820152602a60248201527f5769746864726177616c4c696d69746174696f6e3a20696e76616c69642061726044820152690e4c2f240d8cadccee8d60b31b6064820152608401610a28565b610e8984848484612666565b6000806116b86000805160206157b9833981519152546001600160a01b031690565b6001600160a01b0316336001600160a01b0316146117115760405162461bcd60e51b815260206004820152602260248201526000805160206157d983398151915260448201526132b960f11b6064820152608401610a28565b61171b84846122f8565b90925090506117286123d3565b9250929050565b6003546040805163926323d560e01b815290516000926001600160a01b03169163926323d5916004808301926020929190829003018186803b15801561177457600080fd5b505afa158015611788573d6000803e3d6000fd5b505050506040513d601f19601f820116820180604052508101906117ac9190614f89565b6037546117b991906151a1565b6038546117c690846151a1565b101592915050565b6000805160206157b9833981519152546001600160a01b0316331461182e5760405162461bcd60e51b815260206004820152602260248201526000805160206157d983398151915260448201526132b960f11b6064820152608401610a28565b8261188e5760405162461bcd60e51b815260206004820152602a60248201527f5769746864726177616c4c696d69746174696f6e3a20696e76616c69642061726044820152690e4c2f240d8cadccee8d60b31b6064820152608401610a28565b610e898484848461288c565b60408051808201909152600080825260208201526001600160a01b0382166000908152607860205260409081902081518083019092528054829060ff1660018111156118e8576118e8614c71565b60018111156118f9576118f9614c71565b815290546001600160a01b036101009091048116602092830152908201519192501661198d5760405162461bcd60e51b815260206004820152602560248201527f4d61696e636861696e4761746577617956323a20756e737570706f727465642060448201527f746f6b656e0000000000000000000000000000000000000000000000000000006064820152608401610a28565b919050565b6000806119b46000805160206157b9833981519152546001600160a01b031690565b6001600160a01b0316336001600160a01b031614611a0d5760405162461bcd60e51b815260206004820152602260248201526000805160206157d983398151915260448201526132b960f11b6064820152608401610a28565b61171b8484612238565b6000818152607360205260408120610aa790613a13565b6000805160206157b9833981519152546001600160a01b03163314611a8e5760405162461bcd60e51b815260206004820152602260248201526000805160206157d983398151915260448201526132b960f11b6064820152608401610a28565b610a65816121dd565b600082815260726020526040902060010154611ab38133612b65565b610f8a8383612be5565b6000805160206157b9833981519152546001600160a01b03163314611b1d5760405162461bcd60e51b815260206004820152602260248201526000805160206157d983398151915260448201526132b960f11b6064820152608401610a28565b610a658161217b565b6003546040805163926323d560e01b815290516000926001600160a01b03169163926323d5916004808301926020929190829003018186803b158015611b6b57600080fd5b505afa158015611b7f573d6000803e3d6000fd5b505050506040513d601f19601f82011682018060405250810190611ba39190614f89565b600154611bb091906151a1565b6002546117c690846151a1565b6000805160206157b9833981519152546001600160a01b03163314611c1d5760405162461bcd60e51b815260206004820152602260248201526000805160206157d983398151915260448201526132b960f11b6064820152608401610a28565b85611c7c5760405162461bcd60e51b815260206004820152602960248201527f4d61696e636861696e4761746577617956323a20717565727920666f7220656d60448201526870747920617272617960b81b6064820152608401610a28565b611c8a878787878787612467565b611c978787836000610c8f565b611ca48787836001610cb5565b611cb18787836002610cdb565b611cbe8787836003610d01565b50505050505050565b6000805160206157b9833981519152546001600160a01b03163314611d275760405162461bcd60e51b815260206004820152602260248201526000805160206157d983398151915260448201526132b960f11b6064820152608401610a28565b82611d875760405162461bcd60e51b815260206004820152602a60248201527f5769746864726177616c4c696d69746174696f6e3a20696e76616c69642061726044820152690e4c2f240d8cadccee8d60b31b6064820152608401610a28565b610e8984848484612a30565b604080518082018252600080825260208201526074549184015190916001600160a01b031690611dc290613a1d565b60208401516001600160a01b0316611ee1573484604001516040015114611e375760405162461bcd60e51b815260206004820152602360248201527f4d61696e636861696e4761746577617956323a20696e76616c69642072657175604482015262195cdd60ea1b6064820152608401610a28565b611e408161189a565b6040850151519092506001811115611e5a57611e5a614c71565b82516001811115611e6d57611e6d614c71565b14611ecd5760405162461bcd60e51b815260206004820152602a60248201527f4d61696e636861696e4761746577617956323a20696e76616c696420746f6b656044820152691b881cdd185b99185c9960b21b6064820152608401610a28565b6001600160a01b0381166020850152612087565b3415611f3b5760405162461bcd60e51b815260206004820152602360248201527f4d61696e636861696e4761746577617956323a20696e76616c69642072657175604482015262195cdd60ea1b6064820152608401610a28565b611f48846020015161189a565b6040850151519092506001811115611f6257611f62614c71565b82516001811115611f7557611f75614c71565b14611fd55760405162461bcd60e51b815260206004820152602a60248201527f4d61696e636861696e4761746577617956323a20696e76616c696420746f6b656044820152691b881cdd185b99185c9960b21b6064820152608401610a28565b60208401516040850151611fec9185903090613ac7565b83602001516001600160a01b0316816001600160a01b031614156120875760408481015181015190517f2e1a7d4d00000000000000000000000000000000000000000000000000000000815260048101919091526001600160a01b03821690632e1a7d4d90602401600060405180830381600087803b15801561206e57600080fd5b505af1158015612082573d6000803e3d6000fd5b505050505b607680546000918261209883614e06565b91905055905060006120bf858386602001516075548a613ce190949392919063ffffffff16565b90507fd7b25068d9dc8d00765254cfb7f5070f98d263c8d68931d937c7362fa738048b6120eb8261353d565b826040516120fa9291906151c0565b60405180910390a1505050505050565b60006001600160e01b031982167f7965db0b000000000000000000000000000000000000000000000000000000001480610aa757507f01ffc9a7000000000000000000000000000000000000000000000000000000006001600160e01b0319831614610aa7565b6110178282612b43565b6074805473ffffffffffffffffffffffffffffffffffffffff19166001600160a01b0383169081179091556040519081527f9d2334c23be647e994f27a72c5eee42a43d5bdcfe15bb88e939103c2b114cbaf906020015b60405180910390a150565b6003805473ffffffffffffffffffffffffffffffffffffffff19166001600160a01b0383169081179091556040519081527fef40dc07567635f84f5edbd2f8dbc16b40d9d282dd8e7e6f4ff58236b6836169906020016121d2565b6000808284111561228b5760405162461bcd60e51b815260206004820152601c60248201527f4761746577617956323a20696e76616c6964207468726573686f6c64000000006044820152606401610a28565b505060018054600280549285905583905560048054919291849186919060006122b383614e06565b9091555060408051868152602081018690527f976f8a9c5bdf8248dec172376d6e2b80a8e3df2f0328e381c6db8e1cf138c0f891015b60405180910390a49250929050565b600080828411156123715760405162461bcd60e51b815260206004820152602760248201527f5769746864726177616c4c696d69746174696f6e3a20696e76616c696420746860448201527f726573686f6c64000000000000000000000000000000000000000000000000006064820152608401610a28565b5050603780546038805492859055839055600480549192918491869190600061239983614e06565b9091555060408051868152602081018690527f31312c97b89cc751b832d98fd459b967a2c3eef3b49757d1cf5ebaa12bb6eee191016122e9565b6002546037546123e391906151a1565b6038546001546123f391906151a1565b1115610a675760405162461bcd60e51b815260206004820152602860248201527f5769746864726177616c4c696d69746174696f6e3a20696e76616c696420746860448201527f726573686f6c64730000000000000000000000000000000000000000000000006064820152608401610a28565b848314801561247557508481145b6124e75760405162461bcd60e51b815260206004820152602860248201527f4d61696e636861696e4761746577617956323a20696e76616c6964206172726160448201527f79206c656e6774680000000000000000000000000000000000000000000000006064820152608401610a28565b60005b8581101561262c5784848281811061250457612504614d90565b90506020020160208101906125199190614aec565b6078600089898581811061252f5761252f614d90565b90506020020160208101906125449190614aec565b6001600160a01b039081168252602082019290925260400160002080547fffffffffffffffffffffff0000000000000000000000000000000000000000ff1661010093909216929092021790558282828181106125a3576125a3614d90565b90506020020160208101906125b89190615151565b607860008989858181106125ce576125ce614d90565b90506020020160208101906125e39190614aec565b6001600160a01b031681526020810191909152604001600020805460ff19166001838181111561261557612615614c71565b02179055508061262481614e06565b9150506124ea565b507fa4f03cc9c0e0aeb5b71b4ec800702753f65748c2cf3064695ba8e8b46be704448686868686866040516120fa969594939291906152c1565b8281146126c85760405162461bcd60e51b815260206004820152602a60248201527f5769746864726177616c4c696d69746174696f6e3a20696e76616c69642061726044820152690e4c2f240d8cadccee8d60b31b6064820152608401610a28565b60005b83811015612743578282828181106126e5576126e5614d90565b905060200201356039600087878581811061270257612702614d90565b90506020020160208101906127179190614aec565b6001600160a01b031681526020810191909152604001600020558061273b81614e06565b9150506126cb565b507f80bc635c452ae67f12f9b6f12ad4daa6dbbc04eeb9ebb87d354ce10c0e210dc0848484846040516115bc9493929190615339565b8281146127db5760405162461bcd60e51b815260206004820152602a60248201527f5769746864726177616c4c696d69746174696f6e3a20696e76616c69642061726044820152690e4c2f240d8cadccee8d60b31b6064820152608401610a28565b60005b83811015612856578282828181106127f8576127f8614d90565b90506020020135603a600087878581811061281557612815614d90565b905060200201602081019061282a9190614aec565b6001600160a01b031681526020810191909152604001600020558061284e81614e06565b9150506127de565b507f64557254143204d91ba2d95acb9fda1e5fea55f77efd028685765bc1e94dd4b5848484846040516115bc9493929190615339565b8281146128ee5760405162461bcd60e51b815260206004820152602a60248201527f5769746864726177616c4c696d69746174696f6e3a20696e76616c69642061726044820152690e4c2f240d8cadccee8d60b31b6064820152608401610a28565b60005b838110156129fa57620f424083838381811061290f5761290f614d90565b90506020020135111561298a5760405162461bcd60e51b815260206004820152602860248201527f5769746864726177616c4c696d69746174696f6e3a20696e76616c696420706560448201527f7263656e746167650000000000000000000000000000000000000000000000006064820152608401610a28565b82828281811061299c5761299c614d90565b90506020020135603b60008787858181106129b9576129b9614d90565b90506020020160208101906129ce9190614aec565b6001600160a01b03168152602081019190915260400160002055806129f281614e06565b9150506128f1565b507fb05f5de88ae0294ebb6f67c5af2fcbbd593cc6bdfe543e2869794a4c8ce3ea50848484846040516115bc9493929190615339565b828114612a925760405162461bcd60e51b815260206004820152602a60248201527f5769746864726177616c4c696d69746174696f6e3a20696e76616c69642061726044820152690e4c2f240d8cadccee8d60b31b6064820152608401610a28565b60005b83811015612b0d57828282818110612aaf57612aaf614d90565b90506020020135603c6000878785818110612acc57612acc614d90565b9050602002016020810190612ae19190614aec565b6001600160a01b0316815260208101919091526040016000205580612b0581614e06565b915050612a95565b507fb5d2963614d72181b4df1f993d45b83edf42fa19710f0204217ba1b3e183bb73848484846040516115bc9493929190615339565b612b4d8282613db6565b6000828152607360205260409020610f8a9082613e58565b60008281526072602090815260408083206001600160a01b038516845290915290205460ff1661101757612ba3816001600160a01b03166014613e6d565b612bae836020613e6d565b604051602001612bbf9291906153d0565b60408051601f198184030181529082905262461bcd60e51b8252610a2891600401615451565b612bef828261404e565b6000828152607360205260409020610f8a90826140d1565b60005460ff16612c595760405162461bcd60e51b815260206004820152601460248201527f5061757361626c653a206e6f74207061757365640000000000000000000000006044820152606401610a28565b6000805460ff191690557f5db9ee0a495bf2e6ff9c91a7834c1ba4fdd244a5e8aa4e537bd38aeae4b073aa335b6040516001600160a01b03909116815260200160405180910390a1565b6000823561014084013582612cbe6080870160608801614aec565b9050612cdb612cd6368890038801610100890161516e565b613a1d565b6001612ced6040880160208901615151565b6001811115612cfe57612cfe614c71565b14612d715760405162461bcd60e51b815260206004820152602860248201527f4d61696e636861696e4761746577617956323a20696e76616c6964207265636560448201527f697074206b696e640000000000000000000000000000000000000000000000006064820152608401610a28565b60808601354614612de95760405162461bcd60e51b8152602060048201526024808201527f4d61696e636861696e4761746577617956323a20696e76616c6964206368616960448201527f6e206964000000000000000000000000000000000000000000000000000000006064820152608401610a28565b6000612dfe61084a6080890160608a01614aec565b9050612e1261012088016101008901615151565b6001811115612e2357612e23614c71565b81516001811115612e3657612e36614c71565b148015612e675750612e4e60e0880160c08901614aec565b6001600160a01b031681602001516001600160a01b0316145b612ebf5760405162461bcd60e51b815260206004820152602360248201527f4d61696e636861696e4761746577617956323a20696e76616c696420726563656044820152621a5c1d60ea1b6064820152608401610a28565b60008481526079602052604090205415612f415760405162461bcd60e51b815260206004820152603260248201527f4d61696e636861696e4761746577617956323a20717565727920666f7220707260448201527f6f636573736564207769746864726177616c00000000000000000000000000006064820152608401610a28565b6001612f5561012089016101008a01615151565b6001811115612f6657612f66614c71565b1480612f795750612f7782846133bc565b155b612feb5760405162461bcd60e51b815260206004820152603260248201527f4d61696e636861696e4761746577617956323a2072656163686564206461696c60448201527f79207769746864726177616c206c696d697400000000000000000000000000006064820152608401610a28565b6000612fff6112f3368a90038a018a614ff0565b9050600061300f607754836140e6565b6003549091506001600160a01b0316600061303d6130356101208d016101008e01615151565b878985614142565b60408051606081018252600080825260208201819052918101829052919b50919250819081906000805b8f5181101561323c578f818151811061308257613082614d90565b6020908102919091018101518051818301516040808401518151600081529586018083528f905260ff9093169085015260608401526080830152935060019060a0016020604051602081039080840390855afa1580156130e6573d6000803e3d6000fd5b505050602060405103519450846001600160a01b0316846001600160a01b0316106131795760405162461bcd60e51b815260206004820152602160248201527f4d61696e636861696e4761746577617956323a20696e76616c6964206f72646560448201527f72000000000000000000000000000000000000000000000000000000000000006064820152608401610a28565b6040517f953865650000000000000000000000000000000000000000000000000000000081526001600160a01b03808716600483015286955089169063953865659060240160206040518083038186803b1580156131d657600080fd5b505afa1580156131ea573d6000803e3d6000fd5b505050506040513d601f19601f8201168201806040525081019061320e9190614f89565b6132189083615484565b915086821061322a576001955061323c565b8061323481614e06565b915050613067565b50846132b05760405162461bcd60e51b815260206004820152603660248201527f4d61696e636861696e4761746577617956323a20717565727920666f7220696e60448201527f73756666696369656e7420766f746520776569676874000000000000000000006064820152608401610a28565b50505060008a81526079602052604090208690555050881561332c576000888152607a602052604090819020805460ff19166001179055517f89e52969465b1f1866fc5d46fd62de953962e9cb33552443cd999eba05bd20dc906133179086908e906150c4565b60405180910390a15050505050505050610aa7565b6133368688614233565b61337561334960608d0160408e01614aec565b87607460009054906101000a90046001600160a01b03168e61010001803603810190611583919061516e565b7f21e88e956aa3e086f6388e899965cef814688f99ad8bb29b08d396571016372d848c6040516133a69291906150c4565b60405180910390a1505050505050505092915050565b6001600160a01b0382166000908152603a602052604081205482106133e357506000610aa7565b60006133f2620151804261549c565b6001600160a01b0385166000908152603e60205260409020549091508111156134385750506001600160a01b0382166000908152603c6020526040902054811015610aa7565b6001600160a01b0384166000908152603d602052604090205461345c908490615484565b6001600160a01b0385166000908152603c602052604090205411159150610aa79050565b600060025460016002548460015461349891906151a1565b6134a29190615484565b6134ac919061518a565b610aa7919061549c565b60005460ff16156134fc5760405162461bcd60e51b815260206004820152601060248201526f14185d5cd8589b194e881c185d5cd95960821b6044820152606401610a28565b6000805460ff191660011790557f62e78cea01bee320cd4e420270b5ea74000d11b0c9f74754ebdbfc544b05a258612c863390565b600061119883836142c3565b60007fb9d1fe7c9deeec5dc90a2f47ff1684239519f2545b2228d3d91fb27df3189eea60001b8260000151836020015161357a85604001516142ed565b61358786606001516142ed565b6135948760800151614350565b6040516020016135a9969594939291906154be565b604051602081830303815290604052805190602001209050919050565b6000620f42406135d683856151a1565b611198919061549c565b6000816001600160a01b0316836001600160a01b031614156136905760408086015190516001600160a01b0386169180156108fc02916000818181858888f1935050505061368b57816001600160a01b031663d0e30db086604001516040518263ffffffff1660e01b81526004016000604051808303818588803b15801561366757600080fd5b505af115801561367b573d6000803e3d6000fd5b505050505061368b858585614393565b613a0c565b6000855160018111156136a5576136a5614c71565b1415613866576040517f70a082310000000000000000000000000000000000000000000000000000000081523060048201526000906001600160a01b038516906370a082319060240160206040518083038186803b15801561370657600080fd5b505afa15801561371a573d6000803e3d6000fd5b505050506040513d601f19601f8201168201806040525081019061373e9190614f89565b9050856040015181101561385557836001600160a01b03166340c10f193083896040015161376c919061518a565b6040516001600160a01b03909216602483015260448201526064016040516020818303038152906040529060e01b6020820180516001600160e01b0383818316178352505050506040516137c091906154f8565b6000604051808303816000865af19150503d80600081146137fd576040519150601f19603f3d011682016040523d82523d6000602084013e613802565b606091505b505080925050816138555760405162461bcd60e51b815260206004820152601b60248201527f546f6b656e3a204552433230206d696e74696e67206661696c656400000000006044820152606401610a28565b613860868686614393565b50613a0c565b60018551600181111561387b5761387b614c71565b141561399e5761389083858760200151614437565b61368b57602085810151604080516001600160a01b038881166024830152604480830194909452825180830390940184526064909101825292820180516001600160e01b03167f40c10f1900000000000000000000000000000000000000000000000000000000179052519185169161390991906154f8565b6000604051808303816000865af19150503d8060008114613946576040519150601f19603f3d011682016040523d82523d6000602084013e61394b565b606091505b5050809150508061368b5760405162461bcd60e51b815260206004820152601c60248201527f546f6b656e3a20455243373231206d696e74696e67206661696c6564000000006044820152606401610a28565b60405162461bcd60e51b815260206004820152602160248201527f546f6b656e3a20756e737570706f7274656420746f6b656e207374616e64617260448201527f64000000000000000000000000000000000000000000000000000000000000006064820152608401610a28565b5050505050565b6000610aa7825490565b600081516001811115613a3257613a32614c71565b148015613a43575060008160400151115b8015613a5157506020810151155b80613a7b5750600181516001811115613a6c57613a6c614c71565b148015613a7b57506040810151155b610a655760405162461bcd60e51b815260206004820152601360248201527f546f6b656e3a20696e76616c696420696e666f000000000000000000000000006044820152606401610a28565b600060608186516001811115613adf57613adf614c71565b1415613bbd5760408681015181516001600160a01b038881166024830152878116604483015260648083019390935283518083039093018352608490910183526020820180516001600160e01b03166323b872dd60e01b179052915191851691613b4991906154f8565b6000604051808303816000865af19150503d8060008114613b86576040519150601f19603f3d011682016040523d82523d6000602084013e613b8b565b606091505b509092509050818015613bb6575080511580613bb6575080806020019051810190613bb69190615514565b9150613c84565b600186516001811115613bd257613bd2614c71565b141561399e57602086810151604080516001600160a01b0389811660248301528881166044830152606480830194909452825180830390940184526084909101825292820180516001600160e01b03166323b872dd60e01b1790525191851691613c3c91906154f8565b6000604051808303816000865af19150503d8060008114613c79576040519150601f19603f3d011682016040523d82523d6000602084013e613c7e565b606091505b50909250505b81610f5c57613c92866144e2565b613ca6866001600160a01b03166014613e6d565b613cba866001600160a01b03166014613e6d565b613cce866001600160a01b03166014613e6d565b604051602001612bbf9493929190615536565b613d516040805160a08101825260008082526020808301829052835160608082018652838252818301849052818601849052848601919091528451808201865283815280830184905280860184905281850152845190810185528281529081018290529283015290608082015290565b83815260006020820181905250604080820180516001600160a01b039788169052602080890151825190891690820152905146908301528751606084018051918916909152805195909716940193909352935182015292909201516080820152919050565b60008281526072602090815260408083206001600160a01b038516845290915290205460ff166110175760008281526072602090815260408083206001600160a01b03851684529091529020805460ff19166001179055613e143390565b6001600160a01b0316816001600160a01b0316837f2f8788117e7eff1d82e926ec794901d17c78024a50270940304540a733656f0d60405160405180910390a45050565b6000611198836001600160a01b03841661454f565b60606000613e7c8360026151a1565b613e87906002615484565b67ffffffffffffffff811115613e9f57613e9f614e21565b6040519080825280601f01601f191660200182016040528015613ec9576020820181803683370190505b5090507f300000000000000000000000000000000000000000000000000000000000000081600081518110613f0057613f00614d90565b60200101906001600160f81b031916908160001a9053507f780000000000000000000000000000000000000000000000000000000000000081600181518110613f4b57613f4b614d90565b60200101906001600160f81b031916908160001a9053506000613f6f8460026151a1565b613f7a906001615484565b90505b6001811115613fff577f303132333435363738396162636465660000000000000000000000000000000085600f1660108110613fbb57613fbb614d90565b1a60f81b828281518110613fd157613fd1614d90565b60200101906001600160f81b031916908160001a90535060049490941c93613ff881615606565b9050613f7d565b5083156111985760405162461bcd60e51b815260206004820181905260248201527f537472696e67733a20686578206c656e67746820696e73756666696369656e746044820152606401610a28565b60008281526072602090815260408083206001600160a01b038516845290915290205460ff16156110175760008281526072602090815260408083206001600160a01b0385168085529252808320805460ff1916905551339285917ff6391f5c32d9c69d2a47ea670b442974b53935d1edc7fd64eb21e047a839171b9190a45050565b6000611198836001600160a01b03841661459e565b604080517f19010000000000000000000000000000000000000000000000000000000000006020808301919091526022820185905260428083018590528351808403909101815260629092019092528051910120600090611198565b6000806000836001600160a01b031663926323d56040518163ffffffff1660e01b815260040160206040518083038186803b15801561418057600080fd5b505afa158015614194573d6000803e3d6000fd5b505050506040513d601f19601f820116820180604052508101906141b89190614f89565b90506141c381613480565b925060008760018111156141d9576141d9614c71565b1415614229576001600160a01b038616600090815260396020526040902054851061420a5761420781614691565b92505b6001600160a01b0386166000908152603a602052604090205485101591505b5094509492505050565b6000614242620151804261549c565b6001600160a01b0384166000908152603e6020526040902054909150811115614291576001600160a01b03929092166000908152603e6020908152604080832094909455603d90529190912055565b6001600160a01b0383166000908152603d6020526040812080548492906142b9908490615484565b9091555050505050565b60008260000182815481106142da576142da614d90565b9060005260206000200154905092915050565b805160208083015160408085015190516000946135a9947f353bdd8d69b9e3185b3972e08b03845c0c14a21a390215302776a7a34b0e87649491939192019384526001600160a01b03928316602085015291166040830152606082015260800190565b805160208083015160408085015190516000946135a9947f1e2b74b2a792d5c0f0b6e59b037fa9d43d84fbb759337f0112fcc15ca414fc8d94919391920161561d565b600080845160018111156143a9576143a9614c71565b14156143c5576143be828486604001516146a9565b90506143ef565b6001845160018111156143da576143da614c71565b141561399e576143be82848660200151614437565b80610e89576143fd846144e2565b614411846001600160a01b03166014613e6d565b614425846001600160a01b03166014613e6d565b604051602001612bbf93929190615648565b604080513060248201526001600160a01b038481166044830152606480830185905283518084039091018152608490920183526020820180516001600160e01b03166323b872dd60e01b1790529151600092861691614495916154f8565b6000604051808303816000865af19150503d80600081146144d2576040519150601f19603f3d011682016040523d82523d6000602084013e6144d7565b606091505b509095945050505050565b606061450d826000015160018111156144fd576144fd614c71565b6001600160a01b03166001613e6d565b61451a8360200151614795565b6145278460400151614795565b604051602001614539939291906156d9565b6040516020818303038152906040529050919050565b600081815260018301602052604081205461459657508154600181810184556000848152602080822090930184905584548482528286019093526040902091909155610aa7565b506000610aa7565b600081815260018301602052604081205480156146875760006145c260018361518a565b85549091506000906145d69060019061518a565b905081811461463b5760008660000182815481106145f6576145f6614d90565b906000526020600020015490508087600001848154811061461957614619614d90565b6000918252602080832090910192909255918252600188019052604090208390555b855486908061464c5761464c6157a2565b600190038181906000526020600020016000905590558560010160008681526020019081526020016000206000905560019350505050610aa7565b6000915050610aa7565b600060385460016038548460375461349891906151a1565b604080516001600160a01b038481166024830152604480830185905283518084039091018152606490920183526020820180516001600160e01b03167fa9059cbb0000000000000000000000000000000000000000000000000000000017905291516000926060929087169161471f91906154f8565b6000604051808303816000865af19150503d806000811461475c576040519150601f19603f3d011682016040523d82523d6000602084013e614761565b606091505b50909250905081801561478c57508051158061478c57508080602001905181019061478c9190615514565b95945050505050565b6060816147d557505060408051808201909152600481527f3078303000000000000000000000000000000000000000000000000000000000602082015290565b8160005b81156147f857806147e981614e06565b915050600882901c91506147d9565b6111848482613e6d565b604080516060810182526000808252602082015290810161483e6040805160608101909152806000815260200160008152602001600081525090565b905290565b60006020828403121561485557600080fd5b81356001600160e01b03198116811461119857600080fd5b6001600160a01b0381168114610a6557600080fd5b803561198d8161486d565b8060608101831015610aa757600080fd5b8060808101831015610aa757600080fd5b60008083601f8401126148c157600080fd5b50813567ffffffffffffffff8111156148d957600080fd5b6020830191508360208260051b850101111561172857600080fd5b60008060008060008060008060008060006101408c8e03121561491657600080fd5b61491f8c614882565b9a5061492d60208d01614882565b995061493b60408d01614882565b985060608c0135975060808c0135965060a08c0135955060c08c0135945067ffffffffffffffff8060e08e0135111561497357600080fd5b6149838e60e08f01358f0161488d565b9450806101008e0135111561499757600080fd5b6149a88e6101008f01358f0161489e565b9350806101208e013511156149bc57600080fd5b506149ce8d6101208e01358e016148af565b81935080925050509295989b509295989b9093969950565b600080600080604085870312156149fc57600080fd5b843567ffffffffffffffff80821115614a1457600080fd5b614a20888389016148af565b90965094506020870135915080821115614a3957600080fd5b50614a46878288016148af565b95989497509550505050565b60008060008060008060608789031215614a6b57600080fd5b863567ffffffffffffffff80821115614a8357600080fd5b614a8f8a838b016148af565b90985096506020890135915080821115614aa857600080fd5b614ab48a838b016148af565b90965094506040890135915080821115614acd57600080fd5b50614ada89828a016148af565b979a9699509497509295939492505050565b600060208284031215614afe57600080fd5b81356111988161486d565b600060208284031215614b1b57600080fd5b5035919050565b60008060408385031215614b3557600080fd5b823591506020830135614b478161486d565b809150509250929050565b600060a08284031215614b6457600080fd5b50919050565b60006101608284031215614b6457600080fd5b60008060006101808486031215614b9357600080fd5b614b9d8585614b6a565b925061016084013567ffffffffffffffff80821115614bbb57600080fd5b818601915086601f830112614bcf57600080fd5b813581811115614bde57600080fd5b876020606083028501011115614bf357600080fd5b6020830194508093505050509250925092565b60008060408385031215614c1957600080fd5b8235614c248161486d565b946020939093013593505050565b60008060408385031215614c4557600080fd5b50508035926020909101359150565b60006101608284031215614c6757600080fd5b6111988383614b6a565b634e487b7160e01b600052602160045260246000fd5b60028110610a6557634e487b7160e01b600052602160045260246000fd5b81516040820190614cb581614c87565b808352506001600160a01b03602084015116602083015292915050565b60008060008060008060006080888a031215614ced57600080fd5b873567ffffffffffffffff80821115614d0557600080fd5b614d118b838c016148af565b909950975060208a0135915080821115614d2a57600080fd5b614d368b838c016148af565b909750955060408a0135915080821115614d4f57600080fd5b614d5b8b838c016148af565b909550935060608a0135915080821115614d7457600080fd5b50614d818a828b0161489e565b91505092959891949750929550565b634e487b7160e01b600052603260045260246000fd5b6000808335601e19843603018112614dbd57600080fd5b83018035915067ffffffffffffffff821115614dd857600080fd5b6020019150600581901b360382131561172857600080fd5b634e487b7160e01b600052601160045260246000fd5b6000600019821415614e1a57614e1a614df0565b5060010190565b634e487b7160e01b600052604160045260246000fd5b6040516060810167ffffffffffffffff81118282101715614e6857634e487b7160e01b600052604160045260246000fd5b60405290565b60028110610a6557600080fd5b600060608284031215614e8d57600080fd5b614e95614e37565b90508135614ea281614e6e565b80825250602082013560208201526040820135604082015292915050565b600060a08284031215614ed257600080fd5b614eda614e37565b8235614ee58161486d565b81526020830135614ef58161486d565b6020820152614f078460408501614e7b565b60408201529392505050565b600060608284031215614f2557600080fd5b6040516060810181811067ffffffffffffffff82111715614f5657634e487b7160e01b600052604160045260246000fd5b604052823560ff81168114614f6a57600080fd5b8152602083810135908201526040928301359281019290925250919050565b600060208284031215614f9b57600080fd5b5051919050565b600060608284031215614fb457600080fd5b614fbc614e37565b90508135614fc98161486d565b81526020820135614fd98161486d565b806020830152506040820135604082015292915050565b6000610160828403121561500357600080fd5b60405160a0810181811067ffffffffffffffff8211171561503457634e487b7160e01b600052604160045260246000fd5b60405282358152602083013561504981614e6e565b602082015261505b8460408501614fa2565b604082015261506d8460a08501614fa2565b6060820152615080846101008501614e7b565b60808201529392505050565b80356150978161486d565b6001600160a01b0390811683526020820135906150b38261486d565b166020830152604090810135910152565b6000610180820190508382528235602083015260208301356150e581614e6e565b6150ee81614c87565b80604084015250615105606083016040850161508c565b61511560c0830160a0850161508c565b61012061010084013561512781614e6e565b61513081614c87565b81840152830135610140808401919091529092013561016090910152919050565b60006020828403121561516357600080fd5b813561119881614e6e565b60006060828403121561518057600080fd5b6111988383614e7b565b60008282101561519c5761519c614df0565b500390565b60008160001904831182151516156151bb576151bb614df0565b500290565b6000610180820190508382528251602083015260208301516151e181614c87565b6040838101919091528381015180516001600160a01b03908116606086015260208201511660808501529081015160a084015250606083015180516001600160a01b0390811660c085015260208201511660e08401526040810151610100840152506080830151805161525381614c87565b6101208401526020810151610140840152604001516101609092019190915292915050565b8183526000602080850194508260005b858110156152b657813561529b8161486d565b6001600160a01b031687529582019590820190600101615288565b509495945050505050565b6060815260006152d560608301888a615278565b6020838203818501526152e982888a615278565b8481036040860152858152869250810160005b8681101561532a57833561530f81614e6e565b61531881614c87565b825292820192908201906001016152fc565b509a9950505050505050505050565b60408152600061534d604083018688615278565b82810360208401528381527f07ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff84111561538557600080fd5b8360051b80866020840137600091016020019081529695505050505050565b60005b838110156153bf5781810151838201526020016153a7565b83811115610e895750506000910152565b7f416363657373436f6e74726f6c3a206163636f756e74200000000000000000008152600083516154088160178501602088016153a4565b7f206973206d697373696e6720726f6c652000000000000000000000000000000060179184019182015283516154458160288401602088016153a4565b01602801949350505050565b60208152600082518060208401526154708160408501602087016153a4565b601f01601f19169190910160400192915050565b6000821982111561549757615497614df0565b500190565b6000826154b957634e487b7160e01b600052601260045260246000fd5b500490565b8681526020810186905260c081016154d586614c87565b8560408301528460608301528360808301528260a0830152979650505050505050565b6000825161550a8184602087016153a4565b9190910192915050565b60006020828403121561552657600080fd5b8151801515811461119857600080fd5b7f546f6b656e3a20636f756c64206e6f74207472616e7366657220000000000000815260008551602061556f82601a8601838b016153a4565b7f2066726f6d200000000000000000000000000000000000000000000000000000601a9285019283015286516155aa81838501848b016153a4565b630103a37960e51b92018181019290925285516155cd81602485018985016153a4565b660103a37b5b2b7160cd1b6024939091019283015284516155f481602b85018489016153a4565b91909101602b01979650505050505050565b60008161561557615615614df0565b506000190190565b8481526080810161562d85614c87565b84602083015283604083015282606083015295945050505050565b7f546f6b656e3a20636f756c64206e6f74207472616e736665722000000000000081526000845161568081601a8501602089016153a4565b630103a37960e51b601a9184019182015284516156a481601e8401602089016153a4565b660103a37b5b2b7160cd1b601e929091019182015283516156cc8160258401602088016153a4565b0160250195945050505050565b7f546f6b656e496e666f280000000000000000000000000000000000000000000081526000845161571181600a8501602089016153a4565b80830190507f2c0000000000000000000000000000000000000000000000000000000000000080600a830152855161575081600b850160208a016153a4565b600b920191820152835161576b81600c8401602088016153a4565b7f2900000000000000000000000000000000000000000000000000000000000000600c9290910191820152600d0195945050505050565b634e487b7160e01b600052603160045260246000fdfeb53127684a568b3173ae13b9f8a6016e243e63b6e8ee1178d6a717850b5d610348617350726f787941646d696e3a20756e617574686f72697a65642073656e64",
        ),
    ];
}
