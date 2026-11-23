//! Model-based conformance test of the fork, snapshot and persistent-account lifecycle.
//!
//! [`Model`] is a reference implementation of the Lean state machine in
//! `formal/Lifecycle/Registry.lean`: each operation and helper mirrors the Lean definition of the
//! same name (`select_fork` is `selectFork`, `inherit_local` is `inheritLocal`, and so on). The
//! test drives the real [`ExecutionLifecycle`] over in-memory backing databases with pseudo-random
//! operation sequences and compares, after every step, everything the model makes observable: the
//! active selection, persistent accounts, live snapshots, the running state of the active context
//! and the accepted state of every inactive context.
//!
//! Every write loads the whole account first (its info and its only slot), which is the regime the
//! model describes: accepted overlays hold whole accounts, except for the slots that leaving the
//! local context copies into forks.

use super::*;
use evm2::evm::DynDatabase;
use std::{
    collections::{BTreeMap, BTreeSet},
    iter, mem,
};

/// Number of tracked accounts.
const ACCOUNTS: u8 = 3;

/// Maximum number of forks a sequence creates.
const MAX_FORKS: u64 = 3;

/// A context: the local one for `None`, otherwise a fork.
type Ctx = Option<u64>;

/// An account field the model tracks: its balance or its storage slot zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Field {
    Bal,
    Slot,
}

const FIELDS: [Field; 2] = [Field::Bal, Field::Slot];

/// `Loc` in the Lean model.
type Loc = (u8, Field);

/// `Overlay` in the Lean model: accepted or in-flight values over a backing database.
type Overlay = BTreeMap<Loc, u64>;

/// `backing` in the Lean model: the value of `loc` in the database of `seed`.
fn backing(seed: u64, (account, field): Loc) -> u64 {
    1_000 + 100 * seed + 10 * u64::from(account) + if field == Field::Bal { 1 } else { 2 }
}

fn address(account: u8) -> Address {
    Address::with_last_byte(0x10 + account)
}

/// `Parked` in the Lean model, mirroring [`Parked`].
#[derive(Clone, Debug, Default)]
struct MParked {
    seed: u64,
    /// `Parked::cache`, with `ran` for `Some`.
    overlay: Overlay,
    ran: bool,
    /// `Parked::loaded`; every loaded account has its slot loaded.
    loaded: BTreeSet<u8>,
}

impl MParked {
    fn new(seed: u64) -> Self {
        Self { seed, ..Default::default() }
    }

    /// `Parked.read`: the context's accepted value of `loc`.
    fn read(&self, loc: Loc) -> u64 {
        self.overlay.get(&loc).copied().unwrap_or_else(|| backing(self.seed, loc))
    }
}

/// `Running` in the Lean model: the running state of the active context.
#[derive(Clone, Debug, Default)]
struct MRunning {
    accepted: Overlay,
    inflight: Overlay,
    tx_loaded: BTreeSet<u8>,
}

/// `Snap` in the Lean model, mirroring [`LifecycleSnapshot`].
#[derive(Clone, Debug)]
struct MSnap {
    active: Ctx,
    parked: MParked,
    running: MRunning,
}

/// `Op` in the Lean model.
#[derive(Clone, Copy, Debug)]
enum Op {
    CreateFork(u64),
    SelectFork(u64),
    SetBalance(u8, u64),
    SetSlot(u8, u64),
    MakePersistent(u8),
    RevokePersistent(u8),
    Snapshot,
    RevertTo(u64, bool),
    DeleteSnapshot(u64),
    RollFork(u64, u64),
    EndTx,
    Peek(u8, Field),
}

/// `St` in the Lean model.
#[derive(Clone, Debug)]
struct Model {
    local: MParked,
    forks: BTreeMap<u64, MParked>,
    active: Ctx,
    persistent: BTreeSet<u8>,
    running: MRunning,
    snaps: BTreeMap<u64, MSnap>,
    next_snap: u64,
}

impl Model {
    fn new() -> Self {
        Self {
            local: MParked::new(0),
            forks: BTreeMap::new(),
            active: None,
            persistent: BTreeSet::new(),
            running: MRunning::default(),
            snaps: BTreeMap::new(),
            next_snap: 0,
        }
    }

    fn parked(&self, ctx: Ctx) -> Option<&MParked> {
        match ctx {
            None => Some(&self.local),
            Some(id) => self.forks.get(&id),
        }
    }

    fn parked_mut(&mut self, ctx: Ctx) -> &mut MParked {
        match ctx {
            None => &mut self.local,
            Some(id) => self.forks.get_mut(&id).expect("context exists"),
        }
    }

    /// `St.read`: the running value of `loc` on the active context.
    fn read(&self, loc: Loc) -> u64 {
        let running = &self.running;
        running.inflight.get(&loc).or_else(|| running.accepted.get(&loc)).copied().unwrap_or_else(
            || backing(self.parked(self.active).expect("active context exists").seed, loc),
        )
    }

    /// `load`: loading an account caches its info and slot in the running transaction.
    fn load(&mut self, account: u8) {
        for field in FIELDS {
            let value = self.read((account, field));
            self.running.inflight.entry((account, field)).or_insert(value);
        }
        self.running.tx_loaded.insert(account);
    }

    /// `write`.
    fn write(&mut self, account: u8, field: Field, value: u64) {
        self.load(account);
        self.running.inflight.insert((account, field), value);
    }

    /// `inheritLocal`, mirroring [`Parked::inherit_local`].
    fn inherit_local(target: &mut MParked, local: &MParked) {
        if !local.ran {
            return;
        }
        for account in &local.loaded {
            let value = local.overlay.get(&(*account, Field::Slot)).copied().unwrap_or_default();
            target.overlay.insert((*account, Field::Slot), value);
        }
        target.loaded.extend(&local.loaded);
    }

    /// `mergePersistent`, mirroring [`ForkRegistry::merge_persistent`].
    fn merge_persistent(&self, source: &Overlay, target: &mut Overlay) {
        for account in &self.persistent {
            for field in FIELDS {
                if let Some(value) = source.get(&(*account, field)) {
                    target.insert((*account, field), *value);
                }
            }
        }
    }

    /// `suspend`: accepts the running state into the active context and returns what the running
    /// transaction carries into the next context.
    fn suspend(&mut self) -> (Overlay, BTreeSet<u8>) {
        let running = mem::take(&mut self.running);
        let mut overlay = running.accepted;
        overlay.extend(running.inflight);
        let parked = self.parked_mut(self.active);
        parked.loaded.extend(running.tx_loaded);
        parked.overlay = overlay.clone();
        parked.ran = true;
        (overlay, parked.loaded.clone())
    }

    /// `resume`: installs the active context, carrying persistent accounts over from `source`, and
    /// with `carry` the suspended context's loaded accounts.
    fn resume(&mut self, source: &Overlay, source_loaded: &BTreeSet<u8>, carry: bool) {
        let persistent = self.persistent.clone();
        let parked = self.parked_mut(self.active);
        if carry {
            parked.loaded.extend(source_loaded);
        }
        let mut cache = mem::take(&mut parked.overlay);
        parked.ran = false;
        parked.loaded.extend(source_loaded.intersection(&persistent));
        self.merge_persistent(source, &mut cache);
        self.running = MRunning { accepted: cache, ..Default::default() };
    }

    /// `createFork`.
    fn create_fork(&mut self, seed: u64) -> u64 {
        let id = self.forks.len() as u64;
        let mut parked = MParked::new(seed);
        Self::inherit_local(&mut parked, &self.local);
        self.forks.insert(id, parked);
        id
    }

    /// `selectFork`.
    fn select_fork(&mut self, id: u64) -> bool {
        if !self.forks.contains_key(&id) {
            return false;
        }
        if self.active == Some(id) {
            return true;
        }
        let (source, source_loaded) = self.suspend();
        if self.active.is_none() {
            for parked in self.forks.values_mut() {
                Self::inherit_local(parked, &self.local);
            }
        }
        self.active = Some(id);
        self.resume(&source, &source_loaded, false);
        true
    }

    /// `snapshot`.
    fn snapshot(&mut self) -> u64 {
        let id = self.next_snap;
        self.next_snap += 1;
        let parked = self.parked(self.active).expect("active context exists").clone();
        self.snaps.insert(id, MSnap { active: self.active, parked, running: self.running.clone() });
        id
    }

    /// `revertTo`.
    fn revert_to(&mut self, id: u64, keep: bool) -> bool {
        let snap = if keep { self.snaps.get(&id).cloned() } else { self.snaps.remove(&id) };
        let Some(MSnap { active, parked, running }) = snap else { return false };
        if active != self.active {
            // The running context keeps its accepted state and drops the in-flight changes.
            let accepted = mem::take(&mut self.running.accepted);
            let current = self.parked_mut(self.active);
            current.overlay = accepted;
            current.ran = true;
        }
        *self.parked_mut(active) = parked;
        self.active = active;
        self.running = running;
        true
    }

    /// `deleteSnapshot`.
    fn delete_snapshot(&mut self, id: u64) -> bool {
        self.snaps.remove(&id).is_some()
    }

    /// `rollFork`.
    fn roll_fork(&mut self, id: u64, seed: u64) -> bool {
        if !self.forks.contains_key(&id) {
            return false;
        }
        let mut parked = MParked::new(seed);
        Self::inherit_local(&mut parked, &self.local);
        if self.active != Some(id) {
            self.forks.insert(id, parked);
            return true;
        }
        let (source, source_loaded) = self.suspend();
        self.forks.insert(id, parked);
        self.resume(&source, &source_loaded, true);
        true
    }

    /// `endTx`: commits the running transaction and accepts it into the active context.
    fn end_tx(&mut self) {
        let running = mem::take(&mut self.running);
        let mut overlay = running.accepted;
        overlay.extend(running.inflight);
        let parked = self.parked_mut(self.active);
        parked.overlay = overlay.clone();
        parked.ran = true;
        parked.loaded.clear();
        self.running.accepted = overlay;
    }

    /// `peek`: reads `loc` on the running state. The read caches a value it takes from the backing
    /// database in the accepted state, as `CacheDB` does under `State::account_info_untracked`
    /// and `State::storage_slot_untracked`.
    fn peek(&mut self, loc: Loc) -> u64 {
        let value = self.read(loc);
        if !self.running.inflight.contains_key(&loc) {
            self.running.accepted.entry(loc).or_insert(value);
        }
        value
    }

    /// `step`: applies `op` and returns its result.
    fn step(&mut self, op: Op) -> Option<u64> {
        match op {
            Op::CreateFork(seed) => Some(self.create_fork(seed)),
            Op::SelectFork(id) => self.select_fork(id).then_some(0),
            Op::SetBalance(account, value) => {
                self.write(account, Field::Bal, value);
                Some(0)
            }
            Op::SetSlot(account, value) => {
                self.write(account, Field::Slot, value);
                Some(0)
            }
            Op::MakePersistent(account) => Some(u64::from(self.persistent.insert(account))),
            Op::RevokePersistent(account) => Some(u64::from(self.persistent.remove(&account))),
            Op::Snapshot => Some(self.snapshot()),
            Op::RevertTo(id, keep) => self.revert_to(id, keep).then_some(0),
            Op::DeleteSnapshot(id) => self.delete_snapshot(id).then_some(0),
            Op::RollFork(id, seed) => self.roll_fork(id, seed).then_some(0),
            Op::EndTx => {
                self.end_tx();
                Some(0)
            }
            Op::Peek(account, field) => Some(self.peek((account, field))),
        }
    }
}

/// The system under test: the real lifecycle and its running EVM.
struct Real {
    lifecycle: ExecutionLifecycle<EthEvmFactory>,
    evm: Evm<'static, TypesFor<EthEvmNetwork>>,
    env: EvmEnv<TypesFor<EthEvmNetwork>>,
    snapshots: BTreeMap<u64, U256>,
}

fn database(seed: u64) -> InMemoryDB {
    let mut db = InMemoryDB::default();
    for account in 0..ACCOUNTS {
        let balance = backing(seed, (account, Field::Bal));
        let info = AccountInfo::default().with_balance(U256::from(balance));
        db.insert_account_info(&address(account), info);
        let value = U256::from(backing(seed, (account, Field::Slot)));
        db.insert_account_storage(&address(account), &U256::ZERO, &value);
    }
    db
}

impl Real {
    fn new() -> Self {
        let factory = EthEvmFactory::default();
        let spec = SpecId::PRAGUE;
        let env = EvmEnv::new(spec, factory.version(spec, 31337), Default::default());
        let lifecycle = ExecutionLifecycle::new(factory, database(0), env);
        let evm = lifecycle.create_evm();
        Self { lifecycle, evm, env, snapshots: BTreeMap::new() }
    }

    fn write(&mut self, account: u8, field: Field, value: u64) {
        let address = address(account);
        let value = U256::from(value);
        let state = self.evm.state_mut();
        state.account(&address).unwrap();
        state.storage_slot(&address, U256::ZERO).unwrap();
        match field {
            Field::Bal => state.account(&address).unwrap().set_balance(value),
            Field::Slot => state.storage_slot(&address, U256::ZERO).unwrap().set(value),
        }
    }

    fn step(&mut self, op: Op) -> Option<u64> {
        let phase = ExecutionPhase::InFlight;
        let ok = |result: bool| result.then_some(0);
        match op {
            Op::CreateFork(seed) => {
                let info = ForkInfo::new(ForkId::new("memory", Some(seed)), "memory", Some(seed));
                let id = self.lifecycle.create_fork(info, database(seed), self.env);
                Some(id.to::<u64>())
            }
            Op::SelectFork(id) => {
                ok(self.lifecycle.select_fork(&mut self.evm, Some(U256::from(id)), phase).is_ok())
            }
            Op::SetBalance(account, value) => {
                self.write(account, Field::Bal, value);
                Some(0)
            }
            Op::SetSlot(account, value) => {
                self.write(account, Field::Slot, value);
                Some(0)
            }
            Op::MakePersistent(account) => {
                Some(u64::from(self.lifecycle.forks_mut().add_persistent(address(account))))
            }
            Op::RevokePersistent(account) => {
                Some(u64::from(self.lifecycle.forks_mut().remove_persistent(&address(account))))
            }
            Op::Snapshot => {
                let id = self.lifecycle.snapshot(&mut self.evm);
                let model_id = self.snapshots.len() as u64;
                self.snapshots.insert(model_id, id);
                Some(model_id)
            }
            Op::RevertTo(id, keep) => {
                let Some(snapshot) = self.snapshots.get(&id) else { return None };
                let action = if keep {
                    RevertStateSnapshotAction::RevertKeep
                } else {
                    RevertStateSnapshotAction::RevertRemove
                };
                ok(self.lifecycle.revert(&mut self.evm, *snapshot, action, phase).is_some())
            }
            Op::DeleteSnapshot(id) => {
                let Some(snapshot) = self.snapshots.get(&id) else { return None };
                ok(self.lifecycle.delete_snapshot(*snapshot))
            }
            Op::RollFork(id, seed) => {
                let id = Some(U256::from(id));
                let db = database(seed);
                ok(self.lifecycle.roll_fork(&mut self.evm, id, db, self.env, None, phase).is_ok())
            }
            Op::EndTx => {
                self.evm.state_mut().commit_transaction();
                let evm = mem::replace(&mut self.evm, self.lifecycle.create_evm());
                self.lifecycle.park(evm);
                self.evm = self.lifecycle.create_evm();
                Some(0)
            }
            Op::Peek(account, field) => Some(self.read((account, field))),
        }
    }

    /// Reads `loc` on the running state without loading it.
    fn read(&mut self, (account, field): Loc) -> u64 {
        let state = self.evm.state_mut();
        let value = match field {
            Field::Bal => state.account_info_untracked(&address(account)).unwrap().unwrap().balance,
            Field::Slot => state.storage_slot_untracked(&address(account), &U256::ZERO).unwrap(),
        };
        value.to()
    }

    /// Reads `loc` from the accepted state of the inactive context `ctx`.
    fn read_parked(&self, ctx: Ctx, (account, field): Loc) -> u64 {
        let context = self.lifecycle.forks().context(ctx.map(U256::from)).unwrap();
        let mut db = context.accepted_db();
        let value = match field {
            Field::Bal => db.get_account(&address(account)).unwrap().unwrap().balance,
            Field::Slot => db.get_storage(&address(account), &U256::ZERO).unwrap(),
        };
        value.to()
    }
}

/// Compares everything the model makes observable without reading the running state, which
/// [`Op::Peek`] compares.
fn check(model: &Model, real: &mut Real, trace: &[Op]) {
    let at = || format!("after {trace:?}");
    assert_eq!(real.lifecycle.forks().active().map(|id| id.to::<u64>()), model.active, "{}", at());
    assert_eq!(real.lifecycle.forks().len(), model.forks.len(), "{}", at());
    for account in 0..ACCOUNTS {
        let persistent = real.lifecycle.forks().is_persistent(&address(account));
        assert_eq!(persistent, model.persistent.contains(&account), "{}", at());
    }
    for (id, snapshot) in &real.snapshots {
        let live = real.lifecycle.has_snapshot(*snapshot);
        assert_eq!(live, model.snaps.contains_key(id), "snapshot {id} {}", at());
    }
    for account in 0..ACCOUNTS {
        for field in FIELDS {
            let loc = (account, field);
            let contexts = iter::once(None).chain(model.forks.keys().map(|id| Some(*id)));
            for ctx in contexts.filter(|ctx| *ctx != model.active) {
                let expected = model.parked(ctx).unwrap().read(loc);
                assert_eq!(real.read_parked(ctx, loc), expected, "{ctx:?} {loc:?} {}", at());
            }
        }
    }
}

/// A xorshift generator, so the test needs no dependency and replays deterministically.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn random_op(rng: &mut Rng, model: &Model) -> Op {
    let account = rng.below(u64::from(ACCOUNTS)) as u8;
    let value = 1 + rng.below(5);
    // Ids one past the existing ones exercise the error paths.
    let fork = rng.below(model.forks.len() as u64 + 1);
    let snap = rng.below(model.next_snap + 1);
    match rng.below(17) {
        0 if (model.forks.len() as u64) < MAX_FORKS => Op::CreateFork(1 + rng.below(3)),
        0 | 1 | 2 => Op::SelectFork(fork),
        3 | 4 => Op::SetBalance(account, value),
        5 | 6 => Op::SetSlot(account, value),
        7 => Op::MakePersistent(account),
        8 => Op::RevokePersistent(account),
        9 => Op::Snapshot,
        10 => Op::RevertTo(snap, rng.below(2) == 0),
        11 => Op::DeleteSnapshot(snap),
        12 => Op::RollFork(fork, 4 + rng.below(3)),
        13 => Op::EndTx,
        _ => Op::Peek(account, if rng.below(2) == 0 { Field::Bal } else { Field::Slot }),
    }
}

/// Applies `op` to both and compares its result and everything observable. An operation that
/// installs a context is followed by a peek at every location: resuming a context can cache
/// further locations, such as the accounts whose slot originals `restore_originals` reinstates,
/// which the model does not track, so the test makes every location cached on entry.
fn apply(model: &mut Model, real: &mut Real, trace: &mut Vec<Op>, op: Op) {
    trace.push(op);
    assert_eq!(real.step(op), model.step(op), "result {trace:?}");
    check(model, real, trace);
    if matches!(op, Op::SelectFork(_) | Op::RollFork(..) | Op::RevertTo(..) | Op::EndTx) {
        enter(model, real, trace);
    }
}

/// Peeks at every location of the running state.
fn enter(model: &mut Model, real: &mut Real, trace: &mut Vec<Op>) {
    for account in 0..ACCOUNTS {
        for field in FIELDS {
            let op = Op::Peek(account, field);
            assert_eq!(
                real.step(op),
                model.step(op),
                "peek {:?} after {trace:?}",
                (account, field)
            );
        }
    }
}

/// Runs `ops` on the model and the real lifecycle, comparing them after every step.
fn run(ops: impl IntoIterator<Item = Op>) -> Real {
    let mut model = Model::new();
    let mut real = Real::new();
    let mut trace = Vec::new();
    enter(&mut model, &mut real, &mut trace);
    for op in ops {
        apply(&mut model, &mut real, &mut trace, op);
    }
    real
}

#[test]
fn lifecycle_matches_lean_model() {
    for seed in 1..=5_000u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut model = Model::new();
        let mut real = Real::new();
        let mut trace = Vec::new();
        enter(&mut model, &mut real, &mut trace);
        // Most sequences start with forks, as fork tests do in `setUp`.
        for _ in 0..rng.below(3) {
            apply(&mut model, &mut real, &mut trace, Op::CreateFork(1 + rng.below(3)));
        }
        for _ in 0..32 {
            let op = random_op(&mut rng, &model);
            apply(&mut model, &mut real, &mut trace, op);
        }
    }
}

#[test]
fn persistent_account_follows_fork_switches() {
    run([
        Op::CreateFork(1),
        Op::CreateFork(2),
        Op::MakePersistent(0),
        Op::SetBalance(0, 3),
        Op::SelectFork(0),
        Op::SetBalance(0, 4),
        Op::SetSlot(1, 5),
        Op::SelectFork(1),
        Op::SelectFork(0),
        Op::EndTx,
        Op::SelectFork(1),
    ]);
}

/// Leaving the local context seeds every fork with the slots the running transaction loaded there,
/// each time it leaves it (`selectFork_from_local_seeds_forks`). After a revert to a snapshot of
/// the local context, selecting a fork again overwrites what the transaction wrote on the fork to
/// those slots, although the revert itself left the fork as it was (`revertTo_frame`).
#[test]
fn leaving_local_again_reseeds_forks() {
    let slot = (0, Field::Slot);
    let mut real = run([
        Op::CreateFork(1),
        Op::CreateFork(2),
        Op::SetSlot(0, 3),
        Op::Snapshot,
        Op::SelectFork(0),
        Op::SetSlot(0, 5),
        // Switching away accepts the write on fork 0, so the revert keeps it there.
        Op::SelectFork(1),
        Op::RevertTo(0, true),
    ]);
    assert_eq!(real.read_parked(Some(0), slot), 5);
    real.step(Op::SelectFork(0));
    assert_eq!(real.read(slot), 3);
}

/// A revert to a snapshot of another context drops what the running transaction wrote on the
/// running context since it last entered it, but keeps what an earlier switch away from it
/// accepted (`revertTo_leaves_running`).
#[test]
fn revert_from_another_fork_drops_writes_since_entry() {
    let slot = (0, Field::Slot);
    let setup = [Op::CreateFork(1), Op::CreateFork(2), Op::CreateFork(3), Op::SelectFork(0)];
    let dropped = run(setup.into_iter().chain([
        Op::Snapshot,
        Op::SelectFork(1),
        Op::SetSlot(0, 4),
        Op::RevertTo(0, true),
    ]));
    assert_eq!(dropped.read_parked(Some(1), slot), backing(2, slot));
    let kept = run(setup.into_iter().chain([
        Op::Snapshot,
        Op::SelectFork(1),
        Op::SetSlot(0, 4),
        Op::SelectFork(2),
        Op::SelectFork(1),
        Op::RevertTo(0, true),
    ]));
    assert_eq!(kept.read_parked(Some(1), slot), 4);
}
