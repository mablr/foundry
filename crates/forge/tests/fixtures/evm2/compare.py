#!/usr/bin/env python3
"""Compare the bounded migration fixtures with pinned REVM and evm2 Forge binaries."""

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--reference", type=Path, required=True)
parser.add_argument("--candidate", type=Path, required=True)
parser.add_argument("--output", type=Path, required=True)
args = parser.parse_args()
fixture = Path(__file__).resolve().parent
binaries = {"reference": args.reference.resolve(), "candidate": args.candidate.resolve()}
report = {"binaries": {}, "scenarios": {}, "fixtures": {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in fixture.glob("*.sol")}}
for name, binary in binaries.items():
    report["binaries"][name] = {
        "sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "version": subprocess.check_output([str(binary), "--version"], text=True).strip(),
    }
fields = ("status", "reason", "logs", "decoded_logs", "kind", "gas_snapshots", "labeled_addresses")
scenarios = {
    "basic": (True, "^(BasicTest|BasicFailureTest|MilestoneTest|MilestoneFailureTest)$"),
    "execution_bench": (True, "^ExecutionBenchTest$"),
    "console": (True, "^BasicConsoleTest$"),
    "non_reverting_assertions": (False, "^(BasicConsoleTest|BasicFailureTest)$"),
}
for scenario, (assertions_revert, contract) in scenarios.items():
    results = {}
    for name, binary in binaries.items():
        with tempfile.TemporaryDirectory(prefix="evm2-differential-") as directory:
            root = Path(directory)
            for source in fixture.glob("*.sol"):
                shutil.copy2(source, root / source.name)
            (root / "foundry.toml").write_text(
                '[profile.default]\nsrc = "."\ntest = "."\nevm_version = "cancun"\nsolc = "0.8.35"\n'
                'optimizer = true\nassertions_revert = ' + str(assertions_revert).lower() + '\n'
            )
            run = subprocess.run(
                [str(binary), "test", "--root", str(root), "--no-isolate", "--match-contract", contract, "--json"],
                text=True, capture_output=True,
            )
            if run.returncode not in (0, 1):
                raise RuntimeError(f"{name}: {run.stderr}")
            suites = json.loads(run.stdout)
            results[name] = {
                "exit_code": run.returncode,
                "tests": {
                    f"{suite}/{test}": {field: result[field] for field in fields}
                    for suite, data in suites.items()
                    for test, result in data["test_results"].items()
                },
            }
            if not results[name]["tests"]:
                raise RuntimeError(f"{name}: zero tests in {scenario}")
    report["scenarios"][scenario] = {
        "matched": results["reference"] == results["candidate"],
        **results,
    }
args.output.parent.mkdir(parents=True, exist_ok=True)
args.output.write_text(json.dumps(report, indent=2) + "\n")
for scenario, result in report["scenarios"].items():
    print(f"{scenario}: {len(result['reference']['tests'])} tests, matched={result['matched']}")
raise SystemExit(0 if all(s["matched"] for s in report["scenarios"].values()) else 1)
