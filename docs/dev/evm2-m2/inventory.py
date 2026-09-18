#!/usr/bin/env python3
"""Rebuild the M2 static inventory. Lexical evidence is not semantic validation."""
import argparse
import collections
import csv
import io
import hashlib
import json
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
OUT = Path(__file__).resolve().parent
SRC = ROOT / 'crates/cheatcodes/src'

def names(text):
    return set(text.split())

FAMILIES = {
    'M3-live-state': names('snapshot snapshotState revertTo revertToState revertToAndDelete revertToStateAndDelete deleteSnapshot deleteStateSnapshot deleteSnapshots deleteStateSnapshots createFork createSelectFork selectFork activeFork rollFork makePersistent revokePersistent isPersistent transact executeTransaction'),
    'M4-observers': names('breakpoint pauseTracing resumeTracing startDebugTraceRecording stopAndReturnDebugTraceRecording'),
    'M4-publication': names('broadcastRawTransaction'),
    'M5-network': names('setLogoURI setTip20LogoURI expectLogoURIUpdated expectTip20LogoURIUpdated expectKeychainVerified expectKeychainAdminVerified'),
    'M2-assert': names('assertTrue assertFalse assertEq assertNotEq assertGt assertGe assertLt assertLe assertEqDecimal assertNotEqDecimal assertGtDecimal assertGeDecimal assertLtDecimal assertLeDecimal assertApproxEqAbs assertApproxEqAbsDecimal assertApproxEqRel assertApproxEqRelDecimal'),
    'M2-expect': names('_expectCheatcodeRevert expectRevert expectPartialRevert expectCall expectCallMinGas expectDelegateCall expectEmit expectEmitAnonymous expectCreate expectCreate2'),
    'M2-prank': names('prank startPrank stopPrank readCallers'),
    'M2-mock': names('mockCall mockCalls mockCallRevert clearMockedCalls mockFunction'),
    'M2-gas': names('pauseGasMetering resumeGasMetering resetGasMetering lastCallGas lastFrameGas snapshotGasLastCall snapshotGasLastFrame snapshotValue startSnapshotGas stopSnapshotGas'),
    'M2-record': names('record stopRecord accesses recordLogs getRecordedLogs getRecordedLogsJson startStateDiffRecording stopAndReturnStateDiff getStateDiff getStateDiffJson getStorageSlots getStorageAccesses startMappingRecording stopMappingRecording getMappingKeyAndParentOf getMappingLength getMappingSlotAt'),
    'M2-hooks': names('registerMappingSstoreHook registerSloadHook registerSstoreHook'),
    'M2-memory': names('expectSafeMemory expectSafeMemoryCall stopExpectSafeMemory'),
    'M2-control': names('assume assumeNoRevert skip isContext isIsolateMode isImplicitlyApproved assumeImplicitApproval'),
    'M2-arbitrary': names('setArbitraryStorage copyStorage'),
    'M2-broadcast': names('broadcast startBroadcast stopBroadcast attachBlob attachDelegation signAndAttachDelegation signDelegation getWallets'),
    'M2-create': names('deployCode interceptInitcode'),
    'M2-account': names('load store etch deal getNonce resetNonce setNonce setNonceUnsafe cloneAccount cool warmSlot coolSlot accessList noAccessList loadAllocs dumpState allowCheatcodes'),
    'M2-env': names('warp roll rollSlot getSlotNumber coinbase fee prevrandao difficulty chainId getChainId getBlockTimestamp getBlockNumber txGasPrice blobBaseFee getBlobBaseFee blobhashes getBlobhashes setBlockhash setEvmVersion getEvmVersion'),
}
STATE_FAMILIES = {
    'M2-env': names('block gas_price execution_evm_version env_overrides'),
    'M2-broadcast': names('active_delegations active_blob_sidecar broadcast broadcastable_transactions script_address dynamic_gas_limit'),
    'M2-prank': names('pranks'),
    'M2-expect': names('expected_revert expected_calls expected_emits expected_creates'),
    'M2-control': names('assume_no_revert test_context skip_payloads'),
    'M2-record': names('accesses recording_accesses recorded_account_diffs_stack pending_account_diffs recorded_account_diffs_prefix created_accounts created_account_bindings created_account_changes created_accounts_frames recorded_logs mapping_slots'),
    'M2-mock': names('mocked_calls mocked_functions'),
    'M2-memory': names('allowed_mem_writes'),
    'M2-account': names('access_list eth_deals'),
    'M2-gas': names('gas_metering gas_snapshots'),
    'M2-create': names('intercept_next_create_call'),
    'M2-arbitrary': names('arbitrary_storage'),
    'M2-hooks': names('storage_load_hooks storage_store_hooks mapping_storage_store_hooks storage_hook_mapping_slots pending_mapping_hash storage_hooks_registered pending_storage_hook active_storage_hook'),
    'M2-host': names('analysis labels config fs_commit serialized_jsons test_runner deprecated wallets private_key_signers signatures_identifier'),
    'M3-live-state': names('fork_block_number_override fork_revert_diagnostic created_accounts_snapshots env_overrides_snapshots fork_block_number_override_snapshots in_isolation_context'),
    'M4-observers': names('record_debug_steps_info pc breakpoints ignored_traces'),
    'deferred-network': names('context_snapshots extra_cheatcode_addresses'),
}

HOST_GROUPS = {'crypto', 'environment', 'filesystem', 'json', 'toml', 'string', 'utilities'}
HOST_TEST = names('foundryVersionAtLeast foundryVersionCmp getChain getFoundryVersion rpcUrl rpcUrlStructs rpcUrls sleep')
HOST_EVM = names('addr eth_getLogs getRawBlockHeader rpcJson rpc')

def family(c):
    name = c['func']['signature'].split('(')[0]
    matches = [k for k, v in FAMILIES.items() if name in v]
    assert len(matches) <= 1, name
    if matches:
        return matches[0]
    if c['group'] in HOST_GROUPS or name in HOST_TEST | HOST_EVM:
        return 'M2-host-' + c['group']
    raise ValueError('Unclassified ABI entry: ' + c['func']['id'])

def render_csv(rows, fields):
    out = io.StringIO(newline='')
    writer = csv.DictWriter(out, fieldnames=fields, lineterminator='\n')
    writer.writeheader()
    writer.writerows(rows)
    return out.getvalue()

def generate():
    cheats = json.loads((ROOT / 'crates/cheatcodes/assets/cheatcodes.json').read_text())['cheatcodes']
    sources = {p: p.read_text() for p in sorted(SRC.rglob('*.rs')) if p.name != 'native.rs'}
    native = (SRC / 'native.rs').read_text().split('#[cfg(test)]')[0]
    direct = set(re.findall(r'Vm::VmCalls::(\w+)\(', native))
    assertions = set(re.findall(r'\b(assert\w+)Call\b', (SRC / 'test/assert.rs').read_text()))
    shared = {m for text in sources.values() for m in re.findall(r"impl StatelessCheatcode for (\w+)Call", text)}
    expect = (SRC / 'test/expect.rs').read_text()
    for match in re.finditer(r'impl Cheatcode for (\w+)Call\s*\{(.*?)(?=\nimpl |\Z)', expect, re.S):
        if re.search(r'fn apply_(?:expectation|call_expectation|emit_expectation)\(', match[2]):
            shared.add(match[1])
    # These two macros implement StatelessCheatcode for both concrete input types.
    for name in ['json', 'toml']:
        source = (SRC / f'{name}.rs').read_text()
        assert 'impl StatelessCheatcode for $call' in source
        for match in re.finditer(r'impl_parse_' + name + r'!\(\s*(\w+)Call,\s*(\w+)Call,', source):
            shared.update(match.groups())
    known = {c['func']['id'] for c in cheats}
    assert direct | assertions | shared <= known, 'Native extraction found unknown ABI ids'
    assert len(known) == len(cheats), 'Duplicate ABI ids'
    # Candidate files only: aliases, comments, dead code and overload ambiguity are not resolved.
    test_files = {}
    paths = subprocess.check_output(['git', 'ls-files', '--cached', '--others', '--exclude-standard', '--', 'testdata', 'crates/forge/tests', 'crates/cheatcodes/src'], cwd=ROOT, text=True).splitlines()
    for rel in sorted(set(paths)):
        p = ROOT / rel
        if p.is_file() and p.suffix in {'.sol', '.rs'}:
            text = p.read_text()
            if not rel.startswith('crates/cheatcodes/src/') or '#[test]' in text:
                test_files[p] = text
    candidate_index = collections.defaultdict(set)
    for p, text in test_files.items():
        for name in set(re.findall(r'\b(\w+)\s*\(', text)):
            candidate_index[name].add(str(p.relative_to(ROOT)))
    candidates = {c['func']['signature'].split('(')[0]: '' for c in cheats}
    for name in candidates:
        candidates[name] = ';'.join(sorted(candidate_index[name]))
    abi_names = collections.Counter(c['func']['signature'].split('(')[0] for c in cheats)
    spec_names = collections.Counter(re.findall(r'^\s*function\s+(\w+)\s*\(', (ROOT / 'crates/cheatcodes/spec/src/vm.rs').read_text(), re.M))
    assert abi_names == spec_names, 'ABI assets and Vm declarations differ'
    rows = []
    for c in cheats:
        f = c['func']; ident = f['id']; name = f['signature'].split('(')[0]
        # Generated assertion impls are represented by their macro input tokens.
        token = re.compile(r'\b' + re.escape(ident) + r'Call\b')
        refs = [f'{p.relative_to(ROOT)}:{text[:m.start()].count(chr(10))+1}' for p, text in sources.items() if (m := token.search(text))]
        assert refs, 'Missing reference source token: ' + ident
        route = 'conditional' if ident in {'roll', 'etch'} else ('direct' if ident in direct else 'shared' if ident in assertions | shared else 'absent')
        rows.append(dict(id=ident, selector=f['selector'], signature=f['signature'], group=c['group'], family=family(c), native_route=route, reference_tokens=';'.join(refs), test_lookup=name))
    inspector = (SRC / 'inspector.rs').read_text()
    start = inspector.index('pub struct Cheatcodes<')
    end = inspector.index('\n}', start)
    state = []
    for m in re.finditer(r'^    (?:pub(?:\([^)]*\))? )?(\w+):', inspector[start:end], re.M):
        owners = [k for k,v in STATE_FAMILIES.items() if m[1] in v]
        assert len(owners) == 1, 'Unclassified/ambiguous state field: ' + m[1]
        state.append(dict(field=m[1], family=owners[0], reference=f'crates/cheatcodes/src/inspector.rs:{inspector[:start+m.start()].count(chr(10))+1}', lifecycle_validation='open'))
    configuration = []
    for rel, struct in [('crates/cheatcodes/src/config.rs', 'CheatsConfig'), ('crates/evm/evm/src/inspectors/stack.rs', 'InspectorStackInner')]:
        text = (ROOT / rel).read_text()
        start = text.index('pub struct ' + struct)
        end = text.index('\n}', start)
        for m in re.finditer(r'^    (?:pub(?:\([^)]*\))? )?(\w+):', text[start:end], re.M):
            configuration.append(dict(struct=struct, field=m[1], reference=f'{rel}:{text[:start+m.start()].count(chr(10))+1}', validation='open-L8'))
    native_fields = []
    for rel, structs in [
        ('crates/cheatcodes/src/native.rs', ['Session', 'Expectations', 'CallContext']),
        ('crates/evm/evm/src/executors/evm2.rs', ['MigrationInspector']),
    ]:
        text = (ROOT / rel).read_text()
        for struct in structs:
            start = text.index('struct ' + struct)
            end = text.index('\n}', start)
            for m in re.finditer(r'^    (?:pub(?:\([^)]*\))? )?(\w+):', text[start:end], re.M):
                native_fields.append(dict(struct=struct, field=m[1], reference=f'{rel}:{text[:start+m.start()].count(chr(10))+1}', validation='open-L2-L3-L7'))
    hooks = []
    for rel in ['crates/cheatcodes/src/inspector.rs', 'crates/evm/evm/src/inspectors/stack.rs', 'crates/evm/evm/src/executors/evm2.rs']:
        text = (ROOT / rel).read_text()
        for m in re.finditer(r'^    fn (initialize_interp|step|step_end|log|log_full|call|call_end|create|create_end|selfdestruct|frame_start|frame_end)\(', text, re.M):
            hooks.append(dict(source=rel, hook=m[1], line=text[:m.start()].count(chr(10))+1))
    counts = collections.Counter((r['family'], r['native_route']) for r in rows)
    summary = '# Generated static counts\n\nNo row is certified behaviorally by this analysis. Rebuild with `python3 docs/dev/evm2-m2/inventory.py`.\n\n| Owner/family | Total | Direct | Shared | Conditional | Absent |\n| --- | ---: | ---: | ---: | ---: | ---: |\n'
    for f in sorted({r['family'] for r in rows}):
        vals = [counts[f, s] for s in ['direct', 'shared', 'conditional', 'absent']]
        summary += '| ' + f + ' | ' + ' | '.join(map(str, [sum(vals), *vals])) + ' |\n'
    summary += f'\nTotal: {len(rows)} ABI entries; {len(state)} Cheatcodes state fields; {len(hooks)} explicit callback definitions; {len(configuration)} config/stack fields.\n'
    provenance = {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(set(sources) | {SRC / 'native.rs', ROOT / 'crates/cheatcodes/spec/src/vm.rs', ROOT / 'crates/cheatcodes/assets/cheatcodes.json', ROOT / 'crates/evm/evm/src/executors/evm2.rs', ROOT / 'crates/evm/evm/src/inspectors/stack.rs'})}
    return {'native-fields.csv': render_csv(native_fields, list(native_fields[0])), 'configuration.csv': render_csv(configuration, list(configuration[0])), 'source-hashes.json': json.dumps(provenance, indent=2) + '\n', 'test-candidates.csv': render_csv([dict(name=n, candidate_files=v) for n,v in sorted(candidates.items())], ['name', 'candidate_files']), 'selectors.csv': render_csv(rows, list(rows[0])), 'state-fields.csv': render_csv(state, list(state[0])), 'callbacks.csv': render_csv(hooks, list(hooks[0])), 'counts.md': summary}

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true', help='Fail on stale generated inventory')
    args = parser.parse_args()
    stale = []
    for name, text in generate().items():
        path = OUT / name
        if args.check:
            if not path.exists() or path.read_text() != text:
                stale.append(name)
        else:
            path.write_text(text)
    if stale:
        raise SystemExit('Stale inventory: ' + ', '.join(stale))
    print('Static inventory ' + ('checked' if args.check else 'generated'))

if __name__ == '__main__':
    main()
