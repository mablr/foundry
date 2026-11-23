/-!
# Persistent writes

A model of `PersistentWrite::apply_all` (`crates/evm/core/src/evm/lifecycle.rs`): cheatcode state
edits that survive reverts of the enclosing calls are replayed after each revert. Balance and
nonce writes replay as deltas: the deltas of each account since its last absolute write are
summed and saturated once, and the call returns the combined changes, which the caller records in
place of the writes it applied.

Fidelity:

* `I512` balance deltas and `i128` nonce deltas are `Int`; `U256` balances and `u64` nonces are
  `Nat` below a bound `max` (`2^256 - 1` or `2^64 - 1`). `U256::saturating_from` of a non-negative
  `I512` and `u64::try_from(..).unwrap_or(u64::MAX)` of a non-negative `i128` are both `min · max`,
  so balances and nonces share one model (`clamp`). The sums cannot overflow `I512` or `i128` for
  fewer than `2^255` (`2^63`) writes, which this model assumes.
* Accounts are independent (`AddressMap`), so the theorems are stated for one account's writes.
  The nonce filter (`!created || is_created()`) drops writes before they reach the accumulator,
  which `nonceWrites` models.
* Code, wipe, slot, destruct and reload writes are `other` writes on fields balance and nonce
  writes never touch.
-/

namespace Lifecycle.PersistentWrite

/-- How a balance or nonce write replays, ordered by precedence. Models `Replay`. -/
inductive Replay where
  | delta
  | set
  | absolute
  deriving DecidableEq, Repr

/-- The precedence of a mode, as `Replay`'s derived `Ord`. -/
def Replay.rank : Replay → Nat
  | .delta => 0
  | .set => 1
  | .absolute => 2

/-- `Ord::max` of two modes. -/
def Replay.max (a b : Replay) : Replay := if a.rank ≤ b.rank then b else a

/-- A balance or nonce write of one account: `PersistentWrite::Balance` or `Nonce`. -/
structure W where
  /-- The change it made. -/
  delta : Int
  /-- The value it left. -/
  value : Nat
  /-- How it replays. -/
  replay : Replay

/-- The accumulator `apply_all` keeps per account: the value of the last absolute write, the
deltas since, and the combined mode. Its default is `(None, 0, Replay::Delta)`. -/
structure Acc where
  set : Option Nat
  delta : Int
  mode : Replay

/-- The empty accumulator, `Default::default()`. -/
def Acc.empty : Acc := ⟨none, 0, .delta⟩

/-- Adds a write to the accumulator: the body of `apply_all`'s first loop. -/
def Acc.push (acc : Acc) (w : W) : Acc :=
  if w.replay = .absolute then ⟨some w.value, 0, .absolute⟩
  else ⟨acc.set, acc.delta + w.delta, acc.mode.max w.replay⟩

/-- The accumulator of a list of writes. -/
def accOf (ws : List W) : Acc := ws.foldl Acc.push Acc.empty

/-- Saturates `x` into `[0, max]`. -/
def clamp (max : Nat) (x : Int) : Nat := if x < 0 then 0 else min x.toNat max

/-- The value the combined write leaves over `prev`: `apply_all`'s second and third loops. -/
def Acc.result (max prev : Nat) (acc : Acc) : Nat :=
  clamp max ((acc.set.getD prev : Int) + acc.delta)

/-- The change `apply_all` returns in place of the writes: `PersistentWrite::balance(address,
previous, balance).with_replay(replay)`. -/
def applied (max prev : Nat) (ws : List W) : W :=
  let acc := accOf ws
  let v := acc.result max prev
  ⟨(v : Int) - prev, v, acc.mode⟩

/-- `apply_all` on one account's balance (or nonce) writes over `prev`. -/
def applyAll (max prev : Nat) (ws : List W) : Nat := (accOf ws).result max prev

/-! ## Range -/

theorem clamp_le (max : Nat) (x : Int) : clamp max x ≤ max := by
  unfold clamp; split <;> omega

theorem clamp_of_mem {max : Nat} {x : Int} (h0 : 0 ≤ x) (h1 : x ≤ max) : (clamp max x : Int) = x := by
  unfold clamp; split <;> omega

/-- **The result is in range**: never negative and never above the bound. -/
theorem applyAll_le (max prev : Nat) (ws : List W) : applyAll max prev ws ≤ max :=
  clamp_le _ _

/-! ## Absolute writes -/

theorem push_absolute (acc : Acc) (w : W) (h : w.replay = .absolute) :
    acc.push w = ⟨some w.value, 0, .absolute⟩ := by
  simp [Acc.push, h]

/-- **An absolute write resets the account**: the writes before the last absolute one do not
matter. -/
theorem accOf_absolute (xs ys : List W) (w : W) (h : w.replay = .absolute) :
    accOf (xs ++ w :: ys) = accOf (w :: ys) := by
  simp [accOf, List.foldl_append, List.foldl_cons, push_absolute _ _ h]

/-- An absolute write followed by deltas sets its value plus the deltas. -/
theorem applyAll_absolute (max prev : Nat) (w : W) (h : w.replay = .absolute) (ys : List W)
    (hy : ∀ y ∈ ys, y.replay ≠ .absolute) :
    applyAll max prev (w :: ys) = clamp max (w.value + (ys.map W.delta).sum) := by
  suffices ∀ (acc : Acc) (zs : List W), (∀ z ∈ zs, z.replay ≠ .absolute) →
      (zs.foldl Acc.push acc).set = acc.set ∧
      (zs.foldl Acc.push acc).delta = acc.delta + (zs.map W.delta).sum by
    have := this ⟨some w.value, 0, .absolute⟩ ys hy
    simp [applyAll, Acc.result, accOf, List.foldl_cons, push_absolute _ _ h, this.1, this.2]
  intro acc zs hz
  induction zs generalizing acc with
  | nil => simp
  | cons z zs ih =>
    have hz' : z.replay ≠ .absolute := hz z (by simp)
    have := ih (acc.push z) (fun y hy => hz y (by simp [hy]))
    simp only [List.foldl_cons, List.map_cons, List.sum_cons]
    rw [this.1, this.2]
    simp only [Acc.push, hz', if_false]
    constructor
    · trivial
    · omega

/-! ## Modes -/

theorem rank_max (a b : Replay) : (a.max b).rank = Nat.max a.rank b.rank := by
  cases a <;> cases b <;> rfl

theorem max_absolute (a : Replay) : a.max .absolute = .absolute := by cases a <;> rfl

theorem absolute_max (a : Replay) : Replay.absolute.max a = .absolute := by cases a <;> rfl

/-- **The combined change takes the highest mode of the writes it combines**, so a change that
includes an absolute write keeps setting the value it left. -/
theorem accOf_mode (ws : List W) :
    (accOf ws).mode = ws.foldl (fun m w => m.max w.replay) .delta := by
  suffices ∀ acc : Acc, (ws.foldl Acc.push acc).mode =
      ws.foldl (fun m w => m.max w.replay) acc.mode from this _
  induction ws with
  | nil => intro; rfl
  | cons w ws ih =>
    intro acc
    simp only [List.foldl_cons]
    rw [ih]
    congr 1
    by_cases h : w.replay = .absolute
    · simp [Acc.push, h, max_absolute]
    · simp [Acc.push, h]

/-! ## Replays -/

theorem applyAll_single (max prev v : Nat) (m : Replay) (hv : v ≤ max) :
    applyAll max prev [⟨(v : Int) - prev, v, m⟩] = v := by
  simp only [applyAll, accOf, List.foldl_cons, List.foldl_nil, Acc.push, Acc.empty]
  split
  · simp only [Acc.result, Option.getD_some]
    have := clamp_of_mem (max := max) (x := (v : Int) + 0) (by omega) (by omega)
    omega
  · simp only [Acc.result, Option.getD_none]
    have := clamp_of_mem (max := max) (x := (prev : Int) + (0 + ((v : Int) - prev)))
      (by omega) (by omega)
    omega

/-- **A later replay keeps the saturation**: replaying the returned change over the same
pre-state leaves the same value, whatever its mode. -/
theorem applyAll_applied (max prev : Nat) (ws : List W) :
    applyAll max prev [applied max prev ws] = applyAll max prev ws :=
  applyAll_single _ _ _ _ (applyAll_le _ _ _)

/-- Applying writes one at a time, saturating after each: what `apply_all` avoids. -/
def applySeq (max prev : Nat) (ws : List W) : Nat :=
  ws.foldl (fun x w => applyAll max x [w]) prev

/-- **Without saturation, combining equals applying one by one.** If every write applied one at
a time stays in range, `apply_all` agrees with sequential application. -/
theorem applyAll_eq_applySeq (max prev : Nat) (ws : List W) (hprev : prev ≤ max)
    (hin : ∀ n, n ≤ ws.length → ∀ x : Int,
      x = ((ws.take n).foldl (fun (x : Int) (w : W) =>
        if w.replay = .absolute then (w.value : Int) else x + w.delta) prev) → 0 ≤ x ∧ x ≤ max) :
    applyAll max prev ws = applySeq max prev ws := by
  -- The unsaturated running value.
  let step := fun (x : Int) (w : W) => if w.replay = .absolute then (w.value : Int) else x + w.delta
  -- Both sides equal the clamped unsaturated value; the accumulator tracks it exactly.
  have acc_val : ∀ (zs : List W) (acc : Acc) (p : Nat),
      ((zs.foldl Acc.push acc).set.getD p : Int) + (zs.foldl Acc.push acc).delta =
        zs.foldl step ((acc.set.getD p : Int) + acc.delta) := by
    intro zs
    induction zs with
    | nil => intro acc p; rfl
    | cons z zs ih =>
      intro acc p
      simp only [List.foldl_cons]
      rw [ih]
      congr 1
      by_cases h : z.replay = .absolute
      · simp [Acc.push, h, step]
      · simp [Acc.push, h, step]; omega
  have seq_val : ∀ (n : Nat), n ≤ ws.length →
      ((((ws.take n).foldl (fun (x : Nat) w => applyAll max x [w]) prev : Nat)) : Int) =
        (ws.take n).foldl step prev := by
    intro n
    induction n with
    | zero => intro; simp
    | succ n ih =>
      intro hn
      have hlt : n < ws.length := by omega
      rw [List.take_succ_eq_append_getElem hlt, List.foldl_append, List.foldl_append]
      simp only [List.foldl_cons, List.foldl_nil]
      rw [← ih (by omega)]
      generalize hx : (ws.take n).foldl (fun x w => applyAll max x [w]) prev = x
      have hr := hin (n + 1) hn
      rw [List.take_succ_eq_append_getElem hlt, List.foldl_append] at hr
      simp only [List.foldl_cons, List.foldl_nil] at hr
      rw [← ih (by omega), hx] at hr
      have hr := hr _ rfl
      simp only [applyAll, accOf, List.foldl_cons, List.foldl_nil, Acc.result]
      by_cases h : ws[n].replay = .absolute
      · simp only [Acc.push, h, if_true, Option.getD_some, step] at hr ⊢
        exact clamp_of_mem (by omega) (by omega)
      · simp only [Acc.push, h, if_false, Acc.empty, Option.getD_none, step] at hr ⊢
        have : (x : Int) + (0 + ws[n].delta) = x + ws[n].delta := by omega
        rw [this]; exact clamp_of_mem hr.1 hr.2
  have hs := seq_val ws.length (Nat.le_refl _)
  rw [List.take_length] at hs
  have hr := hin ws.length (Nat.le_refl _) _ rfl
  rw [List.take_length] at hr
  have ha := acc_val ws Acc.empty prev
  have h0 : ((Acc.empty.set.getD prev : Nat) : Int) + Acc.empty.delta = prev := by
    simp [Acc.empty]
  rw [h0] at ha
  have : (applyAll max prev ws : Int) = ws.foldl step prev := by
    simp only [applyAll, accOf, Acc.result]
    rw [ha]; exact clamp_of_mem hr.1 hr.2
  unfold applySeq
  omega

/-- **Saturation happens once**: a withdrawal that a later deposit covers does not saturate, as
it would one write at a time. -/
example : applyAll 100 3 [⟨-5, 0, .delta⟩, ⟨5, 5, .delta⟩] = 3 ∧
    applySeq 100 3 [⟨-5, 0, .delta⟩, ⟨5, 5, .delta⟩] = 5 := by decide

/-! ## Separation from other writes -/

/-- A write `apply_all` applies: a balance write, a nonce write, or another write that applies
in order, on a field balance and nonce writes do not touch. -/
inductive Any where
  | balance (w : W)
  | nonce (w : W) (created : Bool)
  | other (field value : Nat)

/-- An account: balance, nonce, whether the running transaction created it, other fields. -/
structure Account where
  balance : Nat
  nonce : Nat
  created : Bool
  other : Nat → Nat

/-- The balance writes, in order. -/
def balanceWrites : List Any → List W
  | [] => []
  | .balance w :: ws => w :: balanceWrites ws
  | _ :: ws => balanceWrites ws

/-- The nonce writes that reach the accumulator: those made on an account the running
transaction had not created, or on one it still holds created. -/
def nonceWrites (created : Bool) : List Any → List W
  | [] => []
  | .nonce w c :: ws => if !c || created then w :: nonceWrites created ws else nonceWrites created ws
  | _ :: ws => nonceWrites created ws

/-- The other writes, applied in order. -/
def applyOthers (other : Nat → Nat) : List Any → Nat → Nat
  | [], f => other f
  | .other g v :: ws, f => if f = g ∧ (ws.all fun | .other h _ => h ≠ g | _ => true) then v
    else applyOthers other ws f
  | _ :: ws, f => applyOthers other ws f

/-- `apply_all` on one account. -/
def applyAccount (maxBalance maxNonce : Nat) (acct : Account) (ws : List Any) : Account :=
  { acct with
    balance := applyAll maxBalance acct.balance (balanceWrites ws)
    nonce := applyAll maxNonce acct.nonce (nonceWrites acct.created ws)
    other := applyOthers acct.other ws }

/-- **Deferring balance and nonce writes is sound**: each field of the result depends only on the
writes of its kind, in their recorded order, so applying the other writes first does not change
the outcome of any interleaving. -/
theorem applyAccount_separates (mb mn : Nat) (acct : Account) (ws : List Any) :
    (applyAccount mb mn acct ws).balance = applyAll mb acct.balance (balanceWrites ws) ∧
    (applyAccount mb mn acct ws).nonce = applyAll mn acct.nonce (nonceWrites acct.created ws) ∧
    (applyAccount mb mn acct ws).created = acct.created := ⟨rfl, rfl, rfl⟩

end Lifecycle.PersistentWrite
