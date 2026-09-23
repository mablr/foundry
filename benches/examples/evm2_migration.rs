//! Runs the bounded migration fixture through the existing warm-cache benchmark runner.
//!
//! Put the selected profiling-build Forge binary on PATH, then pass a version label and output
//! path.

use eyre::{ContextCompat, Result};
use foundry_bench::BenchmarkProject;
use foundry_compilers::project_util::TempProject;
use std::{env, fs, path::Path};

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let version = args.next().wrap_err("expected version label")?;
    let output = args.next().wrap_err("expected output path")?;
    let filter = match args.next().as_deref() {
        None | Some("both") => "",
        Some("compute") => " --match-test '^testCompute$'",
        Some("cheatcodes") => " --match-test '^testCheatcodeReads$'",
        Some(_) => eyre::bail!("expected both, compute, or cheatcodes"),
    };
    eyre::ensure!(args.next().is_none(), "unexpected argument");
    let temp_project = TempProject::dapptools()?;
    let root_path = temp_project.root().to_path_buf();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/forge/tests/fixtures/evm2");
    for name in ["ExecutionBench.t.sol", "foundry.toml"] {
        fs::copy(fixture.join(name), root_path.join(name))?;
    }
    let project = BenchmarkProject {
        name: "evm2-migration".into(),
        temp_project,
        root_path,
        extra_args: Some(format!("--no-isolate --match-contract '^ExecutionBenchTest$'{filter}")),
        org: "local".into(),
        repo: "evm2-migration".into(),
        revision: "fixture".into(),
    };
    let result = project.bench_forge_test_filtered(&version, 30, false)?;
    fs::write(output, serde_json::to_vec_pretty(&result)?)?;
    Ok(())
}
