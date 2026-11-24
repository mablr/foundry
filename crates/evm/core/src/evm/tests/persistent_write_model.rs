//! Conformance of [`PersistentWrite::apply_all`] with the Lean model in
//! `formal/Lifecycle/PersistentWrite.lean`.
//!
//! [`apply_ref`] mirrors `applyAll` (`Acc.push`, `Acc.result`, `clamp`) and `nonceWrites`. The
//! test applies pseudo-random write lists, with balances and nonces near both bounds, to a real
//! [`State`] and checks the values `apply_all` leaves, the modes of the changes it returns, that
//! replaying those changes over the same pre-state leaves the same values (`applyAll_applied`), and
//! that other writes apply in their recorded order (`applyAccount_separates`).

use super::*;
use alloy_primitives::{U512, aliases::I512};
use evm2::evm::State;

const ACCOUNT: Address = Address::with_last_byte(0xA0);

/// `Acc` in the Lean model, with `I512` for `Int`.
#[derive(Clone, Copy, Default)]
struct Acc {
    set: Option<I512>,
    delta: I512,
    mode: Replay,
}

impl Acc {
    /// `Acc.push`.
    fn push(self, delta: I512, value: I512, replay: Replay) -> Self {
        if replay == Replay::Absolute {
            Self { set: Some(value), delta: I512::ZERO, mode: Replay::Absolute }
        } else {
            Self { set: self.set, delta: self.delta + delta, mode: self.mode.max(replay) }
        }
    }

    /// `Acc.result`.
    fn result(self, max: I512, prev: I512) -> I512 {
        clamp(max, self.set.unwrap_or(prev) + self.delta)
    }
}

/// `clamp`.
fn clamp(max: I512, x: I512) -> I512 {
    if x.is_negative() { I512::ZERO } else { x.min(max) }
}

fn widen(value: U256) -> I512 {
    I512::from_raw(U512::from(value))
}

/// The balance and nonce results of `apply_ref`.
type Applied = (Option<(U256, Replay)>, Option<(u64, Replay)>);

/// `applyAll` for the balance and for the nonce writes that reach the accumulator, returning each
/// value and mode if the account has writes of the kind.
fn apply_ref(balance: U256, nonce: u64, writes: &[PersistentWrite]) -> Applied {
    let (mut balances, mut nonces) = (None::<Acc>, None::<Acc>);
    for write in writes {
        match write {
            PersistentWrite::Balance { delta, balance, replay, .. } => {
                let acc = balances.unwrap_or_default();
                balances = Some(acc.push(*delta, widen(*balance), *replay));
            }
            // `nonceWrites`: the account is never created here.
            PersistentWrite::Nonce { delta, nonce, created: false, replay, .. } => {
                let acc = nonces.unwrap_or_default();
                let value = widen(U256::from(*nonce));
                nonces = Some(acc.push(I512::try_from(*delta).unwrap(), value, *replay));
            }
            _ => {}
        }
    }
    let balance = balances.map(|acc| {
        let value = acc.result(widen(U256::MAX), widen(balance));
        (U256::from(value.into_raw()), acc.mode)
    });
    let nonce = nonces.map(|acc| {
        let value = acc.result(widen(U256::from(u64::MAX)), widen(U256::from(nonce)));
        (u64::try_from(value.into_raw()).unwrap(), acc.mode)
    });
    (balance, nonce)
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

fn replay(rng: &mut Rng) -> Replay {
    [Replay::Delta, Replay::Set, Replay::Absolute][rng.below(3) as usize]
}

/// A balance near zero or near the bound.
fn balance_value(rng: &mut Rng) -> U256 {
    let small = U256::from(rng.below(8));
    if rng.below(2) == 0 { small } else { U256::MAX - small }
}

fn nonce_value(rng: &mut Rng) -> u64 {
    let small = rng.below(8);
    if rng.below(2) == 0 { small } else { u64::MAX - small }
}

fn random_write(rng: &mut Rng) -> PersistentWrite {
    match rng.below(4) {
        0 => {
            let (previous, balance) = (balance_value(rng), balance_value(rng));
            PersistentWrite::balance(ACCOUNT, previous, balance).with_replay(replay(rng))
        }
        1 | 2 => {
            let (previous, nonce) = (nonce_value(rng), nonce_value(rng));
            PersistentWrite::Nonce {
                address: ACCOUNT,
                delta: i128::from(nonce) - i128::from(previous),
                nonce,
                // A creation the account no longer holds drops the write.
                created: rng.below(4) == 0,
                replay: replay(rng),
            }
        }
        _ => PersistentWrite::Slot(ACCOUNT, U256::ZERO, U256::from(rng.below(8))),
    }
}

fn state(balance: U256, nonce: u64) -> State<'static> {
    let mut db = InMemoryDB::default();
    db.insert_account_info(
        &ACCOUNT,
        AccountInfo::default().with_balance(balance).with_nonce(nonce),
    );
    State::new(db)
}

fn read(state: &mut State<'_>) -> (U256, u64, U256) {
    let info = state.account_info_untracked(&ACCOUNT).unwrap().unwrap();
    (info.balance, info.nonce, state.storage_slot_untracked(&ACCOUNT, &U256::ZERO).unwrap())
}

#[test]
fn persistent_writes_match_lean_model() {
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    for _ in 0..20_000 {
        let (balance, nonce) = (balance_value(&mut rng), nonce_value(&mut rng));
        let writes = (0..1 + rng.below(6)).map(|_| random_write(&mut rng)).collect::<Vec<_>>();

        let mut real = state(balance, nonce);
        let applied = PersistentWrite::apply_all(&writes, &mut real).unwrap();
        let (real_balance, real_nonce, real_slot) = read(&mut real);

        let (expected_balance, expected_nonce) = apply_ref(balance, nonce, &writes);
        let context = format!("{balance} {nonce} {writes:?}");
        assert_eq!(real_balance, expected_balance.map_or(balance, |(v, _)| v), "{context}");
        assert_eq!(real_nonce, expected_nonce.map_or(nonce, |(v, _)| v), "{context}");
        // `applyAccount_separates`: other writes apply in their recorded order.
        let slot = writes.iter().rev().find_map(|write| match write {
            PersistentWrite::Slot(_, _, value) => Some(*value),
            _ => None,
        });
        assert_eq!(real_slot, slot.unwrap_or_default(), "{context}");

        // `accOf_mode`: each returned change takes the highest mode it combines.
        let mut modes = (None, None);
        for change in &applied {
            match change {
                PersistentWrite::Balance { replay, balance, .. } => {
                    modes.0 = Some((*balance, *replay));
                }
                PersistentWrite::Nonce { replay, nonce, .. } => modes.1 = Some((*nonce, *replay)),
                write => panic!("unexpected change {write:?}"),
            }
        }
        assert_eq!(modes, (expected_balance, expected_nonce), "{context}");

        // `applyAll_applied`: replaying the changes over the same pre-state keeps the values.
        let mut replayed = state(balance, nonce);
        PersistentWrite::apply_all(&applied, &mut replayed).unwrap();
        let (replayed_balance, replayed_nonce, _) = read(&mut replayed);
        assert_eq!((replayed_balance, replayed_nonce), (real_balance, real_nonce), "{context}");
    }
}
