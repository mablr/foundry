//! Conformance of [`SnapshotRestores`] with the Lean model in `formal/Lifecycle/Restores.lean`.
//!
//! [`Model`] mirrors `R` and its operations (`frameStart`, `capture`, `acceptWrites`,
//! `frameEnd`, `reinstate`, `restoreEarlier`) one-to-one. The test drives the real bookkeeping
//! the way the cheatcode inspector does, with pseudo-random sequences of frames, persistent
//! writes, snapshot restorations and fork switches, and compares after every step the retained
//! states, the running ledger, which state a failing frame reinstates and whether a restoration
//! survived the outermost frame. It also checks `R.Inv.bound`: retained states never outnumber
//! open frames.

use super::*;
use crate::evm::Replay;
use alloy_primitives::{Address, U256};
use evm2::evm::AccountInfo;

/// The account whose cached balance identifies a running state.
const MARK: Address = Address::with_last_byte(0xEE);

/// `Entry`: the depth, the journal length (`None` for `usize::MAX`) and the write, identified by
/// its slot value and whether it was reinstated.
type Entry = (usize, Option<usize>, (u64, bool));

#[derive(Clone, Debug, PartialEq)]
struct Later {
    dropped: Vec<Entry>,
    since: usize,
}

/// `Retained`, with the state's mark.
#[derive(Clone, Debug, PartialEq)]
struct Retained {
    state: u64,
    ledger: Vec<Entry>,
    later: Option<Later>,
    earlier: Vec<Option<(usize, usize)>>,
}

/// `R`, with frames innermost last like the Rust `Vec`.
#[derive(Clone, Debug, Default)]
struct Model {
    frames: Vec<(u16, usize)>,
    replaced: Vec<Retained>,
    kept: bool,
}

impl Model {
    /// `R.frameStart`.
    fn frame_start(&mut self, depth: u16) {
        self.frames.push((depth, self.replaced.len()));
    }

    /// `R.first`.
    fn first(&self) -> bool {
        self.frames.last().is_some_and(|(_, len)| *len == self.replaced.len())
    }

    /// `R.capture`.
    fn capture(&mut self, state: u64, ledger: &[Entry], len: usize) {
        let len = len.min(ledger.len());
        let first = self.first();
        let earlier = if first {
            self.replaced
                .iter()
                .map(|x| x.later.as_ref().map(|l| (l.dropped.len(), l.since)))
                .collect()
        } else {
            Vec::new()
        };
        for x in &mut self.replaced {
            if let Some(l) = &mut x.later {
                l.dropped.extend(ledger.iter().skip(l.since).copied());
                l.since = len;
            }
        }
        if first {
            let later = Some(Later { dropped: Vec::new(), since: len });
            self.replaced.push(Retained { state, ledger: ledger.to_vec(), later, earlier });
        }
    }

    /// `R.acceptWrites`.
    fn accept_writes(&mut self) {
        for x in &mut self.replaced {
            x.later = None;
            x.earlier.iter_mut().for_each(|e| *e = None);
        }
    }

    /// `reinstate`, with `restoreEarlier`.
    fn reinstate(
        mut kept: Vec<Retained>,
        ledger: &[Entry],
    ) -> Option<(Vec<Retained>, Vec<Entry>, u64)> {
        let x = kept.pop()?;
        let later = x.later.map_or_else(Vec::new, |l| {
            l.dropped
                .into_iter()
                .chain(ledger.iter().skip(l.since).copied())
                .map(|(depth, _, (value, _))| (depth, None, (value, true)))
                .collect()
        });
        for (retained, earlier) in kept.iter_mut().zip(x.earlier) {
            retained.later = match (retained.later.take(), earlier) {
                (Some(mut l), Some((n, since))) => {
                    l.dropped.truncate(n);
                    l.since = since;
                    Some(l)
                }
                _ => None,
            };
        }
        Some((kept, [x.ledger, later].concat(), x.state))
    }

    /// `R.frameEnd`: returns the ledger and reinstated state if a frame at `depth` was open.
    fn frame_end(
        &mut self,
        ledger: &[Entry],
        depth: u16,
        failed: bool,
    ) -> Option<(Vec<Entry>, Option<u64>)> {
        while self.frames.last().is_some_and(|(d, _)| *d > depth) {
            self.frames.pop();
        }
        let &(d, len) = self.frames.last()?;
        if d != depth {
            return None;
        }
        self.frames.pop();
        let (replaced, ledger, state) = if failed && self.replaced.len() > len {
            let kept = self.replaced.iter().take(len + 1).cloned().collect();
            match Self::reinstate(kept, ledger) {
                Some((replaced, ledger, state)) => (replaced, ledger, Some(state)),
                None => (self.replaced.clone(), ledger.to_vec(), None),
            }
        } else {
            (self.replaced.clone(), ledger.to_vec(), None)
        };
        self.replaced = replaced;
        match self.frames.last() {
            Some((_, caller)) => self.replaced.truncate(caller + 1),
            None => {
                self.kept |= !self.replaced.is_empty();
                self.replaced.clear();
            }
        }
        Some((ledger, state))
    }
}

/// Encodes a model entry as a persistent write, and back.
fn write(entry: &Entry) -> (usize, usize, PersistentWrite) {
    let (depth, len, (value, reinstated)) = *entry;
    // Writes replay as `Set`, which reinstating turns into `Absolute`.
    let replay = if reinstated { Replay::Absolute } else { Replay::Set };
    let write = PersistentWrite::balance(MARK, U256::ZERO, U256::from(value)).with_replay(replay);
    (depth, len.unwrap_or(usize::MAX), write)
}

fn entry((depth, len, write): &(usize, usize, PersistentWrite)) -> Entry {
    let PersistentWrite::Balance { balance, replay, .. } = write else { unreachable!() };
    let len = (*len != usize::MAX).then_some(*len);
    (*depth, len, (balance.to(), *replay == Replay::Absolute))
}

fn marked(mark: u64) -> State<'static> {
    let mut state = State::new(EmptyDB::default());
    let info = AccountInfo::default().with_balance(U256::from(mark));
    state.overlay_db_mut().cache.accounts.insert(MARK, Some(info));
    state
}

fn mark(state: &State<'_>) -> u64 {
    state.overlay_db().cache.accounts[&MARK].as_ref().unwrap().balance.to()
}

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

#[test]
fn snapshot_restores_match_lean_model() {
    let features = EvmFeatures::default();
    // Failing frames that reinstated a state with writes to replay after the restoration.
    let mut replays = 0;
    for seed in 1..=20_000u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let (mut real, mut model) = (SnapshotRestores::default(), Model::default());
        let mut state = marked(0);
        let mut ledger = Vec::<(usize, usize, PersistentWrite)>::new();
        let mut next = 1u64;
        let mut trace = Vec::new();
        for _ in 0..24 {
            let depth = model.frames.len() as u16;
            let op = rng.below(8);
            trace.push(op);
            match op {
                // A call opens a frame.
                0 | 1 => {
                    real.frame_start(depth, state.checkpoint());
                    model.frame_start(depth);
                }
                // A cheatcode records a persistent write.
                2 | 3 => {
                    ledger.push(write(&(usize::from(depth), Some(0), (next, false))));
                    next += 1;
                }
                // `vm.revertToState` replaces the running state.
                4 => {
                    let len = rng.below(ledger.len() as u64 + 2) as usize;
                    let switched = rng.below(4) == 0;
                    real.capture(state.snapshot(), &ledger, len);
                    let model_ledger = ledger.iter().map(entry).collect::<Vec<_>>();
                    model.capture(mark(&state), &model_ledger, len);
                    if switched {
                        real.accept_writes();
                        model.accept_writes();
                    }
                    ledger.truncate(len);
                    state = marked(next);
                    next += 1;
                }
                // A fork switch accepts the running state.
                5 => {
                    ledger.clear();
                    real.accept_writes();
                    model.accept_writes();
                }
                // The innermost frame ends, or an outer one after the inner ones aborted.
                _ if depth > 0 => {
                    let aborted = rng.below(6) == 0;
                    let depth =
                        if aborted { rng.below(u64::from(depth)) as u16 } else { depth - 1 };
                    let failed = rng.below(2) == 0;
                    let model_ledger = ledger.iter().map(entry).collect::<Vec<_>>();
                    let expected = model.frame_end(&model_ledger, depth, failed);
                    let ended = real.frame_end(&mut state, &mut ledger, depth, failed, features);
                    assert_eq!(ended.is_some(), expected.is_some(), "{trace:?}");
                    if let Some((expected_ledger, reinstated)) = expected {
                        if let Some(reinstated) = reinstated {
                            assert_eq!(mark(&state), reinstated, "{trace:?}");
                            replays += usize::from(expected_ledger.iter().any(|e| e.2.1));
                        }
                        let real_ledger = ledger.iter().map(entry).collect::<Vec<_>>();
                        assert_eq!(real_ledger, expected_ledger, "{trace:?}");
                    }
                }
                _ => {}
            }
            assert_eq!(real.frames.len(), model.frames.len(), "{trace:?}");
            assert_eq!(real.replaced.len(), model.replaced.len(), "{trace:?}");
            assert_eq!(real.kept, model.kept, "{trace:?}");
            for (real, model) in real.replaced.iter().zip(&model.replaced) {
                assert_eq!(real.ledger.iter().map(entry).collect::<Vec<_>>(), model.ledger);
                assert_eq!(real.earlier, model.earlier, "{trace:?}");
                let later = real.later.as_ref().map(|l| Later {
                    dropped: l.dropped.iter().map(entry).collect(),
                    since: l.since,
                });
                assert_eq!(later, model.later, "{trace:?}");
            }
            // `R.Inv.bound`, away from frames that aborted without finding their own.
            assert!(model.replaced.len() <= model.frames.len() + 1, "{trace:?}");
        }
    }
    assert!(replays > 1_000, "{replays}");
}
